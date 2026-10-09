//! The map's character from the geography in view (FR-004).

use mantelpiece_core::field_note::MapMode;

/// Coastline inside the box, in box widths, above which the map is about the coast.
pub const COAST_LEN: f32 = 0.6;
/// Relief in view above which the map is about the mountains.
pub const MOUNTAIN_RELIEF_M: f64 = 300.0;
/// Road length inside the box, in box widths, above which the map is a town plan.
pub const TOWN_ROADS: f32 = 6.0;

/// Coast, then mountains, then town, else trail.
#[must_use]
pub fn mode(coast_len: f32, relief_m: f64, road_len: f32) -> MapMode {
    if coast_len >= COAST_LEN {
        MapMode::Coast
    } else if relief_m >= MOUNTAIN_RELIEF_M {
        MapMode::Mountains
    } else if road_len >= TOWN_ROADS {
        MapMode::Town
    } else {
        MapMode::Trail
    }
}

/// Metres across the map for a mode, before widening for an uncertain place.
#[must_use]
pub fn width_m(mode: MapMode) -> f64 {
    match mode {
        MapMode::Town => 2_500.0,
        MapMode::Coast | MapMode::Trail => 8_000.0,
        MapMode::Mountains => 10_000.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coast_wins_then_mountains_then_town() {
        assert_eq!(mode(1.2, 900.0, 20.0), MapMode::Coast);
        assert_eq!(mode(0.0, 900.0, 20.0), MapMode::Mountains);
        assert_eq!(mode(0.0, 40.0, 20.0), MapMode::Town);
        assert_eq!(mode(0.0, 40.0, 1.0), MapMode::Trail);
    }
}
