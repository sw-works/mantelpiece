//! Pre-model quality signals (F-107) and the 0 to 10 interest score (F-204).

use image::{imageops::FilterType, DynamicImage};
use mantelpiece_core::media::{Enrichment, Signals};

/// Longest side used for signal computation. Keeps the Laplacian cheap and makes
/// sharpness comparable across resolutions.
const ANALYSIS_SIDE: u32 = 512;

pub fn signals(img: &DynamicImage) -> Signals {
    let (w, h) = (img.width(), img.height());
    let small = if w.max(h) > ANALYSIS_SIDE {
        img.resize(ANALYSIS_SIDE, ANALYSIS_SIDE, FilterType::Triangle)
    } else {
        img.clone()
    };
    let gray = small.to_luma8();
    Signals {
        phash: Some(crate::phash::phash(img)),
        sharpness: Some(laplacian_variance(&gray)),
        exposure: Some(mean_luma(&gray)),
        width: Some(w),
        height: Some(h),
    }
}

/// Variance of the 3x3 Laplacian. Blurry frames sit near zero.
pub fn laplacian_variance(gray: &image::GrayImage) -> f64 {
    let (w, h) = gray.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }
    let at = |x: u32, y: u32| f64::from(gray.get_pixel(x, y).0[0]);
    let mut sum = 0.0;
    let mut sum_sq = 0.0;
    let mut n = 0.0;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let lap = at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1) - 4.0 * at(x, y);
            sum += lap;
            sum_sq += lap * lap;
            n += 1.0;
        }
    }
    let mean = sum / n;
    sum_sq / n - mean * mean
}

/// Mean luminance 0.0 to 1.0.
pub fn mean_luma(gray: &image::GrayImage) -> f64 {
    let total: u64 = gray.pixels().map(|p| u64::from(p.0[0])).sum();
    total as f64 / (gray.len() as f64 * 255.0)
}

/// Combine signals and enrichment into a 0 to 10 score. Weights are judgement, not
/// science; the point is that a sharp, well-exposed, unique photo of smiling people at a
/// landmark beats a blurry duplicate of a wall.
pub fn score(signals: &Signals, enrichment: &Enrichment, uniqueness: f32) -> f32 {
    let sharp = signals
        .sharpness
        .map_or(0.5, |s| (s / 300.0).clamp(0.0, 1.0) as f32);
    let exposure = signals.exposure.map_or(0.5, |e| {
        1.0 - ((e - 0.5).abs() * 2.0).clamp(0.0, 1.0) as f32
    });
    let faces = (enrichment.face_count.min(4) as f32) / 4.0;
    let smiles = if enrichment.face_count == 0 {
        0.0
    } else {
        enrichment.smile_count.min(enrichment.face_count) as f32 / enrichment.face_count as f32
    };
    let landmark = enrichment
        .landmark
        .as_ref()
        .map_or(0.0, |t| t.confidence.clamp(0.0, 1.0));
    let uniqueness = uniqueness.clamp(0.0, 1.0);
    let raw = 0.25 * sharp
        + 0.15 * exposure
        + 0.2 * faces
        + 0.1 * smiles
        + 0.15 * landmark
        + 0.15 * uniqueness;
    (raw * 10.0).clamp(0.0, 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_images::flat;
    use mantelpiece_core::media::Tag;

    #[test]
    fn checker_is_sharper_than_flat_field() {
        let sharp = flat(64, 64, 100, true).to_luma8();
        let blur = flat(64, 64, 100, false).to_luma8();
        assert!(laplacian_variance(&sharp) > 1000.0);
        assert!(laplacian_variance(&blur) < f64::EPSILON);
    }

    #[test]
    fn exposure_is_mean_luma() {
        let dark = flat(16, 16, 25, false).to_luma8();
        let mid = flat(16, 16, 128, false).to_luma8();
        assert!((mean_luma(&dark) - 25.0 / 255.0).abs() < 1e-9);
        assert!((mean_luma(&mid) - 128.0 / 255.0).abs() < 1e-9);
    }

    #[test]
    fn signals_reports_original_dimensions() {
        let s = signals(&flat(800, 600, 120, true));
        assert_eq!((s.width, s.height), (Some(800), Some(600)));
        assert!(s.phash.is_some());
        assert!(s.sharpness.unwrap() > 0.0);
    }

    #[test]
    fn score_orders_sensibly() {
        let good = Signals {
            sharpness: Some(500.0),
            exposure: Some(0.5),
            ..Signals::default()
        };
        let bad = Signals {
            sharpness: Some(5.0),
            exposure: Some(0.05),
            ..Signals::default()
        };
        let people = Enrichment {
            face_count: 3,
            smile_count: 3,
            landmark: Some(Tag {
                name: "Kinkaku-ji".into(),
                confidence: 0.9,
            }),
            ..Enrichment::default()
        };
        let empty = Enrichment::default();
        let best = score(&good, &people, 1.0);
        let worst = score(&bad, &empty, 0.0);
        assert!(best > 8.0, "{best}");
        assert!(worst < 1.0, "{worst}");
        assert!(score(&good, &empty, 0.5) > score(&bad, &empty, 0.5));
        assert!((0.0..=10.0).contains(&best) && (0.0..=10.0).contains(&worst));
    }
}
