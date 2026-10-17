//! The music library: a few CC BY tracks with beat grids computed offline by
//! `scripts/music-library.sh` and embedded here, so planning needs neither the audio nor
//! the network. The audio itself is deployed to the media bucket under `music/`.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

static LIBRARY: &str = include_str!("../assets/music.json");

/// A track's first bar must fall this early: the reel starts with the music at its own 0
/// (the TV never seeks, decision 39), and the title slide lasts until a bar line.
pub const MAX_FIRST_BAR_MS: u64 = 4_000;

#[derive(Clone, Debug, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub licence: String,
    /// Where the track and its licence are published.
    pub source: String,
    /// S3 key of the AAC audio, `music/{id}.m4a`.
    pub file: String,
    pub bpm: f64,
    pub duration_ms: u64,
    /// Onset strength on beats over the mean; above about 3 the grid is worth cutting to.
    pub clarity: f64,
    pub beats_ms: Vec<u64>,
    pub bars_ms: Vec<u64>,
    pub sections_ms: Vec<u64>,
    /// The last bar line before the music decays.
    pub final_hit_ms: u64,
    pub decay_end_ms: u64,
}

impl Track {
    /// The on-screen credit CC BY asks for.
    #[must_use]
    pub fn credit(&self) -> String {
        format!(
            "Music: \u{201c}{}\u{201d} by {}, {}",
            self.title, self.artist, self.licence
        )
    }

    /// Median bar length.
    #[must_use]
    pub fn bar_ms(&self) -> u64 {
        let mut d: Vec<u64> = self.bars_ms.windows(2).map(|w| w[1] - w[0]).collect();
        d.sort_unstable();
        d.get(d.len() / 2).copied().unwrap_or(2_000)
    }

    /// Bar lines a cut may land on: from the first bar up to the final hit.
    #[must_use]
    pub fn cut_points(&self) -> Vec<u64> {
        self.bars_ms
            .iter()
            .copied()
            .filter(|&b| b <= self.final_hit_ms)
            .collect()
    }
}

#[derive(Deserialize)]
struct File {
    tracks: Vec<Track>,
}

/// Every track in the library.
///
/// # Panics
/// When the embedded `assets/music.json` is malformed, which a test catches.
pub fn library() -> &'static [Track] {
    static TRACKS: OnceLock<Vec<Track>> = OnceLock::new();
    TRACKS.get_or_init(|| {
        serde_json::from_str::<File>(LIBRARY)
            .expect("assets/music.json is valid")
            .tracks
    })
}

/// The track for a reel: one of the tracks whose first bar is early enough, chosen by a
/// hash of `seed` (the trip and day), so a trip keeps its track and trips vary.
///
/// # Panics
/// When the library has no usable track, which a test catches.
#[must_use]
pub fn pick(seed: &str) -> &'static Track {
    let usable: Vec<&Track> = library()
        .iter()
        .filter(|t| t.bars_ms.first().is_some_and(|&b| b <= MAX_FIRST_BAR_MS))
        .collect();
    let digest = Sha256::digest(seed.as_bytes());
    let n = u64::from_be_bytes(digest[..8].try_into().expect("8 bytes"));
    let len = u64::try_from(usable.len()).expect("small");
    usable[usize::try_from(n % len).expect("small")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_parses_and_has_usable_tracks() {
        let lib = library();
        assert!(lib.len() >= 2);
        for t in lib {
            assert!(
                t.bars_ms.windows(2).all(|w| w[0] < w[1]),
                "{} bars ascend",
                t.id
            );
            assert!(t.final_hit_ms <= t.duration_ms);
            assert_eq!(t.file, format!("music/{}.m4a", t.id));
            assert!(t.credit().contains("CC BY"));
        }
        let t = pick("trip-1/day-2");
        assert!(t.bars_ms[0] <= MAX_FIRST_BAR_MS);
        assert_eq!(
            pick("trip-1/day-2").id,
            t.id,
            "the same seed picks the same track"
        );
    }
}
