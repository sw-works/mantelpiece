//! Memories that resurface (F-701 to F-703, F-707): `GET /memories/home` (the
//! `memories_for` tool) and `GET /map`, the all-trips pin map.

use memorylane_trips::memories::{self, Period};
use serde_json::json;
use time::{Date, Month};

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

/// `?date=YYYY-MM-DD`, defaulting to today in UTC.
fn date_param(app: &App, req: &Req) -> Result<Date, ApiError> {
    match req.query.get("date") {
        None => Ok(app.now().date()),
        Some(s) => Date::parse(s, &time::format_description::well_known::Iso8601::DATE)
            .map_err(|_| ApiError::bad_request("date must be YYYY-MM-DD")),
    }
}

pub async fn home(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let date = date_param(app, req)?;
    let media = app.storage.list_media(&household).await?;
    let on_this_day = memories::on_this_day(&media, &household, date);
    let then_and_now: Vec<_> = memories::then_and_now(&media, &household)
        .into_iter()
        .map(|p| json!({ "then": p.then, "now": p.now, "lat": p.place.lat, "lon": p.place.lon, "years_apart": p.years_apart }))
        .collect();
    let last_month = date
        .replace_day(1)
        .map_err(|e| ApiError::internal(e.to_string()))?
        - time::Duration::days(1);
    let best_of_month = memories::best_of(
        &media,
        &household,
        Period::Month {
            year: last_month.year(),
            month: last_month.month(),
        },
    );
    let best_of_year = memories::best_of(
        &media,
        &household,
        Period::Year {
            year: date.year() - 1,
        },
    );
    Resp::ok(json!({
        "date": date.to_string(),
        "on_this_day": on_this_day,
        "then_and_now": then_and_now,
        "best_of_month": { "year": last_month.year(), "month": month_name(last_month.month()), "media": best_of_month },
        "best_of_year": { "year": date.year() - 1, "media": best_of_year },
    }))
}

fn month_name(m: Month) -> &'static str {
    match m {
        Month::January => "January",
        Month::February => "February",
        Month::March => "March",
        Month::April => "April",
        Month::May => "May",
        Month::June => "June",
        Month::July => "July",
        Month::August => "August",
        Month::September => "September",
        Month::October => "October",
        Month::November => "November",
        Month::December => "December",
    }
}

/// `GET /map` (F-707): one pin per trip, drawn once per set of trips and presigned.
pub async fn map(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let trips = app.storage.list_trips(&household).await?;
    let pins: Vec<serde_json::Value> = trips
        .iter()
        .filter_map(|t| {
            let p = t.route.points.first()?;
            Some(json!({ "trip": t.id, "name": t.name, "lat": p.at.lat, "lon": p.at.lon, "start": t.start.date().to_string() }))
        })
        .collect();
    if pins.is_empty() {
        return Resp::ok(json!({ "url": null, "pins": pins }));
    }
    let points: Vec<_> = trips
        .iter()
        .filter_map(|t| t.route.points.first().map(|p| p.at))
        .collect();
    // The key names the set of trips, so a new trip means a new map and old ones stay cached.
    let key = format!("maps/{household}/all-{}.png", points.len());
    if app.blobs.get(&key).await.is_err() {
        let png = app.maps.pins_map(&points, 1280, 720).await?;
        app.blobs.put(&key, png, "image/png").await?;
    }
    Resp::ok(json!({ "url": app.blobs.presign_get(&key).await?, "pins": pins }))
}
