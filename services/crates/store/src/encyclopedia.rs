//! What an encyclopedia says about a named landmark (F-210). The name comes from a photo
//! or a route; the answer is accepted only when it is the same place: an article whose
//! coordinates lie near where the photo was taken, or, with no position to go on, an
//! article whose title is the name. Wikipedia for real runs, a fixed list for tests.

use async_trait::async_trait;
use memorylane_core::geo::GeoPoint;
use memorylane_core::landmark;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::{Result, StorageError};

/// An article further than this from the photo is about somewhere else.
pub const SAME_PLACE_M: f64 = 15_000.0;
const MAX_SUMMARY_CHARS: usize = 1_200;
pub const WIKIPEDIA_CREDIT: &str = "Wikipedia, CC BY-SA 4.0";

#[derive(Clone, Debug, PartialEq)]
pub struct Article {
    pub title: String,
    pub tagline: Option<String>,
    pub summary: String,
    pub url: String,
    pub credit: String,
    pub at: Option<GeoPoint>,
}

#[async_trait]
pub trait Encyclopedia: Send + Sync {
    async fn lookup(&self, name: &str, near: Option<GeoPoint>) -> Result<Option<Article>>;
}

/// Pick the article that is about `name`, from candidates in the source's own order.
pub fn pick(name: &str, near: Option<GeoPoint>, candidates: Vec<Article>) -> Option<Article> {
    let wanted = landmark::key(name);
    candidates.into_iter().find(|a| {
        if a.summary.trim().is_empty() || a.title.starts_with("List of") {
            return false;
        }
        match (near, a.at) {
            (Some(here), Some(there)) => here.distance_m(there) <= SAME_PLACE_M,
            // A position but an article without one: only the exact title will do.
            // No position at all: the same.
            _ => landmark::key(&a.title) == wanted,
        }
    })
}

/// Answers from a fixed list, through the same `pick` rule.
#[derive(Debug, Default)]
pub struct MemoryEncyclopedia {
    pub articles: Vec<Article>,
    pub calls: Mutex<Vec<String>>,
}

impl MemoryEncyclopedia {
    #[must_use]
    /// Articles as `(title, summary, position)`.
    #[allow(clippy::type_complexity)]
    pub fn with(articles: Vec<(&str, &str, Option<(f64, f64)>)>) -> Self {
        Self {
            articles: articles
                .into_iter()
                .map(|(title, summary, at)| Article {
                    title: title.into(),
                    tagline: None,
                    summary: summary.into(),
                    url: format!("https://example.test/wiki/{}", title.replace(' ', "_")),
                    credit: "Test encyclopedia".into(),
                    at: at.map(|(lat, lon)| GeoPoint::new(lat, lon)),
                })
                .collect(),
            calls: Mutex::default(),
        }
    }
}

#[async_trait]
impl Encyclopedia for MemoryEncyclopedia {
    async fn lookup(&self, name: &str, near: Option<GeoPoint>) -> Result<Option<Article>> {
        self.calls.lock().unwrap().push(name.to_owned());
        let wanted = landmark::key(name);
        // Like a search: titles sharing a word with the name, best match first.
        let mut hits: Vec<Article> = self
            .articles
            .iter()
            .filter(|a| {
                let title = landmark::key(&a.title);
                title
                    .split('-')
                    .any(|w| wanted.split('-').any(|n| n == w && w.len() > 2))
            })
            .cloned()
            .collect();
        hits.sort_by_key(|a| landmark::key(&a.title) != wanted);
        Ok(pick(name, near, hits))
    }
}

/// Knows nothing: landmarks stay names.
#[derive(Debug, Default)]
pub struct NoEncyclopedia;

#[async_trait]
impl Encyclopedia for NoEncyclopedia {
    async fn lookup(&self, _name: &str, _near: Option<GeoPoint>) -> Result<Option<Article>> {
        Ok(None)
    }
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

/// English Wikipedia: one search request returns titles, lead text, coordinates and
/// descriptions together. One request a second per Wikimedia's etiquette. Response shape
/// verified against the live API on 2026-09-21.
#[derive(Debug)]
pub struct Wikipedia {
    endpoint: String,
    user_agent: String,
    last_call: Mutex<Option<Instant>>,
}

impl Default for Wikipedia {
    fn default() -> Self {
        Self::new("https://en.wikipedia.org/w/api.php")
    }
}

impl Wikipedia {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            user_agent: "Mantelpiece/0.1 (https://github.com/hanyuxiao/memory-lane)".into(),
            last_call: Mutex::default(),
        }
    }

    /// Candidates from a `generator=search` response, in search order.
    pub fn parse(body: &serde_json::Value) -> Vec<Article> {
        let mut pages: Vec<&serde_json::Value> = body["query"]["pages"]
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        pages.sort_by_key(|p| p["index"].as_u64().unwrap_or(u64::MAX));
        pages
            .into_iter()
            .filter(|p| p["pageprops"]["disambiguation"].is_null())
            .filter_map(|p| {
                let summary: String = p["extract"]
                    .as_str()?
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let summary = if summary.chars().count() > MAX_SUMMARY_CHARS {
                    let cut: String = summary.chars().take(MAX_SUMMARY_CHARS).collect();
                    // End on a whole sentence.
                    cut.rfind(". ")
                        .map_or(cut.clone(), |i| cut[..=i].to_owned())
                } else {
                    summary
                };
                let at = p["coordinates"].as_array().and_then(|c| {
                    let c = c.first()?;
                    Some(GeoPoint::new(c["lat"].as_f64()?, c["lon"].as_f64()?))
                });
                Some(Article {
                    title: p["title"].as_str()?.to_owned(),
                    tagline: p["description"]
                        .as_str()
                        .filter(|d| !d.is_empty())
                        .map(str::to_owned),
                    summary,
                    url: p["fullurl"].as_str()?.to_owned(),
                    credit: WIKIPEDIA_CREDIT.into(),
                    at,
                })
            })
            .collect()
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[async_trait]
impl Encyclopedia for Wikipedia {
    async fn lookup(&self, name: &str, near: Option<GeoPoint>) -> Result<Option<Article>> {
        let wait = {
            let last = self.last_call.lock().unwrap();
            last.map(|t| Duration::from_secs(1).saturating_sub(t.elapsed()))
        };
        if let Some(w) = wait.filter(|w| !w.is_zero()) {
            tokio::time::sleep(w).await;
        }
        *self.last_call.lock().unwrap() = Some(Instant::now());
        let url = format!(
            "{}?action=query&generator=search&gsrsearch={}&gsrlimit=5&prop=coordinates%7Cextracts%7Cdescription%7Cinfo%7Cpageprops&ppprop=disambiguation&exintro=1&explaintext=1&exlimit=5&inprop=url&redirects=1&format=json&formatversion=2",
            self.endpoint,
            percent_encode(name)
        );
        let agent = self.user_agent.clone();
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
        Ok(pick(name, near, Self::parse(&body)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response() -> serde_json::Value {
        serde_json::json!({ "query": { "pages": [
            { "index": 3, "title": "List of waterfalls", "extract": "A list.", "fullurl": "https://en.wikipedia.org/wiki/List_of_waterfalls" },
            { "index": 1, "title": "Skógafoss", "description": "Waterfall in Iceland",
              "extract": "Skógafoss is a waterfall on the Skógá River.\nIt drops 60 metres.",
              "fullurl": "https://en.wikipedia.org/wiki/Sk%C3%B3gafoss",
              "coordinates": [{ "lat": 63.5321, "lon": -19.5111 }] },
            { "index": 2, "title": "Skógar", "extract": "Skógar is a village.", "fullurl": "https://en.wikipedia.org/wiki/Sk%C3%B3gar",
              "pageprops": { "disambiguation": "" } }
        ]}})
    }

    #[test]
    fn search_results_parse_in_search_order_without_disambiguation_pages() {
        let got = Wikipedia::parse(&response());
        let titles: Vec<&str> = got.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Skógafoss", "List of waterfalls"]);
        assert_eq!(got[0].tagline.as_deref(), Some("Waterfall in Iceland"));
        assert_eq!(
            got[0].summary,
            "Skógafoss is a waterfall on the Skógá River. It drops 60 metres."
        );
        assert_eq!(got[0].credit, WIKIPEDIA_CREDIT);
    }

    #[test]
    fn an_article_counts_only_when_it_is_the_same_place() {
        let articles = Wikipedia::parse(&response());
        let at_the_falls = GeoPoint::new(63.5300, -19.5100);
        let in_kyoto = GeoPoint::new(35.0, 135.7);
        // Amazon Location's name differs from the title; the position settles it.
        assert_eq!(
            pick("Skógafoss Waterfall", Some(at_the_falls), articles.clone())
                .unwrap()
                .title,
            "Skógafoss"
        );
        assert!(pick("Skógafoss Waterfall", Some(in_kyoto), articles.clone()).is_none());
        // No position: the title must be the name.
        assert!(pick("Skógafoss Waterfall", None, articles.clone()).is_none());
        assert!(pick("skógafoss", None, articles).is_some());
        assert_eq!(
            percent_encode("Skógafoss Waterfall"),
            "Sk%C3%B3gafoss%20Waterfall"
        );
    }

    /// Hits the live API: `cargo test -p memorylane-store -- --ignored live_wikipedia`.
    #[tokio::test]
    #[ignore = "network"]
    async fn live_wikipedia_finds_the_falls_by_the_name_amazon_location_uses() {
        let w = Wikipedia::default();
        let found = w
            .lookup(
                "Skógafoss Waterfall",
                Some(GeoPoint::new(63.5300, -19.5100)),
            )
            .await
            .unwrap()
            .expect("found");
        assert_eq!(found.title, "Skógafoss");
        assert!(found.summary.contains("waterfall"), "{}", found.summary);
        let elsewhere = w
            .lookup("Skógafoss Waterfall", Some(GeoPoint::new(35.0, 135.7)))
            .await
            .unwrap();
        assert!(elsewhere.is_none());
    }
}
