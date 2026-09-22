//! Top-N highlights (F-209): best scores, at most one per burst group, spread across
//! days so a long trip is not represented by its single busiest afternoon.

use crate::Sample;
use mantelpiece_core::ids::MediaId;
use std::collections::BTreeSet;
use time::Date;

/// `n` highlights. Pinned ids always come first; hidden ids never appear.
pub fn pick(samples: &[Sample], n: usize, pinned: &[MediaId], hidden: &[MediaId]) -> Vec<MediaId> {
    let hidden: BTreeSet<&MediaId> = hidden.iter().collect();
    let mut out: Vec<MediaId> = pinned
        .iter()
        .filter(|p| !hidden.contains(p))
        .cloned()
        .collect();
    if out.len() >= n {
        out.truncate(n);
        return out;
    }

    // Per day, candidates sorted by score, one per burst group.
    let mut days: Vec<(Date, Vec<&Sample>)> = Vec::new();
    let mut sorted: Vec<&Sample> = samples
        .iter()
        .filter(|s| !hidden.contains(&s.id) && !out.contains(&s.id))
        .collect();
    sorted.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut used_groups: BTreeSet<&str> = BTreeSet::new();
    for s in sorted {
        if let Some(g) = s.burst_group.as_deref() {
            if !used_groups.insert(g) {
                continue;
            }
        }
        let date = s.local().date();
        match days.iter_mut().find(|(d, _)| *d == date) {
            Some((_, v)) => v.push(s),
            None => days.push((date, vec![s])),
        }
    }
    days.sort_by_key(|(d, _)| *d);

    // Round-robin across days by rank.
    let mut rank = 0;
    while out.len() < n {
        let mut any = false;
        for (_, v) in &days {
            if let Some(s) = v.get(rank) {
                out.push(s.id.clone());
                any = true;
                if out.len() == n {
                    break;
                }
            }
        }
        if !any {
            break;
        }
        rank += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample;

    fn scored(id: &str, hours: f64, score: f32, group: Option<&str>) -> Sample {
        let mut s = sample(id, hours, None);
        s.score = score;
        s.burst_group = group.map(str::to_owned);
        s
    }

    #[test]
    fn spreads_across_days_and_dedups_bursts() {
        let samples = vec![
            scored("d1a", 0.0, 9.0, Some("g")),
            scored("d1b", 0.1, 8.5, Some("g")), // same burst as d1a: skipped
            scored("d1c", 1.0, 7.0, None),
            scored("d2a", 24.0, 6.0, None),
            scored("d2b", 25.0, 5.0, None),
            scored("d3a", 48.0, 2.0, None),
        ];
        let picks = pick(&samples, 4, &[], &[]);
        let ids: Vec<&str> = picks.iter().map(MediaId::as_str).collect();
        assert_eq!(ids, vec!["d1a", "d2a", "d3a", "d1c"]);
    }

    #[test]
    fn pinned_first_hidden_never() {
        let samples = vec![
            scored("a", 0.0, 9.0, None),
            scored("b", 1.0, 1.0, None),
            scored("c", 2.0, 5.0, None),
        ];
        let picks = pick(&samples, 2, &[MediaId::new("b")], &[MediaId::new("a")]);
        let ids: Vec<&str> = picks.iter().map(MediaId::as_str).collect();
        assert_eq!(ids, vec!["b", "c"]);
    }

    #[test]
    fn fewer_samples_than_n() {
        let samples = vec![scored("a", 0.0, 1.0, None)];
        assert_eq!(pick(&samples, 5, &[], &[]).len(), 1);
    }
}
