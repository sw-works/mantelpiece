//! `POST /search` (F-303): vector ranking inside a trip or across the household, with a
//! keyword fallback so a query still answers before embeddings exist. Single pass, no
//! model call, so the warm path stays inside the 300 ms budget (constitution V).

use mantelpiece_core::ids::{ContributorId, HouseholdId, MediaId, TripId};
use mantelpiece_core::media::MediaItem;
use mantelpiece_ingest::embed;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

pub const DEFAULT_LIMIT: usize = 24;
/// Cosine similarity below this is noise for a hashed embedding.
pub const MIN_SCORE: f32 = 0.15;

#[derive(Deserialize)]
struct SearchBody {
    query: String,
    trip: Option<TripId>,
    /// Person names to require (US9 fills this in).
    #[serde(default)]
    people: Vec<String>,
    limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct SearchHit {
    pub media: MediaId,
    pub score: f32,
    pub trip: Option<TripId>,
    pub contributor: ContributorId,
}

fn keyword_score(m: &MediaItem, terms: &[String]) -> f32 {
    if terms.is_empty() {
        return 0.0;
    }
    let mut hay: Vec<String> = m
        .enrichment
        .scene_tags
        .iter()
        .map(|t| t.name.to_lowercase())
        .collect();
    if let Some(l) = &m.enrichment.landmark {
        hay.push(l.name.to_lowercase());
    }
    if let Some(t) = &m.text {
        hay.push(t.to_lowercase());
    }
    if let Some(d) = &m.description {
        hay.push(d.text.to_lowercase());
    }
    let joined = hay.join(" ");
    let matched = terms.iter().filter(|t| joined.contains(t.as_str())).count();
    let (matched, total) = (
        u16::try_from(matched).unwrap_or(u16::MAX),
        u16::try_from(terms.len()).unwrap_or(u16::MAX),
    );
    f32::from(matched) / f32::from(total)
}

/// Ranked media for `query`, visible to `household`.
pub async fn run(
    app: &App,
    household: &HouseholdId,
    query: &str,
    trip: Option<&TripId>,
    limit: usize,
) -> Result<Vec<SearchHit>, ApiError> {
    let vector = embed::embed_query(query);
    let hits = match trip {
        Some(t) => {
            app.vectors
                .query_trip(household, t, &vector, limit * 2)
                .await?
        }
        None => {
            app.vectors
                .query_household(household, &vector, limit * 2)
                .await?
        }
    };
    let terms = embed::tokens(query);
    let mut scored: Vec<(MediaId, f32)> = hits
        .into_iter()
        .filter(|h| h.score >= MIN_SCORE)
        .map(|h| (h.media, h.score))
        .collect();
    // Keyword fallback over what the pipeline wrote down, for items without a vector.
    let all = app.storage.list_media(household).await?;
    for m in &all {
        if scored.iter().any(|(id, _)| id == &m.id) {
            continue;
        }
        if trip.is_some() && m.trip.as_ref() != trip {
            continue;
        }
        let k = keyword_score(m, &terms);
        if k > 0.0 {
            scored.push((m.id.clone(), k * 0.9));
        }
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut out = Vec::new();
    for (id, score) in scored {
        let Some(m) = all.iter().find(|m| m.id == id) else {
            continue;
        };
        if m.is_removed() || m.hidden_for(household) {
            continue;
        }
        out.push(SearchHit {
            media: m.id.clone(),
            score,
            trip: m.trip.clone(),
            contributor: m.contributor.clone(),
        });
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

pub async fn search(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let body: SearchBody = req.json()?;
    if body.query.trim().is_empty() {
        return Err(ApiError::bad_request("query required"));
    }
    let limit = body.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 200);
    let mut hits = run(
        app,
        caller.household(),
        &body.query,
        body.trip.as_ref(),
        limit * 4,
    )
    .await?;
    // F-704: "Maya at the beach" keeps only photos every named person is on.
    if !body.people.is_empty() {
        let wanted =
            crate::handlers::people::resolve_names(app, caller.household(), &body.people).await?;
        let all = app.storage.list_media(caller.household()).await?;
        hits.retain(|h| {
            all.iter()
                .find(|m| m.id == h.media)
                .is_some_and(|m| wanted.iter().all(|p| m.people.contains(p)))
        });
    }
    hits.truncate(limit);
    Resp::ok(json!({ "query": body.query, "people": body.people, "results": hits }))
}
