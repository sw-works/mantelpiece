//! Sketch maps of a stored trip, with real map data over HTTPS (no AWS).
//!
//!     cargo run --release -p mantelpiece-fieldnotes --example sketch -- <trip.json> <out-dir> [lat,lon home]
//!
//! `trip.json` is a trip record as the API returns it. Writes `trip.jpg`, `trip-public.jpg`
//! and `day-N.jpg` per day, and prints timings. Tiles are cached under `<out-dir>/tiles`.

use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::sketch::Which;
use mantelpiece_core::trip::Trip;
use mantelpiece_fieldnotes::geo::TileId;
use mantelpiece_fieldnotes::{sketch, Tiles};
use std::path::PathBuf;
use std::time::Instant;

struct Http {
    cache: PathBuf,
    vector_template: String,
}

fn get(url: &str) -> Result<Option<Vec<u8>>, String> {
    match ureq::get(url)
        .header("User-Agent", "Mantelpiece field notes spike")
        .call()
    {
        Ok(mut r) => r
            .body_mut()
            .with_config()
            .limit(10 << 20)
            .read_to_vec()
            .map(Some)
            .map_err(|e| e.to_string()),
        Err(ureq::Error::StatusCode(404 | 204)) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

impl Http {
    fn cached(&self, kind: &str, id: TileId, url: &str) -> Result<Option<Vec<u8>>, String> {
        let path = self
            .cache
            .join(format!("{kind}-{}-{}-{}", id.z, id.x, id.y));
        if let Ok(b) = std::fs::read(&path) {
            return Ok((!b.is_empty()).then_some(b));
        }
        let got = get(url)?;
        let _ = std::fs::write(&path, got.as_deref().unwrap_or_default());
        Ok(got)
    }
}

impl Tiles for Http {
    fn vector(&self, id: TileId) -> Result<Option<Vec<u8>>, String> {
        let url = self
            .vector_template
            .replace("{z}", &id.z.to_string())
            .replace("{x}", &id.x.to_string())
            .replace("{y}", &id.y.to_string());
        self.cached("omt", id, &url)
    }
    fn terrain(&self, id: TileId) -> Result<Option<Vec<u8>>, String> {
        let url = format!(
            "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{}/{}/{}.png",
            id.z, id.x, id.y
        );
        self.cached("dem", id, &url)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let trip: Trip = serde_json::from_slice(&std::fs::read(&args[0]).unwrap()).unwrap();
    let out = PathBuf::from(&args[1]);
    let home = args.get(2).map(|s| {
        let (a, b) = s.split_once(',').unwrap();
        GeoPoint::new(a.parse().unwrap(), b.parse().unwrap())
    });
    std::fs::create_dir_all(out.join("tiles")).unwrap();
    let tilejson: serde_json::Value = serde_json::from_slice(
        &get("https://tiles.openfreemap.org/planet")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let http = Http {
        cache: out.join("tiles"),
        vector_template: tilejson["tiles"][0].as_str().unwrap().to_owned(),
    };
    let which = std::iter::once(Which::Trip).chain(trip.days.iter().map(|d| Which::Day(d.index)));
    for w in which {
        let started = Instant::now();
        let name = match w {
            Which::Trip => "trip".to_owned(),
            Which::Day(n) => format!("day-{n}"),
        };
        match sketch::draw(&trip, w, home, &http) {
            Ok(s) => {
                std::fs::write(out.join(format!("{name}.jpg")), &s.owner).unwrap();
                std::fs::write(out.join(format!("{name}-public.jpg")), &s.public).unwrap();
                println!(
                    "{name}: {:.1}s {}",
                    started.elapsed().as_secs_f32(),
                    s.credit
                );
            }
            Err(e) => println!("{name}: skipped, {}", e.reason()),
        }
    }
}
