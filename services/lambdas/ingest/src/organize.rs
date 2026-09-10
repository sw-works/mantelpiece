//! Per-household task: bursts and keepers, scores, clock alignment, trips.
//!
//! Idempotent (F-106): re-running over the same media produces the same groups and
//! updates the same trips in place. Trips are matched to existing ones by date overlap.

use memorylane_core::events::EventKind;
use memorylane_core::ids::{ContributorId, HouseholdId, MediaId, TripId};
use memorylane_core::media::{MediaItem, ProcessingState};
use memorylane_core::trip::{Trip, TripDay};
use memorylane_ingest::burst::{self, Candidate};
use memorylane_ingest::quality;
use memorylane_trips::{clock, cluster, highlights, naming, route, Sample};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use time::{OffsetDateTime, UtcOffset};

use crate::{Ingest, Result};

pub const HIGHLIGHTS_PER_TRIP: usize = 12;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub media: usize,
    pub groups: usize,
    pub keepers: Vec<MediaId>,
    pub trips: Vec<TripId>,
    pub new_trips: usize,
}

pub async fn run(ingest: &Ingest, household: &HouseholdId) -> Result<Output> {
    let mut media: Vec<MediaItem> = ingest
        .storage
        .list_media(household)
        .await?
        .into_iter()
        .filter(|m| {
            matches!(
                m.state,
                ProcessingState::Signalled | ProcessingState::Enriched
            )
        })
        .collect();

    // 1. Bursts and keepers, across contributors.
    let candidates = candidates(&media);
    let groups = burst::group(&candidates, burst::Params::default());
    assign_groups(&mut media, &groups);

    // 2. Samples, clock alignment, clustering.
    let mut samples = to_samples(&media);
    let offsets = match clock::reference(&samples) {
        Some(reference) => clock::estimate(&samples, &reference, clock::Params::default()),
        None => vec![],
    };
    clock::apply(&mut samples, &offsets);
    let clustering = cluster::cluster(&samples, cluster::Params::default());
    let sample_by_id: HashMap<MediaId, &Sample> = clustering
        .sorted
        .iter()
        .map(|s| (s.id.clone(), s))
        .collect();

    // 3. Trips.
    let existing = ingest.storage.list_trips(household).await?;
    let now = ingest.now();
    let mut trip_ids = Vec::new();
    let mut new_trips = 0;
    let pinned: Vec<MediaId> = media
        .iter()
        .filter(|m| m.pinned)
        .map(|m| m.id.clone())
        .collect();
    let hidden: Vec<MediaId> = media
        .iter()
        .filter(|m| m.hidden)
        .map(|m| m.id.clone())
        .collect();

    for c in clustering.clusters.iter().filter(|c| c.is_trip) {
        let members: Vec<&Sample> = c
            .sample_indices
            .iter()
            .map(|&i| &clustering.sorted[i])
            .collect();
        let start = members.first().map_or(now, |s| s.captured);
        let end = members.last().map_or(now, |s| s.captured);
        let previous = existing.iter().find(|t| t.start <= end && start <= t.end);
        let id = previous.map_or_else(TripId::random, |t| t.id.clone());
        if previous.is_none() {
            new_trips += 1;
        }

        let member_samples: Vec<Sample> = members.iter().map(|s| (*s).clone()).collect();
        let (trip, new_media, new_contributors) = build_trip(BuildTrip {
            household,
            id: id.clone(),
            start,
            end,
            days: &c.days,
            members: &member_samples,
            sample_by_id: &sample_by_id,
            previous,
            pinned: &pinned,
            hidden: &hidden,
            offsets: &offsets,
        });
        ingest.storage.put_trip(&trip).await?;
        for s in &members {
            if let Some(m) = media.iter_mut().find(|m| m.id == s.id) {
                m.trip = Some(id.clone());
            }
        }

        if new_media > 0 {
            announce(
                ingest,
                household,
                &id,
                previous.is_some(),
                new_media,
                &new_contributors,
                &member_samples,
            )
            .await?;
        }
        trip_ids.push(id);
    }

    for m in &media {
        ingest.storage.put_media(m).await?;
    }

    Ok(Output {
        media: media.len(),
        groups: groups.len(),
        keepers: groups.iter().map(|g| g.keeper.clone()).collect(),
        trips: trip_ids,
        new_trips,
    })
}

fn candidates(media: &[MediaItem]) -> Vec<Candidate> {
    media
        .iter()
        .filter_map(|m| {
            Some(Candidate {
                id: m.id.clone(),
                captured: m.sort_instant(),
                phash: m.signals.phash?,
                sharpness: m.signals.sharpness.unwrap_or(0.0),
                exposure: m.signals.exposure.unwrap_or(0.5),
            })
        })
        .collect()
}

fn assign_groups(media: &mut [MediaItem], groups: &[burst::BurstGroup]) {
    let mut group_of: HashMap<MediaId, (String, usize, MediaId)> = HashMap::new();
    for g in groups {
        for m in &g.members {
            group_of.insert(m.clone(), (g.id.clone(), g.members.len(), g.keeper.clone()));
        }
    }
    let by_id: HashMap<MediaId, MediaItem> =
        media.iter().map(|m| (m.id.clone(), m.clone())).collect();
    for m in media.iter_mut() {
        let Some((gid, size, keeper)) = group_of.get(&m.id) else {
            // No hash (video, failed decode): its own keeper, middling uniqueness.
            m.burst_group = None;
            m.is_keeper = true;
            m.quality_score = Some(quality::score(&m.signals, &m.enrichment, 0.5));
            continue;
        };
        m.burst_group = Some(gid.clone());
        m.is_keeper = &m.id == keeper;
        if !m.is_keeper
            && m.enrichment.inherited_from.is_none()
            && m.enrichment.scene_tags.is_empty()
        {
            if let Some(k) = by_id
                .get(keeper)
                .filter(|k| k.state == ProcessingState::Enriched)
            {
                m.enrichment = k.enrichment.clone();
                m.enrichment.inherited_from = Some(keeper.clone());
            }
        }
        let uniqueness = 1.0 / *size as f32;
        m.quality_score = Some(quality::score(&m.signals, &m.enrichment, uniqueness));
    }
}

fn to_samples(media: &[MediaItem]) -> Vec<Sample> {
    media
        .iter()
        .map(|m| Sample {
            id: m.id.clone(),
            contributor: m.contributor.clone(),
            captured: m.sort_instant(),
            offset: m
                .captured
                .and_then(|c| c.offset_seconds)
                .and_then(|s| UtcOffset::from_whole_seconds(s).ok()),
            gps: m.gps,
            score: m.quality_score.unwrap_or(0.0),
            burst_group: m.burst_group.clone(),
            landmark: m.enrichment.landmark.as_ref().map(|t| t.name.clone()),
        })
        .collect()
}

struct BuildTrip<'a> {
    household: &'a HouseholdId,
    id: TripId,
    start: OffsetDateTime,
    end: OffsetDateTime,
    days: &'a [cluster::Day],
    members: &'a [Sample],
    sample_by_id: &'a HashMap<MediaId, &'a Sample>,
    previous: Option<&'a Trip>,
    pinned: &'a [MediaId],
    hidden: &'a [MediaId],
    offsets: &'a [memorylane_core::trip::ClockOffset],
}

/// Returns the trip, how many of its media are new, and which contributors are new.
fn build_trip(b: BuildTrip<'_>) -> (Trip, usize, Vec<ContributorId>) {
    let day_index = |i: usize| u32::try_from(i + 1).unwrap_or(u32::MAX);
    let days: Vec<TripDay> = b
        .days
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let day_samples: Vec<Sample> = d
                .media
                .iter()
                .filter_map(|m| b.sample_by_id.get(m).map(|s| (*s).clone()))
                .collect();
            let landmark =
                naming::dominant_landmark(day_samples.iter().filter_map(|s| s.landmark.as_deref()));
            TripDay {
                index: day_index(i),
                date: d.date,
                media: d.media.clone(),
                route: route::from_gps(&day_samples, route::Params::default()),
                title: Some(naming::day_title(day_index(i), landmark.as_deref())),
                story: None,
            }
        })
        .collect();
    let landmark =
        naming::dominant_landmark(b.members.iter().filter_map(|s| s.landmark.as_deref()));
    let picks = highlights::pick(b.members, HIGHLIGHTS_PER_TRIP, b.pinned, b.hidden);
    let mut contributors: Vec<ContributorId> =
        b.members.iter().map(|s| s.contributor.clone()).collect();
    contributors.sort();
    contributors.dedup();

    let previous_members: BTreeSet<&MediaId> = b
        .previous
        .iter()
        .flat_map(|t| t.days.iter().flat_map(|d| d.media.iter()))
        .collect();
    let new_media = b
        .members
        .iter()
        .filter(|s| !previous_members.contains(&s.id))
        .count();
    let new_contributors: Vec<ContributorId> = contributors
        .iter()
        .filter(|c| b.previous.is_none_or(|t| !t.contributors.contains(c)))
        .cloned()
        .collect();

    // A name the narrative step already used stays; otherwise it follows the data.
    let name = b.previous.filter(|t| t.narrative.is_some()).map_or_else(
        || naming::trip_name(landmark.as_deref(), days[0].date, days[days.len() - 1].date),
        |t| t.name.clone(),
    );
    let trip = Trip {
        id: b.id,
        household: b.household.clone(),
        name,
        start: b.start,
        end: b.end,
        route: route::from_gps(b.members, route::Params::default()),
        days,
        hero: picks.first().cloned(),
        highlights: picks,
        clock_offsets: b
            .offsets
            .iter()
            .filter(|o| contributors.contains(&o.contributor))
            .cloned()
            .collect(),
        contributors,
        narrative: b.previous.and_then(|t| t.narrative.clone()),
        members: b.previous.map(|t| t.members.clone()).unwrap_or_default(),
    };
    (trip, new_media, new_contributors)
}

/// `trip.detected` for every batch that adds media; `contribution.received` for each
/// contributor who is new to an existing trip (F-160, W6).
async fn announce(
    ingest: &Ingest,
    household: &HouseholdId,
    trip: &TripId,
    existed: bool,
    new_media: usize,
    new_contributors: &[ContributorId],
    members: &[Sample],
) -> Result<()> {
    let new_media = u32::try_from(new_media).unwrap_or(u32::MAX);
    emit(
        ingest,
        household,
        EventKind::TripDetected {
            trip: trip.clone(),
            new_media,
        },
    )
    .await?;
    if !existed {
        return Ok(());
    }
    for c in new_contributors {
        let display_name = ingest
            .storage
            .get_contributor(household, c)
            .await?
            .map_or_else(|| c.to_string(), |c| c.display_name);
        let count = members.iter().filter(|s| &s.contributor == c).count();
        emit(
            ingest,
            household,
            EventKind::ContributionReceived {
                trip: Some(trip.clone()),
                contributor: c.clone(),
                display_name,
                count: u32::try_from(count).unwrap_or(u32::MAX),
            },
        )
        .await?;
    }
    Ok(())
}

async fn emit(ingest: &Ingest, household: &HouseholdId, kind: EventKind) -> Result<()> {
    let event = ingest
        .storage
        .append_event(household, ingest.now(), kind)
        .await?;
    ingest.bus.publish(&event).await
}
