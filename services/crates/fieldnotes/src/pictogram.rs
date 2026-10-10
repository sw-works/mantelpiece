//! Little sketched drawings for the trip's named places on a sketch map (F-712): what kind
//! of place a landmark is, from its provider type and its name, and a few pen strokes for
//! each kind.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Waterfall,
    Lighthouse,
    Glacier,
    Church,
    Peak,
    HotSpring,
    Castle,
    Beach,
    Lake,
    Town,
    Star,
}

/// The kind of a landmark from its provider type and its name, in English and the
/// Nordic and German words that name such places.
#[must_use]
pub fn kind_of(name: &str, kind: Option<&str>) -> Kind {
    let n = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if has(&["waterfall", "falls", "foss", "fall "]) {
        Kind::Waterfall
    } else if has(&["lighthouse", "viti", "fyr"]) {
        Kind::Lighthouse
    } else if has(&["glacier", "jökull", "jokull", "gletscher"]) {
        Kind::Glacier
    } else if has(&["church", "kirkja", "kirche", "cathedral", "chapel", "kirk"]) {
        Kind::Church
    } else if has(&["hot spring", "geysir", "geyser", "hver", "laug", "thermal"]) {
        Kind::HotSpring
    } else if has(&["castle", "burg", "schloss", "fortress"]) {
        Kind::Castle
    } else if has(&["beach", "strand", "fjara"]) {
        Kind::Beach
    } else if has(&["lake", "vatn", "lagoon", "lón"]) || kind == Some("waterbody") {
        Kind::Lake
    } else if has(&["peak", "mount", "fjall", "tind", "berg", "volcano"])
        || kind == Some("mountain")
    {
        Kind::Peak
    } else if kind == Some("city") {
        Kind::Town
    } else {
        Kind::Star
    }
}

/// The pen strokes of a pictogram centred on `(x, y)`, `s` px tall, as open polylines
/// (closed ones repeat their first point).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn strokes(kind: Kind, x: f32, y: f32, s: f32) -> Vec<Vec<[f32; 2]>> {
    let h = s / 2.0;
    let p = |dx: f32, dy: f32| [x + dx * h, y + dy * h];
    let wave = |x0: f32, x1: f32, yy: f32, amp: f32| -> Vec<[f32; 2]> {
        (0..=8)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f32 / 8.0;
                p(
                    x0 + (x1 - x0) * t,
                    yy + amp * (t * std::f32::consts::TAU * 1.5).sin(),
                )
            })
            .collect()
    };
    match kind {
        Kind::Waterfall => vec![
            vec![p(-1.0, -0.9), p(0.9, -0.9)],
            vec![p(-0.5, -0.8), p(-0.6, 0.0), p(-0.45, 0.8)],
            vec![p(0.0, -0.8), p(-0.05, 0.1), p(0.05, 0.9)],
            vec![p(0.5, -0.8), p(0.45, 0.0), p(0.55, 0.8)],
            wave(-1.0, 1.0, 1.0, 0.08),
        ],
        Kind::Lighthouse => vec![
            vec![
                p(-0.35, 1.0),
                p(-0.2, -0.5),
                p(0.2, -0.5),
                p(0.35, 1.0),
                p(-0.35, 1.0),
            ],
            vec![
                p(-0.25, -0.5),
                p(-0.25, -0.85),
                p(0.25, -0.85),
                p(0.25, -0.5),
            ],
            vec![p(-0.28, 0.2), p(0.28, 0.2)],
            vec![p(0.4, -0.75), p(1.0, -0.95)],
            vec![p(-0.4, -0.75), p(-1.0, -0.95)],
        ],
        Kind::Glacier => vec![
            vec![
                p(-1.0, 0.8),
                p(-0.4, -0.6),
                p(-0.1, -0.2),
                p(0.3, -0.9),
                p(1.0, 0.8),
            ],
            vec![p(-0.6, 0.1), p(-0.2, 0.1)],
            vec![p(0.1, -0.1), p(0.6, -0.1)],
            vec![p(-0.8, 0.5), p(0.8, 0.5)],
        ],
        Kind::Church => vec![
            vec![
                p(-0.7, 1.0),
                p(-0.7, -0.1),
                p(0.0, -0.6),
                p(0.7, -0.1),
                p(0.7, 1.0),
                p(-0.7, 1.0),
            ],
            vec![p(0.0, -0.6), p(0.0, -1.0)],
            vec![p(-0.2, -0.85), p(0.2, -0.85)],
            vec![p(-0.15, 1.0), p(-0.15, 0.45), p(0.15, 0.45), p(0.15, 1.0)],
        ],
        Kind::Peak => vec![
            vec![p(-1.0, 0.8), p(0.0, -0.9), p(1.0, 0.8)],
            vec![p(-0.35, -0.3), p(-0.1, -0.15), p(0.1, -0.35), p(0.35, -0.3)],
        ],
        Kind::HotSpring => vec![
            vec![p(-0.9, 0.3), p(-0.6, 0.9), p(0.6, 0.9), p(0.9, 0.3)],
            vec![p(-0.4, 0.1), p(-0.55, -0.3), p(-0.35, -0.6), p(-0.5, -1.0)],
            vec![p(0.0, 0.1), p(-0.15, -0.3), p(0.05, -0.6), p(-0.1, -1.0)],
            vec![p(0.4, 0.1), p(0.25, -0.3), p(0.45, -0.6), p(0.3, -1.0)],
        ],
        Kind::Castle => vec![
            vec![
                p(-0.8, 1.0),
                p(-0.8, -0.6),
                p(-0.5, -0.6),
                p(-0.5, -0.3),
                p(-0.15, -0.3),
                p(-0.15, -0.6),
                p(0.15, -0.6),
                p(0.15, -0.3),
                p(0.5, -0.3),
                p(0.5, -0.6),
                p(0.8, -0.6),
                p(0.8, 1.0),
                p(-0.8, 1.0),
            ],
            vec![p(-0.2, 1.0), p(-0.2, 0.4), p(0.2, 0.4), p(0.2, 1.0)],
        ],
        Kind::Beach => vec![
            wave(-1.0, 1.0, 0.4, 0.15),
            wave(-0.8, 0.8, 0.85, 0.12),
            vec![
                p(0.3, -0.9),
                p(0.6, -0.6),
                p(0.3, -0.3),
                p(0.0, -0.6),
                p(0.3, -0.9),
            ],
        ],
        Kind::Lake => {
            let ellipse: Vec<_> = (0..=16)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let a = i as f32 / 16.0 * std::f32::consts::TAU;
                    p(a.cos(), 0.55 * a.sin())
                })
                .collect();
            vec![ellipse, wave(-0.5, 0.5, 0.0, 0.12)]
        }
        Kind::Town => vec![
            vec![
                p(-1.0, 1.0),
                p(-1.0, 0.0),
                p(-0.55, -0.45),
                p(-0.1, 0.0),
                p(-0.1, 1.0),
                p(-1.0, 1.0),
            ],
            vec![
                p(0.1, 1.0),
                p(0.1, -0.3),
                p(0.55, -0.8),
                p(1.0, -0.3),
                p(1.0, 1.0),
                p(0.1, 1.0),
            ],
        ],
        Kind::Star => {
            let pts: Vec<_> = (0..=10)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let a = i as f32 / 10.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                    let r = if i % 2 == 0 { 1.0 } else { 0.42 };
                    p(r * a.cos(), r * a.sin())
                })
                .collect();
            vec![pts]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icelandic_and_english_names_find_their_kind() {
        assert_eq!(kind_of("Skógarfoss", Some("landmark")), Kind::Waterfall);
        assert_eq!(kind_of("Gullfoss Waterfall", None), Kind::Waterfall);
        assert_eq!(kind_of("Malarrif Lighthouse", None), Kind::Lighthouse);
        assert_eq!(kind_of("Ólafsvíkurviti", None), Kind::Lighthouse);
        assert_eq!(kind_of("Thingvallakirkja Church", None), Kind::Church);
        assert_eq!(
            kind_of("Deildartunguhver Thermal Spring", None),
            Kind::HotSpring
        );
        assert_eq!(kind_of("Diamond Beach", None), Kind::Beach);
        assert_eq!(kind_of("Lake Mývatn", None), Kind::Lake);
        assert_eq!(kind_of("Námafjall Hverir", None), Kind::HotSpring);
        assert_eq!(kind_of("Anchor Monument", None), Kind::Star);
        assert!(strokes(Kind::Castle, 10.0, 10.0, 20.0)
            .iter()
            .all(|l| l.len() >= 2));
    }
}
