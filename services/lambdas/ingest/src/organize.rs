//! Per-household task: bursts and keepers, scores, clock alignment, trips.
//!
//! Idempotent (F-106): re-running over the same media produces the same groups and
//! updates the same trips in place. Trips are matched to existing ones by date overlap.

use mantelpiece_core::events::EventKind;
use mantelpiece_core::field_note::FieldNote;
use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::ids::{ContributorId, HouseholdId, MediaId, TripId};
use mantelpiece_core::media::{MediaItem, ProcessingState};
use mantelpiece_core::trip::{Landmark, Trip, TripDay};
use mantelpiece_ingest::burst::{self, Candidate};
use mantelpiece_ingest::quality;
use mantelpiece_store::places::Place;
use mantelpiece_trips::{clock, cluster, highlights, naming, route, Sample};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use time::{OffsetDateTime, UtcOffset};

use crate::{Ingest, Result};

pub const HIGHLIGHTS_PER_TRIP: usize = 12;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub media: usize,
    /// Photos without GPS that took a position from geotagged neighbours (F-206).
    pub estimated_positions: usize,
    pub groups: usize,
    pub keepers: Vec<MediaId>,
    /// S3 key of a JSON array of `{household, media_id}` for the enrich Distributed Map
    /// (F-108); keeps the state input small for large batches.
    pub keepers_manifest: String,
    pub trips: Vec<TripId>,
    pub new_trips: usize,
    /// S3 key of a JSON array of `{household, media_id}`: highlights whose field note is
    /// missing or stale (F-711), for the field-notes Distributed Map.
    pub field_notes_manifest: String,
    pub field_notes: usize,
    /// S3 key of a JSON array of `{household, trip, day}`: every trip and day sketch map
    /// (F-712), for the sketch-maps Distributed Map; each item skips itself when current.
    pub sketch_maps_manifest: String,
    pub sketch_maps: usize,
}

/// Cell size for the home area, in degrees (about 2 km).
const HOME_CELL: f64 = 0.02;
/// A cell needs this many everyday photos to count as home.
const HOME_MIN_PHOTOS: usize = 20;

/// The centre of the busiest cell among geotagged photos outside any trip (research R9).
#[must_use]
pub fn home_point(media: &[MediaItem]) -> Option<mantelpiece_core::geo::GeoPoint> {
    let mut cells: HashMap<(i64, i64), Vec<mantelpiece_core::geo::GeoPoint>> = HashMap::new();
    for g in media
        .iter()
        .filter(|m| m.trip.is_none())
        .filter_map(|m| m.gps)
    {
        #[allow(clippy::cast_possible_truncation)]
        let key = (
            (g.lat / HOME_CELL).floor() as i64,
            (g.lon / HOME_CELL).floor() as i64,
        );
        cells.entry(key).or_default().push(g);
    }
    let (_, pts) = cells
        .into_iter()
        .filter(|(_, p)| p.len() >= HOME_MIN_PHOTOS)
        .max_by_key(|(k, p)| (p.len(), std::cmp::Reverse(*k)))?;
    let n = pts.len() as f64;
    Some(mantelpiece_core::geo::GeoPoint::new(
        pts.iter().map(|p| p.lat).sum::<f64>() / n,
        pts.iter().map(|p| p.lon).sum::<f64>() / n,
    ))
}

#[allow(clippy::too_many_lines)]
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
    // Cameras without GPS ride along the phone's fixes; done after clock alignment so the
    // interpolation uses corrected times, never before it so offsets come from real fixes.
    let estimated_positions = route::estimate_positions(&mut samples, route::Params::default());
    let clustering = cluster::cluster(&samples, ingest.cluster);
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
    let mut place_cache: HashMap<((i64, i64), u32), Vec<Place>> = HashMap::new();
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

    let mut built: Vec<mantelpiece_core::trip::Trip> = Vec::new();
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
        let (mut trip, new_media, new_contributors) = build_trip(BuildTrip {
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
            undated: &c.undated,
        });
        attach_landmarks(ingest, &mut trip, &sample_by_id, &mut place_cache).await;
        ingest.storage.put_trip(&trip).await?;
        built.push(trip.clone());
        for s in members.iter().map(|s| &s.id).chain(c.undated.iter()) {
            if let Some(m) = media.iter_mut().find(|m| &m.id == s) {
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

    // F-711: the home area, then which highlights need a field note made or remade, and
    // which photos lost theirs (no longer a highlight). Removed photos are not in `media`,
    // so their notes wait for a restore.
    let home = home_point(&media);
    if let Some(mut h) = ingest.storage.get_household(household).await? {
        if h.home != home {
            h.home = home;
            ingest.storage.put_household(&h).await?;
        }
    }
    let mut notes: Vec<MediaId> = Vec::new();
    if ingest.field_notes {
        for m in &mut media {
            let trip = m
                .trip
                .as_ref()
                .and_then(|t| built.iter().find(|b| &b.id == t));
            match trip.filter(|t| t.highlights.contains(&m.id)) {
                Some(t) => {
                    let print = mantelpiece_fieldnotes::fingerprint(m, t, home);
                    if m.field_note.as_ref().is_none_or(|n| n.fingerprint != print) {
                        notes.push(m.id.clone());
                    }
                }
                None => {
                    if m.field_note.take().is_some() {
                        let keys = FieldNote::keys(household.as_str(), m.id.as_str()).to_vec();
                        ingest.blobs.delete(&keys).await?;
                    }
                }
            }
        }
    }

    for m in &media {
        ingest.storage.put_media(m).await?;
    }

    let keepers: Vec<MediaId> = groups.iter().map(|g| g.keeper.clone()).collect();
    let keepers_manifest = format!(
        "manifests/{household}/keepers-{}.json",
        now.unix_timestamp_nanos()
    );
    let manifest: Vec<serde_json::Value> = keepers
        .iter()
        .map(|k| serde_json::json!({ "household": household, "media_id": k }))
        .collect();
    ingest
        .blobs
        .put(
            &keepers_manifest,
            serde_json::to_vec(&manifest).expect("manifest serialises"),
            "application/json",
        )
        .await?;

    let field_notes_manifest = format!(
        "manifests/{household}/field-notes-{}.json",
        now.unix_timestamp_nanos()
    );
    let entries: Vec<serde_json::Value> = notes
        .iter()
        .map(|m| serde_json::json!({ "household": household, "media_id": m }))
        .collect();
    ingest
        .blobs
        .put(
            &field_notes_manifest,
            serde_json::to_vec(&entries).expect("manifest serialises"),
            "application/json",
        )
        .await?;

    // F-712: every trip and day map; the task itself skips maps whose fingerprint matches.
    let sketch_maps_manifest = format!(
        "manifests/{household}/sketch-maps-{}.json",
        now.unix_timestamp_nanos()
    );
    let maps: Vec<serde_json::Value> = if ingest.field_notes {
        built
            .iter()
            .flat_map(|t| {
                std::iter::once(None)
                    .chain(t.days.iter().map(|d| Some(d.index)))
                    .map(|day| serde_json::json!({ "household": household, "trip": t.id, "day": day }))
            })
            .collect()
    } else {
        vec![]
    };
    ingest
        .blobs
        .put(
            &sketch_maps_manifest,
            serde_json::to_vec(&maps).expect("manifest serialises"),
            "application/json",
        )
        .await?;

    Ok(Output {
        sketch_maps_manifest,
        sketch_maps: maps.len(),
        field_notes_manifest,
        field_notes: notes.len(),
        media: media.len(),
        estimated_positions,
        groups: groups.len(),
        keepers,
        keepers_manifest,
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
            dated: m.captured.is_some() || m.gps.is_some(),
            offset: m
                .captured
                .and_then(|c| c.offset_seconds)
                .and_then(|s| UtcOffset::from_whole_seconds(s).ok()),
            gps: m.gps,
            gps_estimated: false,
            score: m.quality_score.unwrap_or(0.0),
            burst_group: m.burst_group.clone(),
            landmark: m.enrichment.landmark.as_ref().map(|t| t.name.clone()),
        })
        .collect()
}

/// Radius a place counts as "along the route" and a photo counts as "at" the place.
pub const LANDMARK_RADIUS_M: u32 = 2_000;
/// Lookups per day: where the photos cluster, plus positions spread along the route, never
/// closer than a kilometre; one wider lookup for the region the day was mostly in.
const LANDMARK_STEP_M: f64 = 1_000.0;
const LANDMARK_LOOKUPS_PER_DAY: usize = 16;
const LANDMARK_PROBES_MAX: usize = 24;
const REGION_RADIUS_M: u32 = 10_000;
const REGION_LOOKUP_LIMIT: usize = 50;
const REGION_LOOKUPS_PER_DAY: usize = 6;
/// At most this many places from the same neighbourhood, so a city does not crowd out
/// the loch and the mountain further along.
const LANDMARKS_PER_NEIGHBOURHOOD: usize = 2;
const NEIGHBOURHOOD_M: f64 = 1_500.0;
/// Candidates fetched per probe; ranking picks from them.
const LANDMARKS_PER_LOOKUP: usize = 8;
/// Places kept per day after ranking.
const LANDMARKS_PER_DAY: usize = 12;

fn place_cell(at: GeoPoint) -> (i64, i64) {
    #[allow(clippy::cast_possible_truncation)]
    let cell = (
        (at.lat * 1000.0).round() as i64,
        (at.lon * 1000.0).round() as i64,
    );
    cell
}

/// Where to ask "what is here" for one day: one probe per photo cluster (a new cluster
/// whenever the photos moved on), positions spread along the route, then the busiest
/// clusters again at region radius so the island or the city the day was in comes back.
fn day_probes(day: &TripDay, positioned: &[(&MediaId, GeoPoint)]) -> Vec<(GeoPoint, u32, usize)> {
    // A cluster's probe is its centroid, not its first photo, so a stop photographed
    // from several spots is looked up from its middle.
    let mut clusters: Vec<(GeoPoint, usize)> = Vec::new();
    for (_, at) in positioned {
        match clusters.last_mut() {
            Some((c, n)) if c.distance_m(*at) < f64::from(LANDMARK_RADIUS_M) => {
                #[allow(clippy::cast_precision_loss)]
                let k = *n as f64;
                *c = GeoPoint::new(
                    (c.lat * k + at.lat) / (k + 1.0),
                    (c.lon * k + at.lon) / (k + 1.0),
                );
                *n += 1;
            }
            _ => clusters.push((*at, 1)),
        }
    }
    let mut probes: Vec<(GeoPoint, u32, usize)> = clusters
        .iter()
        .map(|(at, _)| (*at, LANDMARK_RADIUS_M, LANDMARKS_PER_LOOKUP))
        .collect();
    for at in route::probes(&day.route, LANDMARK_LOOKUPS_PER_DAY, LANDMARK_STEP_M) {
        if probes
            .iter()
            .all(|(p, _, _)| p.distance_m(at) >= LANDMARK_STEP_M)
        {
            probes.push((at, LANDMARK_RADIUS_M, LANDMARKS_PER_LOOKUP));
        }
    }
    probes.truncate(LANDMARK_PROBES_MAX);
    let mut busiest = clusters;
    busiest.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (at, _) in busiest.into_iter().take(REGION_LOOKUPS_PER_DAY) {
        probes.push((at, REGION_RADIUS_M, REGION_LOOKUP_LIMIT));
    }
    probes
}

async fn lookup(
    ingest: &Ingest,
    cache: &mut HashMap<((i64, i64), u32), Vec<Place>>,
    at: GeoPoint,
    radius: u32,
    limit: usize,
) -> Vec<Place> {
    let key = (place_cell(at), radius);
    if let Some(p) = cache.get(&key) {
        return p.clone();
    }
    let p = match ingest.places.nearby(at, radius, limit).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, lat = at.lat, lon = at.lon, "place lookup failed");
            Vec::new()
        }
    };
    cache.insert(key, p.clone());
    p
}

/// Most photographed first, then the kind of place a day gets named after, then the
/// closest to the route; noise (a court, a shop) sinks whatever its count. At most two
/// places per neighbourhood so a city does not crowd out the loch further along.
fn keep_ranked(mut found: Vec<Landmark>) -> Vec<Landmark> {
    found.sort_by(|a, b| {
        (b.prominence() > 0)
            .cmp(&(a.prominence() > 0))
            .then(b.media.len().cmp(&a.media.len()))
            .then(b.prominence().cmp(&a.prominence()))
            .then(a.distance_m.total_cmp(&b.distance_m))
    });
    let mut kept: Vec<Landmark> = Vec::new();
    for l in found {
        let neighbours = kept
            .iter()
            .filter(|k| k.at.distance_m(l.at) < NEIGHBOURHOOD_M)
            .count();
        if neighbours < LANDMARKS_PER_NEIGHBOURHOOD && kept.len() < LANDMARKS_PER_DAY {
            kept.push(l);
        }
    }
    kept
}

/// Name the places along each day's route (F-206, constitution V: a lookup before any
/// model). Each place keeps the photos taken within its reach, so a whole island can
/// outrank a chapel. A day or trip still called "Day 3" or "Trip, ..." takes its name from
/// the first landmark. Lookup failures leave the day unnamed and never fail organize.
async fn attach_landmarks(
    ingest: &Ingest,
    trip: &mut Trip,
    sample_by_id: &HashMap<MediaId, &Sample>,
    cache: &mut HashMap<((i64, i64), u32), Vec<Place>>,
) {
    for day in &mut trip.days {
        // Every photo with a position counts, including the ones route thinning dropped.
        let positioned: Vec<(&MediaId, GeoPoint)> = day
            .media
            .iter()
            .filter_map(|m| sample_by_id.get(m).and_then(|s| s.gps.map(|g| (m, g))))
            .collect();
        let mut found: Vec<Landmark> = Vec::new();
        for (at, radius, limit) in day_probes(day, &positioned) {
            for place in lookup(ingest, cache, at, radius, limit).await {
                if let Some(seen) = found.iter_mut().find(|l| l.name == place.name) {
                    seen.distance_m = seen.distance_m.min(place.distance_m);
                    continue;
                }
                found.push(Landmark {
                    name: place.name,
                    at: place.at,
                    distance_m: place.distance_m,
                    source: place.source,
                    kind: place.kind,
                    dim_m: place.dim_m,
                    media: vec![],
                    country: place.country,
                });
            }
        }
        for l in &mut found {
            let reach = l.reach_m();
            l.media = positioned
                .iter()
                .filter(|(_, at)| at.distance_m(l.at) <= reach)
                .map(|(m, _)| (*m).clone())
                .collect();
        }
        day.landmarks = keep_ranked(found);
        // Facts for what the day shows (F-210): what the route passed, looked up once.
        for l in &day.landmarks {
            crate::landmarks::ensure_facts(ingest, &l.name, Some(l.at)).await;
        }
        let unnamed = day.title.as_deref().is_none_or(|t| !t.contains(':'));
        if unnamed {
            if let Some(best) = Landmark::best_name(&day.landmarks) {
                day.title = Some(naming::day_title(day.index, Some(&best.name)));
            }
        }
    }
    let generic = trip.narrative.is_none() && trip.name.starts_with("Trip,");
    if generic {
        if let Some(place) = trip_place(trip) {
            let (first, last) = (trip.days[0].date, trip.days[trip.days.len() - 1].date);
            trip.name = naming::trip_name(Some(&place), first, last);
        }
    }
}

/// What a trip is named after. Days spent around one town take the town's name; days
/// spread over several towns of one country take the country's ("Iceland", not the town
/// the last day happened to end in).
fn trip_place(trip: &Trip) -> Option<String> {
    let day_places: Vec<&Landmark> = trip
        .days
        .iter()
        .filter_map(|d| Landmark::best_name(&d.landmarks))
        .collect();
    let mut towns: Vec<&str> = day_places.iter().map(|l| l.name.as_str()).collect();
    towns.sort_unstable();
    towns.dedup();
    if towns.len() > 1 {
        let mut countries: Vec<&str> = day_places
            .iter()
            .filter_map(|l| l.country.as_deref())
            .collect();
        countries.sort_unstable();
        let most = countries
            .chunk_by(|a, b| a == b)
            .max_by_key(|run| run.len())
            .filter(|run| run.len() * 2 > day_places.len());
        if let Some(run) = most {
            return Some(run[0].to_owned());
        }
    }
    let all = trip.landmarks();
    Landmark::best_name(&all).map(|l| l.name.clone())
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
    offsets: &'a [mantelpiece_core::trip::ClockOffset],
    /// Photos with no capture time, placed at the end of the last day and flagged.
    undated: &'a [MediaId],
}

/// Returns the trip, how many of its media are new, and which contributors are new.
fn build_trip(b: BuildTrip<'_>) -> (Trip, usize, Vec<ContributorId>) {
    let day_index = |i: usize| u32::try_from(i + 1).unwrap_or(u32::MAX);
    let mut days: Vec<TripDay> = b
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
                story: b.previous.and_then(|t| {
                    t.days
                        .iter()
                        .find(|d| d.index == day_index(i))
                        .and_then(|d| d.story.clone())
                }),
                undated: vec![],
                landmarks: vec![],
            }
        })
        .collect();
    if let Some(last) = days.last_mut() {
        for id in b.undated {
            if !last.media.contains(id) {
                last.media.push(id.clone());
            }
            last.undated.push(id.clone());
        }
    }
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
    let event =
        mantelpiece_store::feed::emit(ingest.storage.as_ref(), household, ingest.now(), kind)
            .await?;
    ingest.bus.publish(&event).await?;
    Ok(())
}

#[cfg(test)]
mod trip_place_tests {
    use super::*;
    use mantelpiece_core::ids::{HouseholdId, TripId};
    use mantelpiece_core::trip::{Route, TripDay};
    use time::macros::{date, datetime};

    fn town(name: &str, country: &str) -> Landmark {
        Landmark {
            name: name.into(),
            at: GeoPoint::new(64.0, -21.0),
            distance_m: 0.0,
            source: "aws".into(),
            kind: Some("city".into()),
            dim_m: Some(10_000),
            media: vec![],
            country: Some(country.into()),
        }
    }

    fn trip_of(towns: &[(&str, &str)]) -> Trip {
        Trip {
            id: TripId::new("t"),
            household: HouseholdId::new("h"),
            name: "Trip, June 27 to 28, 2015".into(),
            start: datetime!(2015-06-27 00:00:00 UTC),
            end: datetime!(2015-06-28 00:00:00 UTC),
            days: towns
                .iter()
                .enumerate()
                .map(|(i, (name, country))| TripDay {
                    index: u32::try_from(i + 1).unwrap(),
                    date: date!(2015 - 06 - 27),
                    media: vec![],
                    route: Route::default(),
                    title: None,
                    story: None,
                    undated: vec![],
                    landmarks: vec![town(name, country)],
                })
                .collect(),
            route: Route::default(),
            highlights: vec![],
            contributors: vec![],
            clock_offsets: vec![],
            narrative: None,
            hero: None,
            members: vec![],
        }
    }

    #[test]
    fn a_trip_across_towns_is_named_after_their_country_and_one_town_keeps_its_name() {
        let iceland = trip_of(&[
            ("Borgarbyggð", "Iceland"),
            ("Norðurþing", "Iceland"),
            ("Reykjanesbær", "Iceland"),
        ]);
        assert_eq!(trip_place(&iceland).as_deref(), Some("Iceland"));
        let oxford = trip_of(&[("Oxford", "United Kingdom"), ("Oxford", "United Kingdom")]);
        assert_eq!(trip_place(&oxford).as_deref(), Some("Oxford"));
        // Towns in different countries with no majority: fall back to the best single place.
        let mixed = trip_of(&[("Basel", "Switzerland"), ("Freiburg", "Germany")]);
        assert!(["Basel", "Freiburg"].contains(&trip_place(&mixed).as_deref().unwrap()));
    }
}
