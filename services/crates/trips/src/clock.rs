//! Per-contributor clock offset from overlapping GPS (F-155).
//!
//! Two phones standing at the same spot took their photos at about the same time. When
//! one phone's clock is off, its photos land minutes or hours away from the others on
//! the timeline. For each contributor, pair their geotagged photos with the reference
//! contributor's photos within `radius_m`, take the median time difference, and report
//! it as a correction when enough pairs agree and the offset is larger than noise.

use crate::Sample;
use mantelpiece_core::ids::ContributorId;
use mantelpiece_core::trip::ClockOffset;
use std::collections::BTreeMap;
use time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub radius_m: f64,
    /// Pairs further apart in time than this are not the same moment.
    pub max_pair_gap: Duration,
    pub min_samples: u32,
    /// Offsets smaller than this are shutter lag and walking, not clock error.
    pub min_offset: Duration,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            radius_m: 50.0,
            max_pair_gap: Duration::hours(12),
            min_samples: 3,
            min_offset: Duration::seconds(90),
        }
    }
}

/// Pick the reference: the contributor with the most geotagged samples.
pub fn reference(samples: &[Sample]) -> Option<ContributorId> {
    let mut counts: BTreeMap<&ContributorId, usize> = BTreeMap::new();
    for s in samples.iter().filter(|s| s.gps.is_some()) {
        *counts.entry(&s.contributor).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(c, _)| c.clone())
}

/// Estimate offsets for every contributor other than `reference`. A positive offset
/// means the contributor's clock runs ahead: subtract it from their timestamps.
pub fn estimate(samples: &[Sample], reference: &ContributorId, params: Params) -> Vec<ClockOffset> {
    let ref_samples: Vec<&Sample> = samples
        .iter()
        .filter(|s| &s.contributor == reference && s.gps.is_some())
        .collect();
    let mut by_contributor: BTreeMap<&ContributorId, Vec<i64>> = BTreeMap::new();

    for s in samples.iter().filter(|s| &s.contributor != reference) {
        let Some(gps) = s.gps else { continue };
        // Closest reference photo in time among those within the radius.
        let best = ref_samples
            .iter()
            .filter(|r| r.gps.is_some_and(|g| g.distance_m(gps) <= params.radius_m))
            .map(|r| s.captured - r.captured)
            .filter(|d| d.abs() <= params.max_pair_gap)
            .min_by_key(|d| d.abs());
        if let Some(d) = best {
            by_contributor
                .entry(&s.contributor)
                .or_default()
                .push(d.whole_seconds());
        }
    }

    by_contributor
        .into_iter()
        .filter_map(|(contributor, mut diffs)| {
            if (diffs.len() as u32) < params.min_samples {
                return None;
            }
            diffs.sort_unstable();
            let median = diffs[diffs.len() / 2];
            if median.abs() < params.min_offset.whole_seconds() {
                return None;
            }
            Some(ClockOffset {
                contributor: contributor.clone(),
                seconds: median,
                samples: diffs.len() as u32,
            })
        })
        .collect()
}

/// Shift each contributor's samples by their offset.
pub fn apply(samples: &mut [Sample], offsets: &[ClockOffset]) {
    for s in samples.iter_mut() {
        if let Some(o) = offsets.iter().find(|o| o.contributor == s.contributor) {
            s.captured -= Duration::seconds(o.seconds);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample;

    fn maya(id: &str, hours: f64, gps: (f64, f64)) -> Sample {
        let mut s = sample(id, hours, Some(gps));
        s.contributor = ContributorId::new("maya");
        s
    }

    #[test]
    fn detects_one_hour_fast_clock() {
        let spot = (35.0116, 135.7681);
        let spot2 = (35.0120, 135.7690); // ~100 m away
        let mut samples = vec![
            sample("o1", 0.0, Some(spot)),
            sample("o2", 2.0, Some(spot2)),
            sample("o3", 4.0, Some(spot)),
            sample("o4", 8.0, Some(spot2)),
            maya("m1", 1.0, spot),
            maya("m2", 3.0, spot2),
            maya("m3", 5.0, spot),
            maya("m4", 5.01, spot),
        ];
        let reference = reference(&samples).unwrap();
        assert_eq!(reference, ContributorId::new("owner"));
        let offsets = estimate(&samples, &reference, Params::default());
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0].contributor, ContributorId::new("maya"));
        assert_eq!(offsets[0].seconds, 3600);
        assert_eq!(offsets[0].samples, 4);

        apply(&mut samples, &offsets);
        assert_eq!(samples[4].captured, samples[0].captured);
    }

    #[test]
    fn small_differences_and_few_samples_are_ignored() {
        let spot = (35.0, 135.0);
        let samples = vec![
            sample("o1", 0.0, Some(spot)),
            sample("o2", 1.0, Some(spot)),
            sample("o3", 2.0, Some(spot)),
            maya("m1", 0.01, spot),
            maya("m2", 1.01, spot),
            maya("m3", 2.01, spot),
        ];
        assert!(estimate(&samples, &ContributorId::new("owner"), Params::default()).is_empty());

        let few = vec![
            sample("o1", 0.0, Some(spot)),
            maya("m1", 2.0, spot),
            maya("m2", 2.0, spot),
        ];
        assert!(estimate(&few, &ContributorId::new("owner"), Params::default()).is_empty());
    }

    #[test]
    fn no_gps_means_no_reference() {
        assert!(reference(&[sample("a", 0.0, None)]).is_none());
    }
}
