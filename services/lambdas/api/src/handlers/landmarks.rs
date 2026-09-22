//! Landmark facts (F-210): what a viewer learns about a place in their photos. The facts
//! are public knowledge fetched at ingest and shared by every household; trivia lines are
//! written by the Concierge from the summary and stored only when the summary backs them.

use memorylane_core::ids::HouseholdId;
use memorylane_core::landmark::{self, LandmarkFacts};
use memorylane_core::media::MediaItem;
use memorylane_core::trip::Trip;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

/// What a screen shows about one landmark.
pub fn view(f: &LandmarkFacts) -> Value {
    json!({
        "key": f.key,
        "name": f.name,
        "title": f.title,
        "tagline": f.tagline,
        "lines": f.lines(),
        "summary": f.summary,
        "has_trivia": !f.trivia.is_empty(),
        "credit": f.credit,
        "url": f.url,
    })
}

/// Every landmark name a trip carries: route sights first, then what photos show.
pub fn names_in(trip: &Trip, media: &[MediaItem]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut push = |n: &str| {
        if !names.iter().any(|x| landmark::key(x) == landmark::key(n)) {
            names.push(n.to_owned());
        }
    };
    for l in trip.landmarks() {
        if l.kind.as_deref() != Some("city") {
            push(&l.name);
        }
    }
    for m in media {
        if let Some(l) = &m.enrichment.landmark {
            push(&l.name);
        }
    }
    names
}

/// Stored facts for the trip's landmarks, keyed by landmark key. Landmarks nobody has
/// facts for are left out.
pub async fn facts_for(
    app: &App,
    trip: &Trip,
    media: &[MediaItem],
) -> Result<BTreeMap<String, Value>, ApiError> {
    let mut out = BTreeMap::new();
    for name in names_in(trip, media) {
        let key = landmark::key(&name);
        if out.contains_key(&key) {
            continue;
        }
        if let Some(f) = app.storage.get_landmark_facts(&key).await? {
            out.insert(key, view(&f));
        }
    }
    Ok(out)
}

/// Whether the household has a trip or photo at this landmark: facts are public, but a
/// household only reads and annotates the landmarks it has been to.
async fn visited(app: &App, household: &HouseholdId, key: &str) -> Result<bool, ApiError> {
    let media = app.storage.list_media(household).await?;
    if media.iter().any(|m| {
        m.enrichment
            .landmark
            .as_ref()
            .is_some_and(|l| landmark::key(&l.name) == key)
    }) {
        return Ok(true);
    }
    Ok(app
        .storage
        .list_trips(household)
        .await?
        .iter()
        .any(|t| t.landmarks().iter().any(|l| landmark::key(&l.name) == key)))
}

async fn load(app: &App, req: &Req, key: &str) -> Result<LandmarkFacts, ApiError> {
    let household = app.owner(req)?;
    let key = landmark::key(key);
    if !visited(app, &household, &key).await? {
        return Err(ApiError::not_found("no such landmark in this household"));
    }
    app.storage
        .get_landmark_facts(&key)
        .await?
        .ok_or_else(|| ApiError::not_found("nothing known about this landmark yet"))
}

/// `GET /landmarks/{key}`: the `get_landmark_facts` tool, and the TV's "about this place".
pub async fn get(app: &App, req: &Req, key: &str) -> Result<Resp, ApiError> {
    Resp::ok(view(&load(app, req, key).await?))
}

#[derive(Deserialize)]
struct TriviaBody {
    lines: Vec<String>,
}

/// `POST /landmarks/{key}/trivia`: the `save_landmark_trivia` tool. Lines that say more
/// than the summary are refused with 422 and the reasons, and nothing is stored.
pub async fn save_trivia(app: &App, req: &Req, key: &str) -> Result<Resp, ApiError> {
    let mut facts = load(app, req, key).await?;
    let body: TriviaBody = req.json()?;
    let problems = facts.set_trivia(&body.lines);
    if !problems.is_empty() {
        return Err(ApiError::new(
            422,
            format!("trivia refused: {}", problems.join("; ")),
        ));
    }
    app.storage.put_landmark_facts(&facts).await?;
    Resp::ok(view(&facts))
}

/// Add to a trip's JSON what its landmarks are (F-210): `landmark_facts` by key, each
/// day's `landmark_keys`, and `media_tags`, per photo the landmark it shows and, for the
/// trip's own household only, the people in it (F-704).
pub async fn annotate(
    app: &App,
    trip: &Trip,
    media: &[MediaItem],
    with_people: bool,
    v: &mut Value,
) -> Result<(), ApiError> {
    let facts = facts_for(app, trip, media).await?;
    if let Some(days) = v["days"].as_array_mut() {
        for (day, out) in trip.days.iter().zip(days.iter_mut()) {
            // What its photos show first, then what the route passed.
            let from_photos = media
                .iter()
                .filter(|m| day.media.contains(&m.id))
                .filter_map(|m| m.enrichment.landmark.as_ref())
                .map(|l| landmark::key(&l.name));
            let from_route = day
                .landmarks
                .iter()
                .filter(|l| l.kind.as_deref() != Some("city"))
                .map(|l| landmark::key(&l.name));
            let mut keys: Vec<String> = Vec::new();
            for k in from_photos.chain(from_route) {
                if facts.contains_key(&k) && !keys.contains(&k) {
                    keys.push(k);
                }
            }
            out["landmark_keys"] = json!(keys);
        }
    }
    v["landmark_facts"] = json!(facts);
    let tags: serde_json::Map<String, Value> = media
        .iter()
        .filter(|m| m.enrichment.landmark.is_some() || (with_people && !m.people.is_empty()))
        .map(|m| {
            let people = if with_people {
                m.people.clone()
            } else {
                vec![]
            };
            let at = m
                .enrichment
                .landmark
                .as_ref()
                .map(|l| landmark::key(&l.name));
            (
                m.id.to_string(),
                json!({ "landmark": at, "people": people }),
            )
        })
        .collect();
    v["media_tags"] = Value::Object(tags);
    Ok(())
}
