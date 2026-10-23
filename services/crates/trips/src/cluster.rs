//! Event and trip clustering (F-205).
//!
//! Samples are sorted by capture time. Consecutive samples belong to the same event
//! unless the time gap or the distance exceeds the event thresholds; consecutive events
//! belong to the same trip unless the gap exceeds the trip threshold. A trip must span
//! at least two local calendar days; shorter clusters are day outings and are returned
//! separately so the caller can still show them. Samples with no capture time never
//! shape a cluster: they are attached to the nearest cluster by upload time and listed as
//! `undated` so the caller can flag them.

use crate::Sample;
use mantelpiece_core::ids::MediaId;
use time::{Date, Duration};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub event_gap: Duration,
    pub event_radius_m: f64,
    pub trip_gap: Duration,
    pub min_trip_days: u32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            event_gap: Duration::hours(3),
            event_radius_m: 2_000.0,
            trip_gap: Duration::hours(36),
            min_trip_days: 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub media: Vec<MediaId>,
    pub first: usize,
    pub last: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    pub date: Date,
    pub media: Vec<MediaId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    /// Indices into the sorted sample slice returned alongside.
    pub sample_indices: Vec<usize>,
    pub events: Vec<Event>,
    pub days: Vec<Day>,
    pub is_trip: bool,
    /// Media without a capture time whose upload time falls nearest this cluster.
    pub undated: Vec<MediaId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clustering {
    /// Dated input samples sorted by capture time; indices elsewhere refer to this.
    pub sorted: Vec<Sample>,
    pub clusters: Vec<Cluster>,
    /// Undated media with no cluster to attach to.
    pub unplaced: Vec<MediaId>,
}

pub fn cluster(samples: &[Sample], params: Params) -> Clustering {
    let mut sorted: Vec<Sample> = samples.iter().filter(|s| s.dated).cloned().collect();
    sorted.sort_by_key(|s| s.captured);
    let undated: Vec<&Sample> = samples.iter().filter(|s| !s.dated).collect();

    let mut clusters = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for i in 0..sorted.len() {
        if let Some(&prev) = current.last() {
            if sorted[i].captured - sorted[prev].captured > params.trip_gap {
                clusters.push(finish(&sorted, std::mem::take(&mut current), params));
            }
        }
        current.push(i);
    }
    if !current.is_empty() {
        clusters.push(finish(&sorted, current, params));
    }

    let mut unplaced = Vec::new();
    for s in undated {
        let nearest = clusters
            .iter_mut()
            .map(|c| {
                let first = sorted[c.sample_indices[0]].captured;
                let last = sorted[*c.sample_indices.last().expect("non-empty")].captured;
                let distance = if s.captured < first {
                    first - s.captured
                } else if s.captured > last {
                    s.captured - last
                } else {
                    Duration::ZERO
                };
                (distance, c)
            })
            .min_by_key(|(d, _)| *d);
        match nearest {
            Some((_, c)) => c.undated.push(s.id.clone()),
            None => unplaced.push(s.id.clone()),
        }
    }
    Clustering {
        sorted,
        clusters,
        unplaced,
    }
}

fn finish(sorted: &[Sample], indices: Vec<usize>, params: Params) -> Cluster {
    let mut events: Vec<Event> = Vec::new();
    for &i in &indices {
        let new_event = match events.last() {
            None => true,
            Some(ev) => {
                let prev = &sorted[ev.last];
                let far = match (prev.gps, sorted[i].gps) {
                    (Some(a), Some(b)) => a.distance_m(b) > params.event_radius_m,
                    _ => false,
                };
                sorted[i].captured - prev.captured > params.event_gap || far
            }
        };
        if new_event {
            events.push(Event {
                media: vec![],
                first: i,
                last: i,
            });
        }
        let ev = events.last_mut().expect("just pushed");
        ev.media.push(sorted[i].id.clone());
        ev.last = i;
    }

    let mut days: Vec<Day> = Vec::new();
    for &i in &indices {
        let date = sorted[i].local().date();
        match days.last_mut() {
            Some(d) if d.date == date => d.media.push(sorted[i].id.clone()),
            _ => days.push(Day {
                date,
                media: vec![sorted[i].id.clone()],
            }),
        }
    }

    let is_trip = days.len() as u32 >= params.min_trip_days;
    Cluster {
        sample_indices: indices,
        events,
        days,
        is_trip,
        undated: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample;

    #[test]
    fn multi_day_trip_with_events_and_days() {
        let kyoto = Some((35.0116, 135.7681));
        let arashiyama = Some((35.0094, 135.6667));
        let samples = vec![
            sample("a", 0.0, kyoto),
            sample("b", 0.5, kyoto),
            sample("c", 5.0, arashiyama), // 4.5h later and 9km away: new event
            sample("d", 24.0, kyoto),     // next day
            sample("e", 25.0, kyoto),
            sample("z", 24.0 * 20.0, None), // three weeks later: separate cluster
        ];
        let c = cluster(&samples, Params::default());
        assert_eq!(c.clusters.len(), 2);
        let trip = &c.clusters[0];
        assert!(trip.is_trip);
        assert_eq!(trip.events.len(), 3);
        assert_eq!(trip.days.len(), 2);
        assert_eq!(trip.days[0].media.len(), 3);
        assert_eq!(
            trip.days[1].media,
            vec![MediaId::new("d"), MediaId::new("e")]
        );
        assert!(!c.clusters[1].is_trip);
    }

    #[test]
    fn distance_alone_splits_event() {
        let samples = vec![
            sample("a", 0.0, Some((35.0, 135.0))),
            sample("b", 0.1, Some((35.1, 135.0))),
        ];
        let c = cluster(&samples, Params::default());
        assert_eq!(c.clusters[0].events.len(), 2);
    }

    #[test]
    fn local_date_buckets_use_camera_zone() {
        // 23:30 local (+09:00) then 00:30 local next day, both same UTC day.
        let mut a = sample("a", 5.5, None); // 14:30 UTC = 23:30 +09
        let mut b = sample("b", 6.5, None); // 15:30 UTC = 00:30 +09 next day
        a.offset = Some(time::UtcOffset::from_hms(9, 0, 0).unwrap());
        b.offset = a.offset;
        let c = cluster(&[a, b], Params::default());
        assert_eq!(c.clusters[0].days.len(), 2);
        assert!(c.clusters[0].is_trip);
    }

    #[test]
    fn unsorted_input_is_sorted() {
        let samples = vec![sample("b", 1.0, None), sample("a", 0.0, None)];
        let c = cluster(&samples, Params::default());
        assert_eq!(c.sorted[0].id, MediaId::new("a"));
        assert_eq!(
            c.clusters[0].events[0].media,
            vec![MediaId::new("a"), MediaId::new("b")]
        );
    }

    #[test]
    fn empty() {
        assert_eq!(
            cluster(&[], Params::default()).clusters,
            Vec::<Cluster>::new()
        );
    }

    #[test]
    fn undated_media_attach_to_the_nearest_trip_without_shaping_it() {
        let kyoto = Some((35.0116, 135.7681));
        let mut scan = sample("scan", 24.0 * 3.0, None); // uploaded three days after the trip
        scan.dated = false;
        let mut old = sample("old", 24.0 * 40.0, None); // uploaded near the later outing
        old.dated = false;
        let samples = vec![
            sample("a", 0.0, kyoto),
            sample("b", 24.0, kyoto),
            scan,
            sample("z", 24.0 * 41.0, None),
            old,
        ];
        let c = cluster(&samples, Params::default());
        assert_eq!(c.sorted.len(), 3, "undated samples are not sorted in");
        assert_eq!(c.clusters.len(), 2);
        assert_eq!(c.clusters[0].undated, vec![MediaId::new("scan")]);
        assert_eq!(c.clusters[0].days.len(), 2, "an undated photo adds no day");
        assert_eq!(c.clusters[1].undated, vec![MediaId::new("old")]);
        assert_eq!(c.unplaced, Vec::<MediaId>::new());
        let mut lonely = sample("lonely", 0.0, None);
        lonely.dated = false;
        assert_eq!(
            cluster(&[lonely], Params::default()).unplaced,
            vec![MediaId::new("lonely")]
        );
    }
}
