//! Named places near a position (F-206, constitution V): a lookup that runs before any
//! model. Three implementations: in-memory for tests, Wikipedia's geosearch for runs with
//! no AWS account (public, no key, one request a second), and Amazon Location Places for
//! the deployed stack.

use async_trait::async_trait;
use memorylane_core::geo::GeoPoint;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::{Result, StorageError};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    pub at: GeoPoint,
    /// Metres from the query position.
    pub distance_m: f64,
    /// `wikipedia`, `aws`.
    pub source: String,
    /// Provider's type, when given (Wikipedia: `city`, `landmark`, `mountain`, `edu`...).
    #[serde(default)]
    pub kind: Option<String>,
    /// Provider's size estimate in metres, when given.
    #[serde(default)]
    pub dim_m: Option<u32>,
    /// Country the place lies in, when the provider says: a trip across many towns is
    /// named after it.
    #[serde(default)]
    pub country: Option<String>,
}

#[async_trait]
pub trait Places: Send + Sync {
    /// Up to `limit` named places within `radius_m` of `at`, nearest first.
    async fn nearby(&self, at: GeoPoint, radius_m: u32, limit: usize) -> Result<Vec<Place>>;
}

/// Answers from a fixed list: every configured place within the radius.
#[derive(Debug, Default)]
pub struct MemoryPlaces {
    pub places: Vec<Place>,
    pub calls: Mutex<Vec<GeoPoint>>,
}

impl MemoryPlaces {
    #[must_use]
    pub fn with(places: Vec<(&str, f64, f64)>) -> Self {
        Self {
            places: places
                .into_iter()
                .map(|(name, lat, lon)| Place {
                    name: name.into(),
                    at: GeoPoint::new(lat, lon),
                    distance_m: 0.0,
                    source: "memory".into(),
                    kind: None,
                    dim_m: None,
                    country: None,
                })
                .collect(),
            calls: Mutex::default(),
        }
    }
}

#[async_trait]
impl Places for MemoryPlaces {
    async fn nearby(&self, at: GeoPoint, radius_m: u32, limit: usize) -> Result<Vec<Place>> {
        self.calls.lock().unwrap().push(at);
        let mut hits: Vec<Place> = self
            .places
            .iter()
            .map(|p| Place {
                distance_m: at.distance_m(p.at),
                ..p.clone()
            })
            .filter(|p| p.distance_m <= f64::from(radius_m))
            .collect();
        hits.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
        hits.truncate(limit);
        Ok(hits)
    }
}

/// No lookups at all: routes without names.
#[derive(Debug, Default)]
pub struct NoPlaces;

#[async_trait]
impl Places for NoPlaces {
    async fn nearby(&self, _at: GeoPoint, _radius_m: u32, _limit: usize) -> Result<Vec<Place>> {
        Ok(vec![])
    }
}

/// Whether an Amazon Location category names somewhere people travel to see.
pub fn is_sight(category: &str) -> bool {
    const SIGHTS: &[&str] = &[
        "attraction",
        "landmark",
        "monument",
        "histor",
        "museum",
        "gallery",
        "castle",
        "palace",
        "ruin",
        "church",
        "cathedral",
        "temple",
        "mosque",
        "shrine",
        "religious",
        "park",
        "garden",
        "beach",
        "lake",
        "river",
        "waterfall",
        "body_of_water",
        "body of water",
        "mountain",
        "hill",
        "volcano",
        "glacier",
        "island",
        "forest",
        "natur",
        "cave",
        "canyon",
        "geyser",
        "hot_spring",
        "hot spring",
        "lighthouse",
        "bridge",
        "harbor",
        "harbour",
        "viewpoint",
        "scenic",
        "zoo",
        "aquarium",
        "stadium",
        "theat",
        "winery",
    ];
    let c = category.to_lowercase();
    SIGHTS.iter().any(|s| c.contains(s))
        && !c.contains("parking")
        && !c.contains("amusement_arcade")
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

/// Wikipedia geosearch: articles with coordinates near a point. Good for landmarks,
/// villages and lochs; not for shops. Cached per 100 m cell for the process lifetime and
/// throttled to one request a second per Wikimedia's etiquette. Verified against the live
/// API on 2026-09-10.
#[derive(Debug)]
pub struct WikipediaPlaces {
    endpoint: String,
    user_agent: String,
    cache: Mutex<HashMap<(i64, i64, u32), Vec<Place>>>,
    last_call: Mutex<Option<Instant>>,
}

impl Default for WikipediaPlaces {
    fn default() -> Self {
        Self::new("https://en.wikipedia.org/w/api.php")
    }
}

impl WikipediaPlaces {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            user_agent: "MemoryLane/0.1 (https://github.com/hanyuxiao/memory-lane)".into(),
            cache: Mutex::default(),
            last_call: Mutex::default(),
        }
    }

    /// Parse a geosearch response body into places, nearest first.
    pub fn parse(body: &serde_json::Value) -> Vec<Place> {
        let mut places: Vec<Place> = body["query"]["geosearch"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| {
                        Some(Place {
                            name: e["title"].as_str()?.to_owned(),
                            at: GeoPoint::new(e["lat"].as_f64()?, e["lon"].as_f64()?),
                            distance_m: e["dist"].as_f64().unwrap_or(0.0),
                            source: "wikipedia".into(),
                            kind: e["type"].as_str().map(str::to_owned),
                            dim_m: e["dim"].as_u64().and_then(|d| u32::try_from(d).ok()),
                            country: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        places.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
        places
    }

    fn cell(at: GeoPoint, radius_m: u32) -> (i64, i64, u32) {
        // 0.001 degrees is roughly 100 m; one lookup per cell and radius.
        #[allow(clippy::cast_possible_truncation)]
        let key = (
            (at.lat * 1000.0).round() as i64,
            (at.lon * 1000.0).round() as i64,
        );
        (key.0, key.1, radius_m)
    }
}

#[async_trait]
impl Places for WikipediaPlaces {
    async fn nearby(&self, at: GeoPoint, radius_m: u32, limit: usize) -> Result<Vec<Place>> {
        let key = Self::cell(at, radius_m);
        if let Some(hit) = self.cache.lock().unwrap().get(&key) {
            let mut v = hit.clone();
            v.truncate(limit);
            return Ok(v);
        }
        let wait = {
            let last = self.last_call.lock().unwrap();
            last.map(|t| Duration::from_secs(1).saturating_sub(t.elapsed()))
        };
        if let Some(w) = wait.filter(|w| !w.is_zero()) {
            tokio::time::sleep(w).await;
        }
        let url = format!(
            "{}?action=query&list=geosearch&gscoord={:.6}%7C{:.6}&gsradius={}&gslimit={}&gsprop=type%7Cdim&format=json",
            self.endpoint,
            at.lat,
            at.lon,
            radius_m.clamp(10, 10_000),
            limit.clamp(1, 50)
        );
        let agent = self.user_agent.clone();
        *self.last_call.lock().unwrap() = Some(Instant::now());
        let body: serde_json::Value = tokio::task::spawn_blocking(move || {
            ureq::get(&url)
                .header("User-Agent", &agent)
                .call()
                .map_err(backend)?
                .body_mut()
                .read_json::<serde_json::Value>()
                .map_err(backend)
        })
        .await
        .map_err(backend)??;
        let places = Self::parse(&body);
        self.cache.lock().unwrap().insert(key, places.clone());
        let mut v = places;
        v.truncate(limit);
        Ok(v)
    }
}

/// Amazon Location Places (Geo Places API) `SearchNearby`. Compile-checked only.
#[derive(Debug, Clone)]
pub struct AwsPlaces {
    client: aws_sdk_geoplaces::Client,
}

impl AwsPlaces {
    pub fn from_config(config: &aws_config::SdkConfig) -> Self {
        Self {
            client: aws_sdk_geoplaces::Client::new(config),
        }
    }
}

#[async_trait]
impl Places for AwsPlaces {
    async fn nearby(&self, at: GeoPoint, radius_m: u32, limit: usize) -> Result<Vec<Place>> {
        let out = self
            .client
            .search_nearby()
            .query_position(at.lon)
            .query_position(at.lat)
            .query_radius(i64::from(radius_m))
            // "Iceland", not "Ísland": names the story and the TV can use as they are.
            .language("en")
            .max_results(i32::try_from(limit).unwrap_or(10))
            .send()
            .await
            .map_err(backend)?;
        // Amazon Location answers with shops, bus stops and street addresses. None of those
        // names a day; the town they share does. So the town goes in once as a city, points
        // of interest stay as minor places, and streets and addresses are dropped.
        let mut places: Vec<Place> = Vec::new();
        for r in out.result_items() {
            let country = r
                .address()
                .and_then(|a| a.country())
                .and_then(|c| c.name())
                .map(str::to_owned);
            if let Some(town) = r.address().and_then(|a| a.locality()) {
                if !town.is_empty() && !places.iter().any(|p| p.name == town) {
                    places.push(Place {
                        name: town.to_owned(),
                        at,
                        distance_m: 0.0,
                        source: "aws".into(),
                        kind: Some("city".into()),
                        dim_m: Some(10_000),
                        country: country.clone(),
                    });
                }
            }
            // A taxi rank or a guesthouse near the route is not somewhere the trip went, and a
            // story told from such names invents meals and stays. Sights only.
            let sight = r
                .categories()
                .iter()
                .any(|c| is_sight(c.name()) || is_sight(c.id()));
            if r.place_type().as_str() != "PointOfInterest" || !sight {
                continue;
            }
            let pos = r.position();
            let (Some(lon), Some(lat)) = (pos.first(), pos.get(1)) else {
                continue;
            };
            let place_at = GeoPoint::new(*lat, *lon);
            places.push(Place {
                name: r.title().to_owned(),
                at: place_at,
                distance_m: at.distance_m(place_at),
                source: "aws".into(),
                kind: Some("landmark".into()),
                dim_m: None,
                country: country.clone(),
            });
        }
        places.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
        Ok(places)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_places_answer_within_the_radius_nearest_first() {
        let p = MemoryPlaces::with(vec![
            ("Urquhart Castle", 57.3241, -4.4421),
            ("Loch Ness", 57.3, -4.45),
            ("Edinburgh Castle", 55.9486, -3.1999),
        ]);
        let hits = p
            .nearby(GeoPoint::new(57.32, -4.44), 5_000, 5)
            .await
            .unwrap();
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["Urquhart Castle", "Loch Ness"]);
        assert!(hits[0].distance_m < hits[1].distance_m);
    }

    #[test]
    fn only_sights_survive_from_amazon_location_categories() {
        for yes in [
            "Tourist Attraction",
            "Waterfall",
            "church",
            "Historical Monument",
            "Body of Water",
            "national_park",
        ] {
            assert!(is_sight(yes), "{yes}");
        }
        for no in [
            "Taxi Stand",
            "Hotel",
            "Guest House",
            "Casual Dining",
            "Parking Lot",
            "Park & Ride Parking",
            "Bus Stop",
            "Social Service",
        ] {
            assert!(!is_sight(no), "{no}");
        }
    }

    #[test]
    fn wikipedia_geosearch_parses_nearest_first() {
        let body = serde_json::json!({ "query": { "geosearch": [
            { "title": "Dochgarroch", "lat": 57.43576, "lon": -4.30183, "dist": 1473.9, "type": "city", "dim": 10000 },
            { "title": "Loch Dochfour", "lat": 57.4167, "lon": -4.325, "dist": 1062.2 },
            { "title": "broken", "lat": "x" }
        ]}});
        let places = WikipediaPlaces::parse(&body);
        let names: Vec<&str> = places.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Loch Dochfour", "Dochgarroch"]);
        assert_eq!(places[0].source, "wikipedia");
        assert_eq!(places[1].kind.as_deref(), Some("city"));
        assert_eq!(places[1].dim_m, Some(10_000));
    }
}
