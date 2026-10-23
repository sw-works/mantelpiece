//! Near-duplicate grouping (F-107, F-156, F-201) and keeper choice.
//!
//! Works across contributors: two phones at the same landmark within the window produce
//! one group. Keepers are chosen by sharpness, then exposure closeness to mid-grey.

use mantelpiece_core::ids::MediaId;
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub id: MediaId,
    pub captured: OffsetDateTime,
    pub phash: u64,
    pub sharpness: f64,
    pub exposure: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Max Hamming distance between hashes to count as near-duplicate.
    pub max_distance: u32,
    /// Max capture-time gap between two members for them to be compared.
    pub window: Duration,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            max_distance: 10,
            window: Duration::minutes(15),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BurstGroup {
    /// Stable id: the keeper's media id.
    pub id: String,
    pub keeper: MediaId,
    /// All members including the keeper, in capture order.
    pub members: Vec<MediaId>,
}

impl BurstGroup {
    pub fn non_keepers(&self) -> impl Iterator<Item = &MediaId> {
        self.members.iter().filter(move |m| **m != self.keeper)
    }
}

/// Group candidates. Singletons are returned as one-member groups so every media id has a
/// keeper (itself). Output is ordered by first capture time.
pub fn group(candidates: &[Candidate], params: Params) -> Vec<BurstGroup> {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by_key(|&i| candidates[i].captured);

    let mut parent: Vec<usize> = (0..candidates.len()).collect();

    for (pos, &i) in order.iter().enumerate() {
        for &j in &order[pos + 1..] {
            if candidates[j].captured - candidates[i].captured > params.window {
                break;
            }
            if crate::phash::hamming(candidates[i].phash, candidates[j].phash)
                <= params.max_distance
            {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[rj] = ri;
                }
            }
        }
    }

    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for &i in &order {
        let root = find(&mut parent, i);
        match groups.iter_mut().find(|(r, _)| *r == root) {
            Some((_, members)) => members.push(i),
            None => groups.push((root, vec![i])),
        }
    }

    groups
        .into_iter()
        .map(|(_, members)| {
            let keeper = members
                .iter()
                .copied()
                .max_by(|&a, &b| keeper_key(&candidates[a]).total_cmp(&keeper_key(&candidates[b])))
                .expect("group has at least one member");
            BurstGroup {
                id: candidates[keeper].id.to_string(),
                keeper: candidates[keeper].id.clone(),
                members: members.iter().map(|&m| candidates[m].id.clone()).collect(),
            }
        })
        .collect()
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut i = i;
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn keeper_key(c: &Candidate) -> f64 {
    // Sharpness dominates; exposure breaks ties within a stop or so.
    let exposure_penalty = (c.exposure - 0.5).abs();
    c.sharpness.ln_1p() - exposure_penalty
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn c(id: &str, secs: i64, phash: u64, sharp: f64) -> Candidate {
        Candidate {
            id: MediaId::new(id),
            captured: datetime!(2026-04-03 10:00:00 UTC) + Duration::seconds(secs),
            phash,
            sharpness: sharp,
            exposure: 0.5,
        }
    }

    #[test]
    fn burst_becomes_one_group_with_sharpest_keeper() {
        let cands = vec![
            c("a", 0, 0xFF00_FF00_FF00_FF00, 100.0),
            c("b", 1, 0xFF00_FF00_FF00_FF01, 300.0),
            c("c", 2, 0xFF00_FF00_FF00_FF03, 200.0),
            c("d", 600, 0x0000_0000_0000_0000, 50.0),
        ];
        let groups = group(&cands, Params::default());
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].keeper, MediaId::new("b"));
        assert_eq!(groups[0].members.len(), 3);
        assert_eq!(groups[0].non_keepers().count(), 2);
        assert_eq!(groups[1].members, vec![MediaId::new("d")]);
    }

    #[test]
    fn similar_hashes_outside_window_stay_apart() {
        let cands = vec![c("a", 0, 1, 10.0), c("b", 3600, 1, 10.0)];
        assert_eq!(group(&cands, Params::default()).len(), 2);
    }

    #[test]
    fn transitive_chain_merges() {
        // a~b, b~c, but a and c are 12 bits apart: still one group through b.
        let cands = vec![
            c("a", 0, 0b0000_0000_0000, 1.0),
            c("b", 1, 0b0000_0011_1111, 1.0),
            c("c", 2, 0b1111_1111_1111, 1.0),
        ];
        let p = Params {
            max_distance: 6,
            window: Duration::minutes(1),
        };
        assert_eq!(group(&cands, p).len(), 1);
    }

    #[test]
    fn exposure_breaks_ties() {
        let mut a = c("a", 0, 1, 100.0);
        a.exposure = 0.9;
        let b = c("b", 1, 1, 100.0);
        assert_eq!(
            group(&[a, b], Params::default())[0].keeper,
            MediaId::new("b")
        );
    }

    #[test]
    fn empty_input() {
        assert_eq!(group(&[], Params::default()), Vec::<BurstGroup>::new());
    }
}
