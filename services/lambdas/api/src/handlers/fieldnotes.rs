//! Field notes on the API (F-711): the full-resolution share and "This place".

use mantelpiece_core::field_note::{Certainty, FieldNote};
use mantelpiece_core::ids::{MediaId, TripId};
use mantelpiece_core::landmark;
use mantelpiece_core::media::MediaItem;
use mantelpiece_fieldnotes::drawing::Drawing;
use mantelpiece_fieldnotes::raster;
use mantelpiece_ingest::{decode, renditions};
use serde_json::{json, Value};

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

/// Photos nearby for "This place".
const NEARBY: usize = 12;
/// Around a dot, "the same place" is this close.
const SAME_PLACE_M: f64 = 1_000.0;

fn presigned(url: &str) -> Value {
    json!({ "url": url, "expires_in": mantelpiece_store::blobs::PRESIGN_TTL.whole_seconds() })
}

/// `POST /media/{id}/fieldnote/full`: the original with its note composed in, at full
/// resolution, for sharing (FR-020). Owner or co-editor of the trip. Kept while the note's
/// fingerprint is unchanged; the field-note task deletes it when the note is remade.
pub async fn full(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let household = caller.household().clone();
    let media = app
        .storage
        .get_media(&household, &MediaId::new(id))
        .await?
        .filter(|m| !m.is_removed())
        .ok_or_else(|| ApiError::not_found("unknown media"))?;
    let trip = media
        .trip
        .clone()
        .ok_or_else(|| ApiError::not_found("no field note"))?;
    app.owner_or_co_editor(req, &trip).await?;
    let note = media
        .field_note
        .clone()
        .ok_or_else(|| ApiError::not_found("no field note"))?;
    let (h, m) = (household.as_str(), media.id.as_str());
    let full_key = FieldNote::full_key(h, m);
    let marker = format!("{full_key}.fingerprint");
    if app.blobs.get(&marker).await.ok().as_deref() == Some(note.fingerprint.as_bytes()) {
        return Resp::ok(presigned(&app.blobs.presign_get(&full_key).await?));
    }
    let drawing: Drawing = serde_json::from_slice(&app.blobs.get(&note.drawing_key).await?)
        .map_err(|e| ApiError::internal(format!("drawing: {e}")))?;
    let original = app.blobs.get(&media.original_key).await?;
    let orientation = media.orientation;
    let jpeg = tokio::task::spawn_blocking(move || {
        let photo =
            decode::decode(&original).map_err(|e| ApiError::internal(format!("original: {e}")))?;
        let photo = renditions::upright(photo, orientation);
        let layer = raster::layer(&drawing, note.placement, photo.width(), photo.height());
        let composed = raster::compose(&photo, &layer);
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92)
            .encode_image(&composed)
            .map_err(|e| ApiError::internal(format!("encode: {e}")))?;
        Ok::<_, ApiError>(out)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))??;
    app.blobs.put(&full_key, jpeg, "image/jpeg").await?;
    app.blobs
        .put(&marker, note.fingerprint.into_bytes(), "text/plain")
        .await?;
    Resp::ok(presigned(&app.blobs.presign_get(&full_key).await?))
}

/// "Tuesday 30 June 2015, 18:42" in the place's own time, or the date alone when the
/// camera recorded no zone.
fn taken(media: &MediaItem) -> Option<String> {
    let c = media.captured?;
    let t = c.local();
    let date = format!("{} {} {} {}", t.weekday(), t.day(), t.month(), t.year());
    Some(if c.zone_assumed {
        date
    } else {
        format!("{date}, {:02}:{:02}", t.hour(), t.minute())
    })
}

/// `GET /trips/{trip}/places/{media}`: what "This place" shows for a highlight's note
/// (US6). Member households get the public form: no coordinates, "Around ...".
pub async fn place(app: &App, req: &Req, trip_id: &str, media_id: &str) -> Result<Resp, ApiError> {
    let trip_id = TripId::new(trip_id);
    let access = app.trip_access(req, &trip_id).await?;
    let owner = access.owner_household().clone();
    let own = access.viewing_household() == &owner;
    let trip = app
        .storage
        .get_trip(&owner, &trip_id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    let all = app.storage.list_media(&owner).await?;
    let media = all
        .iter()
        .find(|m| m.id.as_str() == media_id && m.trip.as_ref() == Some(&trip_id) && !m.is_removed())
        .ok_or_else(|| ApiError::not_found("unknown media"))?;
    let note = media
        .field_note
        .as_ref()
        .ok_or_else(|| ApiError::not_found("no field note"))?;
    let precise = own && note.certainty.is_precise();
    let (at, radius) = match note.certainty {
        Certainty::Precise { at } => (at, SAME_PLACE_M),
        Certainty::Approximate { at, radius_m } => (at, f64::from(radius_m).max(SAME_PLACE_M)),
    };
    let name = note
        .feature
        .as_ref()
        .map(|f| f.name.clone())
        .or_else(|| note.caption.place.clone());
    let country = note.feature.as_ref().and_then(|f| f.country.clone());
    let line = [name, country]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
    let place_line = if line.is_empty() {
        None
    } else if precise {
        Some(line)
    } else {
        Some(format!("Around {line}"))
    };
    let facts = match &note.feature {
        Some(f) => {
            app.storage
                .get_landmark_facts(&landmark::key(&f.name))
                .await?
        }
        None => None,
    };
    let feature = note.feature.as_ref().map(|f| {
        json!({
            "name": f.name,
            "kind": f.kind,
            "detail": f.detail,
            "summary": facts.as_ref().map(|x| x.summary.clone()),
            "tagline": facts.as_ref().and_then(|x| x.tagline.clone()),
            "credit": facts.as_ref().map(|x| x.credit.clone()),
            "url": facts.as_ref().map(|x| x.url.clone()),
        })
    });
    // The trip's other photos from the same place, nearest first.
    let mut nearby: Vec<(f64, &MediaItem)> = all
        .iter()
        .filter(|m| m.trip.as_ref() == Some(&trip_id) && m.id != media.id && !m.is_removed())
        .filter_map(|m| {
            let p = m.gps.or_else(|| {
                trip.days
                    .iter()
                    .flat_map(|d| &d.route.points)
                    .find(|p| p.media == m.id)
                    .map(|p| p.at)
            })?;
            let d = p.distance_m(at);
            (d <= radius).then_some((d, m))
        })
        .collect();
    nearby.sort_by(|a, b| a.0.total_cmp(&b.0));
    Resp::ok(json!({
        "media": media.id,
        "taken": taken(media),
        "place": place_line,
        "coords": if precise { note.caption.coords.clone() } else { None },
        "feature": feature,
        "description": media.description.as_ref().map(|d| d.text.clone()),
        "nearby": nearby.iter().take(NEARBY).map(|(_, m)| m.id.clone()).collect::<Vec<_>>(),
        "credit": note.credit,
    }))
}

/// Field-note summaries for a trip's JSON: where each sits and how sure the place is.
/// `public` forces the approximate form (member households, code views).
#[must_use]
pub fn summaries(media: &[MediaItem], public: bool) -> Value {
    let map: serde_json::Map<String, Value> = media
        .iter()
        .filter_map(|m| {
            let n = m.field_note.as_ref()?;
            Some((
                m.id.to_string(),
                json!({
                    "placement": n.placement,
                    "mode": n.mode,
                    "certainty": if !public && n.certainty.is_precise() { "precise" } else { "approximate" },
                    "credit": n.credit,
                }),
            ))
        })
        .collect();
    Value::Object(map)
}

/// Force a trip JSON's field notes into the public form.
pub fn make_public(trip: &mut Value) {
    if let Some(notes) = trip.get_mut("field_notes").and_then(Value::as_object_mut) {
        for n in notes.values_mut() {
            n["certainty"] = json!("approximate");
        }
    }
}
