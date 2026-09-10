//! End-to-end flows through the router against in-memory backends.

#![allow(clippy::too_many_lines)]

use memorylane_api::app::{App, Config};
use memorylane_api::req::{Jwt, Method, Req, Resp};
use memorylane_api::starter::MemoryStarter;
use memorylane_api::tokens::Signer;
use memorylane_core::ids::{HouseholdId, MediaId, TripId};
use memorylane_core::trip::{Route, Trip};
use memorylane_store::bus::MemoryBus;
use memorylane_store::memory::{MemoryBlobs, MemoryStorage};
use memorylane_store::vectors::MemoryVectors;
use memorylane_store::{Storage, Vectors};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

struct Harness {
    app: App,
    storage: Arc<MemoryStorage>,
    blobs: Arc<MemoryBlobs>,
    bus: Arc<MemoryBus>,
    vectors: Arc<MemoryVectors>,
    starter: Arc<MemoryStarter>,
    now: Arc<Mutex<OffsetDateTime>>,
}

fn harness() -> Harness {
    let storage = Arc::new(MemoryStorage::new());
    let blobs = Arc::new(MemoryBlobs::default());
    let bus = Arc::new(MemoryBus::default());
    let vectors = Arc::new(MemoryVectors::default());
    let starter = Arc::new(MemoryStarter::default());
    let now = Arc::new(Mutex::new(datetime!(2026-09-08 10:00:00 UTC)));
    let clock_now = now.clone();
    let app = App {
        storage: storage.clone(),
        blobs: blobs.clone(),
        vectors: vectors.clone(),
        bus: bus.clone(),
        starter: starter.clone(),
        signer: Signer::new("test-secret"),
        config: Config {
            poll_hold: Duration::ZERO,
            part_size: 1024,
            ..Config::default()
        },
        clock: Arc::new(move || *clock_now.lock().unwrap()),
    };
    let h = Harness {
        app,
        storage,
        blobs,
        bus,
        vectors,
        starter,
        now,
    };
    // The owner exists once they approved a TV; tests that skip that flow still need them.
    futures_block(
        h.storage
            .put_contributor(&memorylane_core::contributor::Contributor::owner(
                HouseholdId::new("amzn-owner"),
                "Hanyu",
                h.app.now(),
            )),
    );
    h
}

fn futures_block<F: std::future::Future<Output = memorylane_store::Result<()>>>(f: F) {
    // MemoryStorage never awaits anything real, so a no-op waker drives it to completion.
    let mut f = std::pin::pin!(f);
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    match f.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(r) => r.unwrap(),
        std::task::Poll::Pending => panic!("memory storage should not pend"),
    }
}

/// Seed an owner-uploaded, enriched photo with tags and an optional description.
async fn enriched(
    h: &Harness,
    filename: &str,
    seed: u8,
    tags: &[&str],
    description: Option<(&str, &str)>,
) -> MediaId {
    let init = h
        .owner(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": filename, "bytes": 10, "sha256": sha(seed) })),
        )
        .await;
    let id = MediaId::new(init.body["media_id"].as_str().unwrap());
    let mut m = h
        .storage
        .get_media(&HouseholdId::new("amzn-owner"), &id)
        .await
        .unwrap()
        .unwrap();
    m.state = memorylane_core::media::ProcessingState::Enriched;
    m.is_keeper = true;
    m.captured = Some(memorylane_core::media::CaptureTime::new(
        datetime!(2026-04-04 05:00:00 UTC),
        None,
    ));
    m.enrichment.scene_tags = tags
        .iter()
        .map(|t| memorylane_core::media::Tag {
            name: (*t).to_owned(),
            confidence: 0.9,
        })
        .collect();
    if let Some((landmark, text)) = description {
        m.description = Some(memorylane_core::media::Description {
            text: text.into(),
            landmark: Some(landmark.into()),
            model_id: "test".into(),
            generated_at: h.app.now(),
        });
    }
    h.storage.put_media(&m).await.unwrap();
    let vector = memorylane_ingest::embed::embed_media(
        &m.enrichment.scene_tags,
        description.map(|d| d.0),
        None,
        description.map(|d| d.1),
    );
    h.vectors
        .upsert(&HouseholdId::new("amzn-owner"), None, &id, &vector)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn search_ranks_by_meaning_and_falls_back_to_keywords() {
    let h = harness();
    let ramen = enriched(&h, "ramen.jpg", 21, &["Ramen", "Restaurant", "Food"], None).await;
    let temple = enriched(
        &h,
        "temple.jpg",
        22,
        &["Temple", "Architecture"],
        Some(("Kinkaku-ji", "The golden pavilion.")),
    )
    .await;
    let unindexed = enriched(&h, "beach.jpg", 23, &["Beach", "Sea"], None).await;
    h.vectors
        .delete(&HouseholdId::new("amzn-owner"), &unindexed)
        .await
        .unwrap();

    let found = h
        .owner(
            Method::Post,
            "/search",
            Some(json!({ "query": "the ramen place" })),
        )
        .await;
    assert_eq!(found.status, 200, "{}", found.body);
    assert_eq!(found.body["results"][0]["media"], ramen.as_str());
    let golden = h
        .owner(
            Method::Post,
            "/search",
            Some(json!({ "query": "golden pavilion" })),
        )
        .await;
    assert_eq!(golden.body["results"][0]["media"], temple.as_str());
    // No vector: the keyword fallback still finds it.
    let beach = h
        .owner(Method::Post, "/search", Some(json!({ "query": "beach" })))
        .await;
    assert_eq!(beach.body["results"][0]["media"], unindexed.as_str());
    let none = h
        .owner(
            Method::Post,
            "/search",
            Some(json!({ "query": "snowmobile" })),
        )
        .await;
    assert!(none.body["results"].as_array().unwrap().is_empty());
    assert_eq!(
        h.owner(Method::Post, "/search", Some(json!({ "query": " " })))
            .await
            .status,
        400
    );
    // Hidden items never come back.
    h.owner(Method::Post, &format!("/media/{ramen}/hide"), None)
        .await;
    let hidden = h
        .owner(Method::Post, "/search", Some(json!({ "query": "ramen" })))
        .await;
    assert!(hidden.body["results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["media"] != ramen.as_str()));
}

#[tokio::test]
async fn ask_describes_the_photo_on_screen_or_searches_or_admits_nothing() {
    let h = harness();
    let statue = enriched(
        &h,
        "statue.jpg",
        31,
        &["Statue", "Temple"],
        Some(("Todai-ji", "The Great Buddha at Todai-ji in Nara.")),
    )
    .await;
    let ramen = enriched(&h, "ramen.jpg", 32, &["Ramen"], None).await;

    // Nothing on screen yet: a search that becomes a playlist.
    let searched = h
        .owner(
            Method::Post,
            "/ask",
            Some(json!({ "question": "the ramen place" })),
        )
        .await;
    assert_eq!(searched.status, 200, "{}", searched.body);
    assert_eq!(searched.body["mode"], "search");
    assert_eq!(searched.body["count"], 1);
    let playlist = searched.body["playlist"].as_str().unwrap().to_owned();
    let got = h
        .owner(Method::Get, &format!("/playlists/{playlist}"), None)
        .await;
    assert_eq!(got.body["media"][0], ramen.as_str());
    let nothing = h
        .owner(
            Method::Post,
            "/ask",
            Some(json!({ "question": "snowmobiles" })),
        )
        .await;
    assert_eq!(nothing.body["mode"], "nothing");
    assert_eq!(
        h.owner(Method::Get, "/playlists", None)
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // The TV reports the statue on screen, paused.
    let put = h
        .owner(Method::Put, "/presence", Some(json!({ "device_id": "tv-1", "media": statue, "trip": null, "day": 2, "paused": true })))
        .await;
    assert_eq!(put.status, 204, "{}", put.body);
    let listed = h.owner(Method::Get, "/presence", None).await;
    assert_eq!(listed.body[0]["device_id"], "tv-1");
    let asked = h
        .owner(
            Method::Post,
            "/ask",
            Some(json!({ "question": "what's that statue" })),
        )
        .await;
    assert_eq!(asked.body["mode"], "describe", "{}", asked.body);
    assert_eq!(asked.body["tv"], "tv-1");
    assert_eq!(asked.body["card"]["landmark"], "Todai-ji");
    assert_eq!(
        asked.body["card"]["answer"],
        "The Great Buddha at Todai-ji in Nara."
    );
    assert_eq!(asked.body["card"]["taken_by"], "Hanyu");
    assert_eq!(asked.body["card"]["when"], "2026-04-04");
    let feed = h.owner(Method::Get, "/events?since=0", None).await;
    let card = feed.body["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "answer.card")
        .unwrap();
    assert_eq!(card["media"], statue.as_str());

    // Two TVs: the named one wins; a stale one counts as nothing showing.
    h.owner(
        Method::Put,
        "/presence",
        Some(json!({ "device_id": "tv-2", "media": ramen, "paused": false })),
    )
    .await;
    let named = h
        .owner(
            Method::Post,
            "/ask",
            Some(json!({ "question": "what is this", "tv": "tv-1" })),
        )
        .await;
    assert_eq!(named.body["card"]["media"], statue.as_str());
    let freshest = h
        .owner(
            Method::Post,
            "/ask",
            Some(json!({ "question": "what is this" })),
        )
        .await;
    assert_eq!(freshest.body["card"]["media"], ramen.as_str());
    assert!(freshest.body["card"]["answer"]
        .as_str()
        .unwrap()
        .starts_with("Taken by Hanyu on 2026-04-04"));
    h.advance(Duration::minutes(11));
    let stale = h
        .owner(Method::Post, "/ask", Some(json!({ "question": "ramen" })))
        .await;
    assert_eq!(stale.body["mode"], "search");
    assert!(h
        .owner(Method::Get, "/presence", None)
        .await
        .body
        .as_array()
        .unwrap()
        .is_empty());

    // Playlists are created explicitly too (the create_playlist tool).
    let created = h
        .owner(
            Method::Post,
            "/playlists",
            Some(json!({ "name": "Best of Nara", "media": [statue], "source": "trip_curator" })),
        )
        .await;
    assert_eq!(created.status, 201, "{}", created.body);
    assert_eq!(
        h.owner(
            Method::Post,
            "/playlists",
            Some(json!({ "name": "x", "media": [], "source": "s" }))
        )
        .await
        .status,
        400
    );
}

#[tokio::test]
async fn narratives_are_checked_stored_per_day_and_regenerated_through_the_bus() {
    let h = harness();
    let mut kyoto = trip("amzn-owner", "kyoto");
    kyoto.contributors = vec![memorylane_core::ids::ContributorId::new("amzn-owner")];
    kyoto.days = (1..=2)
        .map(|i| memorylane_core::trip::TripDay {
            index: i,
            date: time::macros::date!(2026 - 04 - 03) + Duration::days(i64::from(i) - 1),
            media: vec![],
            route: Route::default(),
            title: Some(if i == 1 {
                "Day 1: Kinkaku-ji".into()
            } else {
                "Day 2".into()
            }),
            story: None,
            undated: vec![],
        })
        .collect();
    h.storage.put_trip(&kyoto).await.unwrap();
    h.storage
        .put_contributor(&memorylane_core::contributor::Contributor::owner(
            HouseholdId::new("amzn-owner"),
            "Hanyu",
            h.app.now(),
        ))
        .await
        .unwrap();

    let bad = h
        .owner(
            Method::Post,
            "/trips/kyoto/narrative",
            Some(json!({ "text": "Grandma flew in from Osaka in May.", "model_id": "nova" })),
        )
        .await;
    assert_eq!(bad.status, 422, "{}", bad.body);
    assert_eq!(bad.body["check"], "withheld");
    let stored = h
        .storage
        .get_trip(&HouseholdId::new("amzn-owner"), &TripId::new("kyoto"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.narrative.as_ref().map(|n| n.shown()), Some(None));
    let shown = h.owner(Method::Get, "/trips/kyoto", None).await;
    assert_eq!(shown.body["narrative"]["check"], "withheld");

    let good = h
        .owner(
            Method::Post,
            "/trips/kyoto/narrative",
            Some(json!({
                "text": "Hanyu started at Kinkaku-ji in April. The second day was quiet.",
                "model_id": "nova",
                "stories": ["Hanyu started at Kinkaku-ji in April.", "The second day was quiet."]
            })),
        )
        .await;
    assert_eq!(good.status, 200, "{}", good.body);
    let stored = h
        .storage
        .get_trip(&HouseholdId::new("amzn-owner"), &TripId::new("kyoto"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.days[1].story.as_deref(),
        Some("The second day was quiet.")
    );
    assert!(stored.narrative.as_ref().unwrap().shown().is_some());

    let (maya, _) = h.guest("Maya").await;
    assert_eq!(
        h.bearer(
            Method::Post,
            "/trips/kyoto/narrative/regenerate",
            None,
            &maya
        )
        .await
        .status,
        403
    );
    let again = h
        .owner(Method::Post, "/trips/kyoto/narrative/regenerate", None)
        .await;
    assert_eq!(again.status, 200, "{}", again.body);
    let stored = h
        .storage
        .get_trip(&HouseholdId::new("amzn-owner"), &TripId::new("kyoto"))
        .await
        .unwrap()
        .unwrap();
    assert!(stored.narrative.is_none());
    {
        let published = h.bus.published.lock().unwrap();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].detail_type(), "trip.detected");
    }
    assert_eq!(
        h.owner(Method::Get, "/trips/kyoto/map", None).await.status,
        404,
        "no map until organize drew one"
    );
}

#[tokio::test]
async fn keeper_moves_within_a_burst_and_non_keepers_inherit() {
    let h = harness();
    let first = h
        .owner(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": "a.jpg", "bytes": 10, "sha256": sha(1) })),
        )
        .await;
    let second = h
        .owner(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": "b.jpg", "bytes": 10, "sha256": sha(2) })),
        )
        .await;
    let (a, b) = (
        MediaId::new(first.body["media_id"].as_str().unwrap()),
        MediaId::new(second.body["media_id"].as_str().unwrap()),
    );
    for (id, keeper) in [(&a, true), (&b, false)] {
        let mut m = h
            .storage
            .get_media(&HouseholdId::new("amzn-owner"), id)
            .await
            .unwrap()
            .unwrap();
        m.burst_group = Some("g1".into());
        m.is_keeper = keeper;
        if !keeper {
            m.state = memorylane_core::media::ProcessingState::Enriched;
            m.enrichment.scene_tags = vec![memorylane_core::media::Tag {
                name: "Temple".into(),
                confidence: 0.9,
            }];
        }
        h.storage.put_media(&m).await.unwrap();
    }
    let moved = h
        .owner(Method::Post, &format!("/media/{b}/keeper"), None)
        .await;
    assert_eq!(moved.status, 200, "{}", moved.body);
    assert_eq!(moved.body["changed"], true);
    assert_eq!(moved.body["previous"], a.as_str());
    let a_now = h
        .storage
        .get_media(&HouseholdId::new("amzn-owner"), &a)
        .await
        .unwrap()
        .unwrap();
    let b_now = h
        .storage
        .get_media(&HouseholdId::new("amzn-owner"), &b)
        .await
        .unwrap()
        .unwrap();
    assert!(b_now.is_keeper && !a_now.is_keeper);
    assert_eq!(a_now.enrichment.inherited_from, Some(b.clone()));
    assert_eq!(a_now.enrichment.scene_tags.len(), 1);
    let same = h
        .owner(Method::Post, &format!("/media/{b}/keeper"), None)
        .await;
    assert_eq!(same.body["changed"], false);
    let (maya, _) = h.guest("Maya").await;
    assert_eq!(
        h.bearer(Method::Post, &format!("/media/{a}/keeper"), None, &maya)
            .await
            .status,
        403
    );
}

impl Harness {
    fn advance(&self, d: Duration) {
        *self.now.lock().unwrap() += d;
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        bearer: Option<&str>,
        jwt: Option<Jwt>,
    ) -> Resp {
        let (path, query) = match path.split_once('?') {
            Some((p, q)) => (
                p.to_owned(),
                q.split('&')
                    .filter_map(|kv| kv.split_once('='))
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect(),
            ),
            None => (path.to_owned(), HashMap::new()),
        };
        self.app
            .handle(Req {
                method: Some(method),
                path,
                query,
                bearer: bearer.map(str::to_owned),
                body: body.map(|b| b.to_string().into_bytes()).unwrap_or_default(),
                jwt,
                device: None,
            })
            .await
    }

    /// A bearer call carrying an `x-device-id` header.
    async fn with_device(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: &str,
        device: &str,
    ) -> Resp {
        self.app
            .handle(Req {
                method: Some(method),
                path: path.to_owned(),
                query: HashMap::new(),
                bearer: Some(token.to_owned()),
                body: body.map(|b| b.to_string().into_bytes()).unwrap_or_default(),
                jwt: None,
                device: Some(device.to_owned()),
            })
            .await
    }

    async fn owner(&self, method: Method, path: &str, body: Option<Value>) -> Resp {
        self.call(method, path, body, None, Some(owner_jwt())).await
    }

    async fn bearer(&self, method: Method, path: &str, body: Option<Value>, token: &str) -> Resp {
        self.call(method, path, body, Some(token), None).await
    }

    /// Owner creates a household code and a guest joins it. Returns the guest token.
    async fn guest(&self, name: &str) -> (String, String) {
        let code = self.owner(Method::Post, "/household/code", None).await;
        assert_eq!(code.status, 201, "{}", code.body);
        let code = code.body["code"].as_str().unwrap().to_owned();
        let joined = self
            .call(
                Method::Post,
                "/join",
                Some(json!({ "code": code, "display_name": name })),
                None,
                None,
            )
            .await;
        assert_eq!(joined.status, 200, "{}", joined.body);
        (joined.body["token"].as_str().unwrap().to_owned(), code)
    }

    async fn upload(&self, token: &str, filename: &str, bytes: u64, sha: &str) -> Resp {
        self.bearer(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": filename, "bytes": bytes, "sha256": sha, "content_type": "image/jpeg" })),
            token,
        )
        .await
    }
}

fn owner_jwt() -> Jwt {
    Jwt {
        sub: "amzn-owner".into(),
        name: Some("Hanyu".into()),
    }
}

fn sha(seed: u8) -> String {
    format!("{seed:02x}").repeat(32)
}

fn trip(household: &str, id: &str) -> Trip {
    Trip {
        id: TripId::new(id),
        household: HouseholdId::new(household),
        name: "Kyoto, April 2026".into(),
        start: datetime!(2026-04-03 00:00:00 UTC),
        end: datetime!(2026-04-09 00:00:00 UTC),
        days: vec![],
        route: Route::default(),
        highlights: vec![],
        contributors: vec![],
        clock_offsets: vec![],
        narrative: None,
        hero: None,
        members: vec![],
    }
}

#[tokio::test]
async fn device_code_sign_in_flow() {
    let h = harness();
    let issued = h
        .call(
            Method::Post,
            "/device/code",
            Some(json!({ "device_id": "tv-1", "tv_name": "Living room" })),
            None,
            None,
        )
        .await;
    assert_eq!(issued.status, 201, "{}", issued.body);
    let code = issued.body["code"].as_str().unwrap().to_owned();
    assert_eq!(code.len(), 8);
    assert!(issued.body["link"]
        .as_str()
        .unwrap()
        .ends_with(&format!("?c={code}")));

    let poll_path = format!("/device/poll?code={code}&device_id=tv-1");
    let pending = h.call(Method::Get, &poll_path, None, None, None).await;
    assert_eq!(pending.body["state"], "pending");

    // Approval needs a signed-in phone.
    let anon = h
        .call(
            Method::Post,
            "/device/approve",
            Some(json!({ "code": code })),
            None,
            None,
        )
        .await;
    assert_eq!(anon.status, 401);
    let approved = h
        .owner(
            Method::Post,
            "/device/approve",
            Some(json!({ "code": code.to_lowercase() })),
        )
        .await;
    assert_eq!(approved.status, 200, "{}", approved.body);
    assert_eq!(approved.body["household"], "amzn-owner");
    let phone = approved.body["token"].as_str().unwrap().to_owned();
    let me = h.bearer(Method::Get, "/me", None, &phone).await;
    assert_eq!(me.body["role"], "owner");

    // A second approval of the same code: the TV is already signed in.
    let twice = h
        .owner(
            Method::Post,
            "/device/approve",
            Some(json!({ "code": code })),
        )
        .await;
    assert_eq!(twice.status, 409);
    assert_eq!(twice.body["error"], "already signed in");

    let done = h.call(Method::Get, &poll_path, None, None, None).await;
    assert_eq!(done.body["state"], "approved");
    let token = done.body["token"].as_str().unwrap().to_owned();

    // The TV's name became the household name, visible to every owner session.
    let me = h.bearer(Method::Get, "/me", None, &token).await;
    assert_eq!(me.body["role"], "owner");
    assert_eq!(me.body["household"], "amzn-owner");
    assert_eq!(me.body["household_name"], "Living room");
    assert_eq!(me.body["people_enabled"], false);

    // Handed over once only.
    let again = h.call(Method::Get, &poll_path, None, None, None).await;
    assert_eq!(again.status, 403);
    // Other device cannot poll.
    let other = h
        .call(
            Method::Get,
            &format!("/device/poll?code={code}&device_id=tv-2"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(other.status, 403);

    // Owner contributor was created on first approval.
    let contributors = h.bearer(Method::Get, "/contributors", None, &token).await;
    assert_eq!(contributors.body.as_array().unwrap().len(), 1);
    assert_eq!(contributors.body[0]["display_name"], "Hanyu");
}

#[tokio::test]
async fn expired_device_code() {
    let h = harness();
    let issued = h
        .call(
            Method::Post,
            "/device/code",
            Some(json!({ "device_id": "tv-1" })),
            None,
            None,
        )
        .await;
    let code = issued.body["code"].as_str().unwrap().to_owned();
    h.advance(Duration::minutes(11));
    let polled = h
        .call(
            Method::Get,
            &format!("/device/poll?code={code}&device_id=tv-1"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(polled.body["state"], "expired");
    let approve = h
        .owner(
            Method::Post,
            "/device/approve",
            Some(json!({ "code": code })),
        )
        .await;
    assert_eq!(approve.status, 409);
}

#[tokio::test]
async fn guest_joins_by_code_and_code_lifecycle() {
    let h = harness();
    let (token, code) = h.guest("Maya").await;
    let me = h.bearer(Method::Get, "/me", None, &token).await;
    assert_eq!(me.body["role"], "contributor");
    assert_eq!(me.body["display_name"], "Maya");
    assert!(me.body["trip"].is_null());

    let nameless = h
        .call(
            Method::Post,
            "/join",
            Some(json!({ "code": code })),
            None,
            None,
        )
        .await;
    assert_eq!(nameless.status, 400);
    let unknown = h
        .call(
            Method::Post,
            "/join",
            Some(json!({ "code": "ZZZZZZ", "display_name": "X" })),
            None,
            None,
        )
        .await;
    assert_eq!(unknown.status, 404);

    // A friend with their own Amazon account joins under that identity.
    let friend = Jwt {
        sub: "amzn-friend".into(),
        name: Some("Ken".into()),
    };
    let joined = h
        .call(
            Method::Post,
            "/join/amazon",
            Some(json!({ "code": code })),
            None,
            Some(friend.clone()),
        )
        .await;
    assert_eq!(joined.status, 200, "{}", joined.body);
    assert_eq!(joined.body["contributor"], "lwa:amzn-friend");
    let rejoined = h
        .call(
            Method::Post,
            "/join",
            Some(json!({ "code": code })),
            None,
            Some(friend),
        )
        .await;
    assert_eq!(rejoined.body["contributor"], "lwa:amzn-friend");

    // Guests cannot manage codes.
    let forbidden = h.bearer(Method::Get, "/codes", None, &token).await;
    assert_eq!(forbidden.status, 403);

    // Rotate: old stops, new works.
    let rotated = h
        .owner(Method::Post, &format!("/codes/{code}/rotate"), None)
        .await;
    assert_eq!(rotated.status, 201);
    let new_code = rotated.body["code"].as_str().unwrap().to_owned();
    assert_ne!(new_code, code);
    let gone = h
        .call(
            Method::Post,
            "/join",
            Some(json!({ "code": code, "display_name": "Late" })),
            None,
            None,
        )
        .await;
    assert_eq!(gone.status, 410);
    let ok = h
        .call(
            Method::Post,
            "/join",
            Some(json!({ "code": new_code, "display_name": "Late" })),
            None,
            None,
        )
        .await;
    assert_eq!(ok.status, 200);

    // Existing session keeps working for reads but not for new uploads once its code is dead.
    let me = h.bearer(Method::Get, "/me", None, &token).await;
    assert_eq!(me.status, 200);
    let blocked = h.upload(&token, "a.jpg", 10, &sha(1)).await;
    assert_eq!(blocked.status, 410);

    let list = h.owner(Method::Get, "/codes", None).await;
    assert_eq!(list.body.as_array().unwrap().len(), 2);
    let revoked = h
        .owner(Method::Delete, &format!("/codes/{new_code}"), None)
        .await;
    assert_eq!(revoked.status, 204);
    let list = h.owner(Method::Get, "/codes", None).await;
    assert!(list
        .body
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["accepts_uploads"] == false));
}

#[tokio::test]
async fn trip_scoped_code_limits_visibility() {
    let h = harness();
    h.storage
        .put_trip(&trip("amzn-owner", "kyoto"))
        .await
        .unwrap();
    h.storage
        .put_trip(&trip("amzn-owner", "lisbon"))
        .await
        .unwrap();
    let missing = h
        .owner(
            Method::Post,
            "/trips/nope/code",
            Some(json!({ "expires_in_days": 7 })),
        )
        .await;
    assert_eq!(missing.status, 404);
    let code = h
        .owner(
            Method::Post,
            "/trips/kyoto/code",
            Some(json!({ "expires_in_days": 7 })),
        )
        .await;
    assert_eq!(code.status, 201, "{}", code.body);
    assert_eq!(code.body["scope"], "trip");
    assert!(code.body["link"].as_str().unwrap().contains("?t="));
    let code = code.body["code"].as_str().unwrap().to_owned();

    let joined = h
        .call(
            Method::Post,
            "/join",
            Some(json!({ "code": code, "display_name": "Maya" })),
            None,
            None,
        )
        .await;
    let token = joined.body["token"].as_str().unwrap().to_owned();
    assert_eq!(joined.body["trip"], "kyoto");

    let trips = h.bearer(Method::Get, "/trips", None, &token).await;
    assert_eq!(trips.body.as_array().unwrap().len(), 1);
    assert_eq!(trips.body[0]["id"], "kyoto");
    assert_eq!(
        h.bearer(Method::Get, "/trips/lisbon", None, &token)
            .await
            .status,
        403
    );
    assert_eq!(
        h.bearer(Method::Get, "/trips/kyoto", None, &token)
            .await
            .status,
        200
    );
    assert_eq!(
        h.owner(Method::Get, "/trips", None)
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // The code expired: the guest token expired with it (F-153).
    h.advance(Duration::days(8));
    let late = h.upload(&token, "a.jpg", 10, &sha(1)).await;
    assert_eq!(late.status, 401);
}

#[tokio::test]
async fn guest_sessions_are_bound_to_their_device() {
    let h = harness();
    let code = h.owner(Method::Post, "/household/code", None).await;
    let code = code.body["code"].as_str().unwrap().to_owned();
    let joined = h
        .call(
            Method::Post,
            "/join",
            Some(json!({ "code": code, "display_name": "Maya", "device_id": "phone-a" })),
            None,
            None,
        )
        .await;
    let token = joined.body["token"].as_str().unwrap().to_owned();
    assert_eq!(
        h.with_device(Method::Get, "/me", None, &token, "phone-a")
            .await
            .status,
        200
    );
    assert_eq!(
        h.with_device(Method::Get, "/me", None, &token, "phone-b")
            .await
            .status,
        401
    );
    assert_eq!(h.bearer(Method::Get, "/me", None, &token).await.status, 401);
}

#[tokio::test]
async fn codes_report_live_counts_and_the_read_only_view_opens_by_code() {
    let h = harness();
    let mut kyoto = trip("amzn-owner", "kyoto");
    kyoto.contributors = vec![memorylane_core::ids::ContributorId::new("amzn-owner")];
    h.storage.put_trip(&kyoto).await.unwrap();
    h.storage
        .put_contributor(&memorylane_core::contributor::Contributor::owner(
            HouseholdId::new("amzn-owner"),
            "Hanyu",
            h.app.now(),
        ))
        .await
        .unwrap();
    let code = h
        .owner(
            Method::Post,
            "/trips/kyoto/code",
            Some(json!({ "expires_in_days": 7 })),
        )
        .await;
    assert_eq!(code.body["contributors"], 1);
    assert_eq!(code.body["member_households"], 0);
    let code = code.body["code"].as_str().unwrap().to_owned();

    // No session: the trip opens by code with credits and the household name.
    let view = h
        .call(Method::Get, &format!("/view?code={code}"), None, None, None)
        .await;
    assert_eq!(view.status, 200, "{}", view.body);
    assert_eq!(view.body["trip"]["id"], "kyoto");
    assert_eq!(
        view.body["trip"]["contributor_names"]["amzn-owner"],
        "Hanyu"
    );
    assert!(view.body["trip"]["media_credits"].is_object());
    assert_eq!(
        h.call(Method::Get, "/view?code=ZZZZZZ", None, None, None)
            .await
            .status,
        404
    );
    let household_code = h.owner(Method::Post, "/household/code", None).await;
    assert_eq!(
        h.call(
            Method::Get,
            &format!(
                "/view?code={}",
                household_code.body["code"].as_str().unwrap()
            ),
            None,
            None,
            None
        )
        .await
        .status,
        403
    );
    // A rotated code closes the view.
    h.owner(Method::Post, &format!("/codes/{code}/rotate"), None)
        .await;
    assert_eq!(
        h.call(Method::Get, &format!("/view?code={code}"), None, None, None)
            .await
            .status,
        410
    );
}

#[tokio::test]
async fn removing_media_cascades_out_of_trips_and_playlists_but_keeps_the_original() {
    let h = harness();
    let (token, _) = h.guest("Maya").await;
    let first = h.upload(&token, "a.jpg", 10, &sha(1)).await;
    let second = h.upload(&token, "b.jpg", 10, &sha(2)).await;
    let (gone, kept) = (
        MediaId::new(first.body["media_id"].as_str().unwrap()),
        MediaId::new(second.body["media_id"].as_str().unwrap()),
    );
    let mut kyoto = trip("amzn-owner", "kyoto");
    kyoto.days = vec![memorylane_core::trip::TripDay {
        index: 1,
        date: time::macros::date!(2026 - 04 - 03),
        media: vec![gone.clone(), kept.clone()],
        route: Route::default(),
        title: None,
        story: None,
        undated: vec![],
    }];
    kyoto.highlights = vec![gone.clone(), kept.clone()];
    kyoto.hero = Some(gone.clone());
    h.storage.put_trip(&kyoto).await.unwrap();
    for id in [&gone, &kept] {
        let mut m = h
            .storage
            .get_media(&HouseholdId::new("amzn-owner"), id)
            .await
            .unwrap()
            .unwrap();
        m.trip = Some(TripId::new("kyoto"));
        h.storage.put_media(&m).await.unwrap();
    }
    h.storage
        .put_playlist(&memorylane_core::trip::Playlist {
            id: memorylane_core::ids::PlaylistId::new("p1"),
            household: HouseholdId::new("amzn-owner"),
            name: "Best".into(),
            source: "ask".into(),
            media: vec![gone.clone(), kept.clone()],
            created_at: h.app.now(),
        })
        .await
        .unwrap();

    let removed = h
        .bearer(Method::Delete, &format!("/media/{gone}"), None, &token)
        .await;
    assert_eq!(removed.status, 204, "{}", removed.body);
    let t = h
        .storage
        .get_trip(&HouseholdId::new("amzn-owner"), &TripId::new("kyoto"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(t.days[0].media, vec![kept.clone()]);
    assert_eq!(t.highlights, vec![kept.clone()]);
    assert_eq!(t.hero, Some(kept.clone()));
    let p = h
        .storage
        .get_playlist(
            &HouseholdId::new("amzn-owner"),
            &memorylane_core::ids::PlaylistId::new("p1"),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.media, vec![kept.clone()]);
    let stored = h
        .storage
        .get_media(&HouseholdId::new("amzn-owner"), &gone)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.is_removed());
    assert!(stored.restore_until.is_some());
    // The original stays for the restore window; nothing was deleted from the bucket.
    assert!(!h
        .blobs
        .log
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.starts_with("delete ")));
    assert_eq!(
        h.bearer(Method::Get, &format!("/media/{gone}/url"), None, &token)
            .await
            .status,
        404
    );
    assert_eq!(
        h.bearer(Method::Delete, &format!("/media/{gone}"), None, &token)
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn upload_flow_with_progress_events_and_dedup() {
    let h = harness();
    let (token, _) = h.guest("Maya").await;

    let init = h.upload(&token, "IMG_0001.HEIC", 2500, &sha(7)).await;
    assert_eq!(init.status, 201, "{}", init.body);
    assert_eq!(init.body["part_urls"].as_array().unwrap().len(), 3);
    assert_eq!(init.body["part_size"], 1024);
    let key = init.body["key"].as_str().unwrap();
    assert!(key.starts_with("originals/amzn-owner/"), "{key}");
    assert_eq!(
        std::path::Path::new(key)
            .extension()
            .and_then(|e| e.to_str()),
        Some("heic")
    );
    let media_id = init.body["media_id"].as_str().unwrap().to_owned();
    let upload_id = init.body["upload_id"].as_str().unwrap().to_owned();

    assert_eq!(
        h.upload(&token, "copy.heic", 2500, &sha(7)).await.status,
        409
    );
    assert_eq!(
        h.upload(&token, "big.mov", 3 * 1024 * 1024 * 1024, &sha(8))
            .await
            .status,
        400
    );
    assert_eq!(h.upload(&token, "bad.jpg", 10, "nothex").await.status, 400);

    let complete = h
        .bearer(
            Method::Post,
            "/uploads/complete",
            Some(json!({
                "media_id": media_id, "upload_id": upload_id,
                "parts": [{"part_number": 1, "etag": "a"}, {"part_number": 2, "etag": "b"}, {"part_number": 3, "etag": "c"}],
                "batch_done": 1, "batch_total": 42
            })),
            &token,
        )
        .await;
    assert_eq!(complete.status, 200, "{}", complete.body);
    assert!(h
        .blobs
        .log
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.starts_with("complete originals/")));
    assert!(
        h.starter.started.lock().unwrap().is_empty(),
        "batch not finished yet"
    );

    // TV sees the progress event; the guest may not read the household feed.
    let events = h.owner(Method::Get, "/events?since=0", None).await;
    assert_eq!(events.status, 200);
    assert_eq!(events.body["events"][0]["type"], "upload.progress");
    assert_eq!(events.body["events"][0]["total"], 42);
    assert_eq!(events.body["next"], 1);
    let none = h.owner(Method::Get, "/events?since=1", None).await;
    assert!(none.body["events"].as_array().unwrap().is_empty());
    assert_eq!(
        h.bearer(Method::Get, "/events", None, &token).await.status,
        403
    );

    // Maya sees her own uploads with their state; Ken sees none of hers.
    let mine = h.bearer(Method::Get, "/uploads/mine", None, &token).await;
    assert_eq!(mine.status, 200, "{}", mine.body);
    assert_eq!(mine.body.as_array().unwrap().len(), 1);
    assert_eq!(mine.body[0]["media_id"], media_id);
    assert_eq!(mine.body[0]["filename"], "IMG_0001.HEIC");
    assert_eq!(mine.body[0]["state"], "uploaded");

    // Another guest cannot touch Maya's upload; Maya can remove her own.
    let (other, _) = h.guest("Ken").await;
    assert!(h
        .bearer(Method::Get, "/uploads/mine", None, &other)
        .await
        .body
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        h.bearer(Method::Delete, &format!("/media/{media_id}"), None, &other)
            .await
            .status,
        403
    );
    let url = h
        .bearer(
            Method::Get,
            &format!("/media/{media_id}/url?rendition=thumb"),
            None,
            &other,
        )
        .await;
    assert!(url.body["url"].as_str().unwrap().ends_with("/thumb.jpg"));
    assert_eq!(
        h.bearer(
            Method::Get,
            &format!("/media/{media_id}/url?rendition=original"),
            None,
            &other
        )
        .await
        .status,
        403
    );
    assert_eq!(
        h.bearer(Method::Delete, &format!("/media/{media_id}"), None, &token)
            .await
            .status,
        204
    );
    let events = h.owner(Method::Get, "/events?since=1", None).await;
    assert_eq!(events.body["events"][0]["type"], "media.removed");
    assert_eq!(
        h.bearer(Method::Get, &format!("/media/{media_id}/url"), None, &token)
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn owner_pin_hide_and_abort() {
    let h = harness();
    let init = h
        .owner(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": "a.jpg", "bytes": 10, "sha256": sha(1) })),
        )
        .await;
    assert_eq!(init.status, 201, "{}", init.body);
    let id = init.body["media_id"].as_str().unwrap().to_owned();
    let pinned = h
        .owner(Method::Post, &format!("/media/{id}/pin"), None)
        .await;
    assert_eq!(pinned.body["pinned"], true);
    let hidden = h
        .owner(Method::Post, &format!("/media/{id}/hide"), None)
        .await;
    assert_eq!(
        (
            hidden.body["pinned"].as_bool(),
            hidden.body["hidden"].as_bool()
        ),
        (Some(false), Some(true))
    );
    let unhidden = h
        .owner(
            Method::Post,
            &format!("/media/{id}/hide"),
            Some(json!({ "value": false })),
        )
        .await;
    assert_eq!(unhidden.body["hidden"], false);

    let upload_id = init.body["upload_id"].as_str().unwrap();
    let aborted = h
        .owner(
            Method::Post,
            "/uploads/abort",
            Some(json!({ "media_id": id, "upload_id": upload_id })),
        )
        .await;
    assert_eq!(aborted.status, 204);
    assert_eq!(
        h.owner(Method::Get, &format!("/media/{id}/url"), None)
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn removing_a_contributor_needs_a_tv_confirmation() {
    let h = harness();
    let (token, _) = h.guest("Maya").await;
    let me = h.bearer(Method::Get, "/me", None, &token).await;
    let maya = me.body["contributor"].as_str().unwrap().to_owned();

    let unconfirmed = h
        .owner(Method::Delete, &format!("/contributors/{maya}"), None)
        .await;
    assert_eq!(unconfirmed.status, 403);

    let minted = h
        .owner(
            Method::Post,
            "/confirmations",
            Some(json!({ "action": "remove_contributor", "contributor_id": maya })),
        )
        .await;
    assert_eq!(minted.status, 201, "{}", minted.body);
    assert_eq!(minted.body["tool"], "remove_contributor");
    let confirmation = minted.body["token"].as_str().unwrap().to_owned();

    // Wrong action for this token.
    let wrong = h
        .owner(
            Method::Delete,
            &format!("/contributors/someone-else?confirmation={confirmation}"),
            None,
        )
        .await;
    assert_eq!(wrong.status, 403);

    let removed = h
        .owner(
            Method::Delete,
            &format!("/contributors/{maya}?confirmation={confirmation}"),
            None,
        )
        .await;
    assert_eq!(removed.status, 200, "{}", removed.body);
    assert_eq!(removed.body["removed"], maya);
    assert!(removed.body["grace_until"].is_string());

    // During the grace period Maya can export her originals; the owner too; Ken cannot.
    let export = h
        .bearer(
            Method::Get,
            &format!("/contributors/{maya}/export"),
            None,
            &token,
        )
        .await;
    assert_eq!(export.status, 200, "{}", export.body);
    assert!(export.body["files"].is_array());
    assert_eq!(
        h.owner(Method::Get, &format!("/contributors/{maya}/export"), None)
            .await
            .status,
        200
    );
    let (ken, _) = h.guest("Ken").await;
    assert_eq!(
        h.bearer(
            Method::Get,
            &format!("/contributors/{maya}/export"),
            None,
            &ken
        )
        .await
        .status,
        403
    );
    h.advance(Duration::days(31));
    assert_eq!(
        h.owner(Method::Get, &format!("/contributors/{maya}/export"), None)
            .await
            .status,
        410
    );
    h.advance(-Duration::days(31));

    // Token is single use; the removed guest can no longer upload.
    let reused = h
        .owner(
            Method::Delete,
            &format!("/contributors/{maya}?confirmation={confirmation}"),
            None,
        )
        .await;
    assert_eq!(reused.status, 403);
    assert_eq!(h.upload(&token, "a.jpg", 10, &sha(3)).await.status, 403);
    let owner_self = h
        .owner(Method::Delete, "/contributors/amzn-owner", None)
        .await;
    assert_eq!(owner_self.status, 400);
}

#[tokio::test]
async fn auth_and_routing_edges() {
    let h = harness();
    assert_eq!(
        h.call(Method::Get, "/trips", None, None, None).await.status,
        401
    );
    assert_eq!(
        h.call(Method::Get, "/nope", None, None, None).await.status,
        404
    );
    assert_eq!(
        h.bearer(Method::Get, "/me", None, "garbage").await.status,
        401
    );
    let expired = h.app.signer.sign(&memorylane_api::tokens::Claims {
        session: memorylane_api::tokens::Session::Tv {
            household: HouseholdId::new("h"),
            subject: "s".into(),
        },
        exp: h.app.now() - Duration::seconds(1),
    });
    assert_eq!(
        h.bearer(Method::Get, "/me", None, &expired).await.status,
        401
    );
    let no_body = h.call(Method::Post, "/device/code", None, None, None).await;
    assert_eq!(no_body.status, 400);
}

#[tokio::test]
async fn finishing_a_batch_starts_organize() {
    let h = harness();
    let (token, _) = h.guest("Maya").await;
    let init = h.upload(&token, "a.jpg", 10, &sha(9)).await;
    let body = json!({
        "media_id": init.body["media_id"], "upload_id": init.body["upload_id"],
        "parts": [{"part_number": 1, "etag": "a"}], "batch_done": 3, "batch_total": 3
    });
    let done = h
        .bearer(Method::Post, "/uploads/complete", Some(body), &token)
        .await;
    assert_eq!(done.status, 200, "{}", done.body);
    assert_eq!(
        h.starter.started.lock().unwrap().as_slice(),
        &[HouseholdId::new("amzn-owner")]
    );
}

/// Roles and memberships (data-model access rules). The routes that use these land with
/// their stories; the guards themselves are exercised here.
mod roles {
    use super::*;
    use memorylane_api::app::TripAccess;
    use memorylane_core::contributor::Role;
    use memorylane_core::ids::ContributorId;
    use memorylane_core::membership::Membership;

    impl Harness {
        /// A friend with an Amazon account joins by household code and is promoted to
        /// co-editor. Returns their upload token and contributor id.
        async fn co_editor(
            &self,
            sub: &str,
            name: &str,
            trip: Option<&str>,
        ) -> (String, ContributorId) {
            let code = match trip {
                Some(t) => {
                    self.owner(
                        Method::Post,
                        &format!("/trips/{t}/code"),
                        Some(json!({ "expires_in_days": 7 })),
                    )
                    .await
                }
                None => self.owner(Method::Post, "/household/code", None).await,
            };
            let code = code.body["code"].as_str().unwrap().to_owned();
            let joined = self
                .call(
                    Method::Post,
                    "/join/amazon",
                    Some(json!({ "code": code })),
                    None,
                    Some(Jwt {
                        sub: sub.into(),
                        name: Some(name.into()),
                    }),
                )
                .await;
            assert_eq!(joined.status, 200, "{}", joined.body);
            let id = ContributorId::new(joined.body["contributor"].as_str().unwrap());
            let mut c = self
                .storage
                .get_contributor(&HouseholdId::new("amzn-owner"), &id)
                .await
                .unwrap()
                .unwrap();
            c.set_role(Role::CoEditor).unwrap();
            self.storage.put_contributor(&c).await.unwrap();
            (joined.body["token"].as_str().unwrap().to_owned(), id)
        }

        /// Another household that joined `trip` from its own TV. Returns its phone token.
        async fn member_household(&self, household: &str, name: &str, trip: &str) -> String {
            let m = Membership::join(
                HouseholdId::new(household),
                HouseholdId::new("amzn-owner"),
                TripId::new(trip),
                name,
                self.app.now(),
            )
            .unwrap();
            self.storage.put_membership(&m).await.unwrap();
            self.app.signer.sign(&memorylane_api::tokens::Claims {
                session: memorylane_api::tokens::Session::Phone {
                    household: HouseholdId::new(household),
                    subject: household.into(),
                },
                exp: self.app.now() + Duration::days(1),
            })
        }

        async fn access(
            &self,
            trip: &str,
            bearer: Option<&str>,
            jwt: Option<Jwt>,
        ) -> Result<TripAccess, u16> {
            let req = Req {
                method: Some(Method::Get),
                path: format!("/trips/{trip}"),
                query: HashMap::new(),
                bearer: bearer.map(str::to_owned),
                body: vec![],
                jwt,
                device: None,
            };
            self.app
                .trip_access(&req, &TripId::new(trip))
                .await
                .map_err(|e| e.status)
        }
    }

    #[tokio::test]
    async fn trip_access_resolves_owner_member_co_editor_and_guest() {
        let h = harness();
        h.storage
            .put_trip(&trip("amzn-owner", "kyoto"))
            .await
            .unwrap();
        h.storage
            .put_trip(&trip("amzn-owner", "lisbon"))
            .await
            .unwrap();

        // Owner of the trip's household.
        assert_eq!(
            h.access("kyoto", None, Some(owner_jwt())).await,
            Ok(TripAccess::Owner {
                household: HouseholdId::new("amzn-owner"),
                subject: "amzn-owner".into()
            })
        );
        assert_eq!(h.access("nope", None, Some(owner_jwt())).await, Err(404));

        // A second household that joined by code sees the trip, may not edit, and views
        // with its own pins.
        let ruth = h
            .member_household("amzn-ruth", "Ruth's living room", "kyoto")
            .await;
        let access = h.access("kyoto", Some(&ruth), None).await.unwrap();
        assert_eq!(
            access,
            TripAccess::Member {
                household: HouseholdId::new("amzn-ruth"),
                owner_household: HouseholdId::new("amzn-owner")
            }
        );
        assert!(!access.can_edit());
        assert_eq!(access.owner_household().as_str(), "amzn-owner");
        assert_eq!(access.viewing_household().as_str(), "amzn-ruth");
        assert_eq!(h.access("lisbon", Some(&ruth), None).await, Err(404));
        assert_eq!(
            h.storage
                .list_trip_members(&HouseholdId::new("amzn-owner"), &TripId::new("kyoto"))
                .await
                .unwrap()
                .len(),
            1
        );

        // Co-editor bound to the trip edits; a guest on the same trip does not.
        let (ken, ken_id) = h.co_editor("amzn-ken", "Ken", Some("kyoto")).await;
        let access = h.access("kyoto", Some(&ken), None).await.unwrap();
        assert_eq!(
            access,
            TripAccess::CoEditor {
                household: HouseholdId::new("amzn-owner"),
                contributor: ken_id.clone()
            }
        );
        assert!(access.can_edit());
        assert_eq!(access.subject(), ken_id.to_string());
        assert_eq!(h.access("lisbon", Some(&ken), None).await, Err(403));
        let (maya, _) = h.guest("Maya").await;
        // Household-scoped guest code names no trip.
        assert_eq!(h.access("kyoto", Some(&maya), None).await, Err(403));
        assert_eq!(h.access("kyoto", None, None).await, Err(401));
    }

    #[tokio::test]
    async fn co_editors_mint_tokens_for_their_actions_only() {
        let h = harness();
        h.storage
            .put_trip(&trip("amzn-owner", "kyoto"))
            .await
            .unwrap();
        let (ken, ken_id) = h.co_editor("amzn-ken", "Ken", Some("kyoto")).await;
        let (maya, _) = h.guest("Maya").await;

        let minted = h
            .bearer(
                Method::Post,
                "/confirmations",
                Some(json!({ "action": "remove_media", "media_ids": ["m1"], "by": ken_id })),
                &ken,
            )
            .await;
        assert_eq!(minted.status, 201, "{}", minted.body);
        assert_eq!(minted.body["tool"], "delete_media");
        let stored = h
            .storage
            .get_confirmation(minted.body["token"].as_str().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.subject.as_deref(), Some(ken_id.as_str()));

        let owner_only = h
            .bearer(
                Method::Post,
                "/confirmations",
                Some(json!({ "action": "remove_contributor", "contributor_id": "x" })),
                &ken,
            )
            .await;
        assert_eq!(owner_only.status, 403);
        let guest = h
            .bearer(
                Method::Post,
                "/confirmations",
                Some(json!({ "action": "apply_cleanup", "media_ids": ["m1"] })),
                &maya,
            )
            .await;
        assert_eq!(guest.status, 403);
        let owner = h
            .owner(
                Method::Post,
                "/confirmations",
                Some(json!({ "action": "apply_cleanup", "media_ids": ["m1"] })),
            )
            .await;
        assert_eq!(owner.status, 201);
        assert_eq!(owner.body["tool"], "apply_cleanup");
    }
}
