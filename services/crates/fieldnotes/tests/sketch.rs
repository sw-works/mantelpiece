//! Sketch maps end to end on the Florence fixture tiles, no network.

use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::ids::{HouseholdId, MediaId, TripId};
use mantelpiece_core::sketch::Which;
use mantelpiece_core::trip::{Landmark, Route, RoutePoint, Trip, TripDay};
use mantelpiece_fieldnotes::geo::TileId;
use mantelpiece_fieldnotes::{sketch, Skip, Tiles};

/// The one Florence tile pair, served for whatever tile is asked: these tests are about
/// determinism, forms and fingerprints, not where the geography lands.
struct Fixtures {
    fail: bool,
}

impl Tiles for Fixtures {
    fn vector(&self, id: TileId) -> Result<Option<Vec<u8>>, String> {
        if self.fail {
            return Err("offline".into());
        }
        let _ = id;
        let dir = env!("CARGO_MANIFEST_DIR");
        Ok(std::fs::read(format!("{dir}/tests/fixtures/omt-13-4352-2986.pbf")).ok())
    }
    fn terrain(&self, id: TileId) -> Result<Option<Vec<u8>>, String> {
        let _ = id;
        let dir = env!("CARGO_MANIFEST_DIR");
        Ok(std::fs::read(format!("{dir}/tests/fixtures/terrarium-12-2176-1493.png")).ok())
    }
}

fn point(lat: f64, lon: f64, n: u32, estimated: bool) -> RoutePoint {
    RoutePoint {
        at: GeoPoint::new(lat, lon),
        time: time::OffsetDateTime::UNIX_EPOCH + time::Duration::hours(i64::from(n)),
        media: MediaId::new(format!("m{n}")),
        estimated,
    }
}

fn day(index: u32, points: Vec<RoutePoint>) -> TripDay {
    TripDay {
        index,
        date: time::macros::date!(2026 - 04 - 03),
        media: vec![],
        route: Route {
            points,
            inferred: false,
            map_key: None,
        },
        title: Some(format!("Day {index}: Florence")),
        story: None,
        undated: vec![],
        landmarks: vec![Landmark {
            name: "Ponte Vecchio".into(),
            at: GeoPoint::new(43.7679, 11.2531),
            distance_m: 10.0,
            source: "aws".into(),
            kind: Some("landmark".into()),
            dim_m: None,
            media: vec![MediaId::new("m1")],
            country: Some("Italy".into()),
        }],
    }
}

fn trip() -> Trip {
    Trip {
        id: TripId::new("t"),
        household: HouseholdId::new("h"),
        name: "Florence".into(),
        start: time::OffsetDateTime::UNIX_EPOCH,
        end: time::OffsetDateTime::UNIX_EPOCH,
        days: vec![
            day(
                1,
                vec![
                    point(43.7700, 11.2400, 1, false),
                    point(43.7679, 11.2531, 2, false),
                ],
            ),
            day(
                2,
                vec![
                    point(43.7660, 11.2600, 3, true),
                    point(43.7720, 11.2650, 4, false),
                ],
            ),
        ],
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
fn a_day_map_draws_the_same_picture_every_time() {
    let t = trip();
    let a = sketch::draw(&t, Which::Day(1), None, &Fixtures { fail: false }).expect("a map");
    let b = sketch::draw(&t, Which::Day(1), None, &Fixtures { fail: false }).expect("a map");
    assert_eq!(a.owner, b.owner, "byte-identical (FR-006)");
    assert_eq!(a.fingerprint, b.fingerprint);
    assert!(a.credit.contains("OpenStreetMap"));
    let pic = image::load_from_memory(&a.owner).unwrap();
    assert_eq!((pic.width(), pic.height()), (sketch::W, sketch::H));
}

#[test]
fn the_public_form_leaves_out_home_and_the_owner_form_marks_it() {
    let t = trip();
    let home = Some(GeoPoint::new(43.7702, 11.2402));
    let s = sketch::draw(&t, Which::Trip, home, &Fixtures { fail: false }).expect("a map");
    assert_ne!(s.owner, s.public, "only the owner form has the home wash");
    let none = sketch::draw(&t, Which::Trip, None, &Fixtures { fail: false }).expect("a map");
    assert_eq!(none.owner, none.public, "without a home the forms agree");
}

#[test]
fn fingerprints_follow_the_route_and_home_only() {
    let t = trip();
    let f = sketch::fingerprint(&t, Which::Day(1), None);
    assert_eq!(f, sketch::fingerprint(&t.clone(), Which::Day(1), None));
    let mut moved = t.clone();
    moved.days[0].route.points[0].at = GeoPoint::new(43.7800, 11.2400);
    assert_ne!(f, sketch::fingerprint(&moved, Which::Day(1), None));
    assert_eq!(
        sketch::fingerprint(&t, Which::Day(2), None),
        sketch::fingerprint(&moved, Which::Day(2), None),
        "another day's change leaves this day alone"
    );
    assert_ne!(
        f,
        sketch::fingerprint(&t, Which::Day(1), Some(GeoPoint::new(43.77, 11.24)))
    );
}

#[test]
fn nothing_to_draw_from_means_no_map() {
    let mut t = trip();
    for d in &mut t.days {
        d.route.points.clear();
        d.landmarks.clear();
    }
    assert!(matches!(
        sketch::draw(&t, Which::Day(1), None, &Fixtures { fail: false }),
        Err(Skip::NoPlace)
    ));
    assert!(matches!(
        sketch::draw(&trip(), Which::Day(1), None, &Fixtures { fail: true }),
        Err(Skip::MapUnavailable)
    ));
}
