//! End-to-end flows through the router against in-memory backends.

#![allow(clippy::too_many_lines)]

use mantelpiece_api::app::{App, Config};
use mantelpiece_api::films::MemoryFilms;
use mantelpiece_api::req::{Jwt, Method, Req, Resp};
use mantelpiece_api::starter::MemoryStarter;
use mantelpiece_api::tokens::Signer;
use mantelpiece_core::ids::{HouseholdId, MediaId, TripId};
use mantelpiece_core::trip::{Route, Trip};
use mantelpiece_store::bus::MemoryBus;
use mantelpiece_store::faces::MemoryFaces;
use mantelpiece_store::live::MemoryLive;
use mantelpiece_store::mailer::MemoryMailer;
use mantelpiece_store::maps::MemoryMaps;
use mantelpiece_store::memory::{MemoryBlobs, MemoryStorage};
use mantelpiece_store::push::MemoryPush;
use mantelpiece_store::speech::MemorySpeech;
use mantelpiece_store::vectors::MemoryVectors;
use mantelpiece_store::Blobs as _;
use mantelpiece_store::{Storage, Vectors};
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
    films: Arc<MemoryFilms>,
    mailer: Arc<MemoryMailer>,
    push: Arc<MemoryPush>,
    live: Arc<MemoryLive>,
    now: Arc<Mutex<OffsetDateTime>>,
}

fn harness() -> Harness {
    harness_with(MemoryLive::default())
}

fn harness_with(live: MemoryLive) -> Harness {
    let storage = Arc::new(MemoryStorage::new());
    let blobs = Arc::new(MemoryBlobs::default());
    let bus = Arc::new(MemoryBus::default());
    let vectors = Arc::new(MemoryVectors::default());
    let starter = Arc::new(MemoryStarter::default());
    let films = Arc::new(MemoryFilms::default());
    let mailer = Arc::new(MemoryMailer::default());
    let push = Arc::new(MemoryPush::default());
    let live = Arc::new(live);
    let now = Arc::new(Mutex::new(datetime!(2026-09-08 10:00:00 UTC)));
    let clock_now = now.clone();
    let app = App {
        storage: storage.clone(),
        blobs: blobs.clone(),
        vectors: vectors.clone(),
        bus: bus.clone(),
        maps: Arc::new(MemoryMaps::default()),
        faces: Arc::new(MemoryFaces::with_faces_per_image(1)),
        speech: Arc::new(MemorySpeech::default()),
        mailer: mailer.clone(),
        push: push.clone(),
        live: live.clone(),
        starter: starter.clone(),
        films: films.clone(),
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
        films,
        mailer,
        push,
        live,
        now,
    };
    // The owner exists once they approved a TV; tests that skip that flow still need them.
    futures_block(
        h.storage
            .put_contributor(&mantelpiece_core::contributor::Contributor::owner(
                HouseholdId::new("amzn-owner"),
                "Hanyu",
                h.app.now(),
            )),
    );
    h
}

fn futures_block<F: std::future::Future<Output = mantelpiece_store::Result<()>>>(f: F) {
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
    m.state = mantelpiece_core::media::ProcessingState::Enriched;
    m.is_keeper = true;
    m.captured = Some(mantelpiece_core::media::CaptureTime::new(
        datetime!(2026-04-04 05:00:00 UTC),
        None,
    ));
    m.enrichment.scene_tags = tags
        .iter()
        .map(|t| mantelpiece_core::media::Tag {
            name: (*t).to_owned(),
            confidence: 0.9,
        })
        .collect();
    if let Some((landmark, text)) = description {
        m.description = Some(mantelpiece_core::media::Description {
            text: text.into(),
            landmark: Some(landmark.into()),
            model_id: "test".into(),
            generated_at: h.app.now(),
        });
    }
    h.storage.put_media(&m).await.unwrap();
    let vector = mantelpiece_ingest::embed::embed_media(
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
async fn proposals_wait_for_the_remote_and_tokens_close_them() {
    let h = harness();
    // Two bursts: a keeper and a non-keeper each.
    let mut ids = Vec::new();
    for (i, (group, keeper)) in [("g1", true), ("g1", false), ("g2", true), ("g2", false)]
        .iter()
        .enumerate()
    {
        let init = h
            .owner(Method::Post, "/uploads/init", Some(json!({ "filename": format!("{i}.jpg"), "bytes": 10, "sha256": sha(40 + u8::try_from(i).unwrap()) })))
            .await;
        let id = MediaId::new(init.body["media_id"].as_str().unwrap());
        let mut m = h
            .storage
            .get_media(&HouseholdId::new("amzn-owner"), &id)
            .await
            .unwrap()
            .unwrap();
        m.burst_group = Some((*group).to_owned());
        m.is_keeper = *keeper;
        h.storage.put_media(&m).await.unwrap();
        ids.push(id);
    }
    let action = json!({ "action": "apply_dedup", "group_ids": ["g1", "g2"] });

    // Without a token the tool is refused (F-504).
    let denied = h
        .owner(
            Method::Post,
            "/media/dedup/apply",
            Some(json!({ "group_ids": ["g1", "g2"] })),
        )
        .await;
    assert_eq!(denied.status, 403);

    // The agent opens a proposal: counts and samples computed server-side.
    let opened = h
        .owner(
            Method::Post,
            "/proposals",
            Some(json!({ "action": action, "trip": null })),
        )
        .await;
    assert_eq!(opened.status, 201, "{}", opened.body);
    assert_eq!(opened.body["before"], 4);
    assert_eq!(opened.body["after"], 2);
    assert_eq!(opened.body["samples"].as_array().unwrap().len(), 2);
    assert_eq!(opened.body["samples"][0]["keep"], ids[0].as_str());
    assert_eq!(opened.body["state"], "open");
    let pid = opened.body["id"].as_str().unwrap().to_owned();
    let feed = h.owner(Method::Get, "/events?since=0", None).await;
    assert!(feed.body["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "dedup.proposal" && e["removable"] == 2));
    assert_eq!(
        h.owner(Method::Get, "/proposals", None)
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Keep everything: nothing changes, and the proposal cannot be confirmed afterwards.
    let cancelled = h
        .owner(Method::Post, &format!("/proposals/{pid}/cancel"), None)
        .await;
    assert_eq!(cancelled.body["state"], "cancelled");
    let late = h
        .owner(
            Method::Post,
            "/confirmations",
            Some(json!({ "action": "apply_dedup", "group_ids": ["g1", "g2"], "proposal": pid })),
        )
        .await;
    assert_eq!(late.status, 409);
    assert_eq!(
        h.owner(Method::Get, "/proposals", None).await.body,
        json!([])
    );

    // A second proposal, confirmed on the remote, applied by the tool, then closed.
    let opened = h
        .owner(
            Method::Post,
            "/proposals",
            Some(json!({ "action": action })),
        )
        .await;
    let pid = opened.body["id"].as_str().unwrap().to_owned();
    let mismatch = h
        .owner(
            Method::Post,
            "/confirmations",
            Some(json!({ "action": "apply_dedup", "group_ids": ["g1"], "proposal": pid })),
        )
        .await;
    assert_eq!(mismatch.status, 400);
    let minted = h
        .owner(
            Method::Post,
            "/confirmations",
            Some(json!({ "action": "apply_dedup", "group_ids": ["g1", "g2"], "proposal": pid })),
        )
        .await;
    assert_eq!(minted.status, 201, "{}", minted.body);
    let token = minted.body["token"].as_str().unwrap().to_owned();
    let polled = h
        .owner(Method::Get, &format!("/proposals/{pid}"), None)
        .await;
    assert_eq!(polled.body["state"], "confirmed");
    assert_eq!(polled.body["confirmation"], token);
    let applied = h
        .owner(
            Method::Post,
            "/media/dedup/apply",
            Some(json!({ "group_ids": ["g1", "g2"], "confirmation": token })),
        )
        .await;
    assert_eq!(applied.status, 200, "{}", applied.body);
    assert_eq!(applied.body["removed"], 2);
    assert_eq!(
        h.owner(Method::Get, &format!("/proposals/{pid}"), None)
            .await
            .body["state"],
        "applied"
    );
    // The token is single use.
    let reused = h
        .owner(
            Method::Post,
            "/media/dedup/apply",
            Some(json!({ "group_ids": ["g1", "g2"], "confirmation": token })),
        )
        .await;
    assert_eq!(reused.status, 403);
    // Removed items are restorable for 30 days, then purged.
    let gone = &ids[1];
    let restored = h
        .owner(Method::Post, &format!("/media/{gone}/restore"), None)
        .await;
    assert_eq!(restored.status, 200, "{}", restored.body);
    assert!(!h
        .storage
        .get_media(&HouseholdId::new("amzn-owner"), gone)
        .await
        .unwrap()
        .unwrap()
        .is_removed());
    let purged = h.owner(Method::Post, "/media/purge", None).await;
    assert_eq!(purged.body["purged"], 0);
    h.advance(Duration::days(31));
    let purged = h.owner(Method::Post, "/media/purge", None).await;
    assert_eq!(purged.body["purged"], 1, "{}", purged.body);
    assert!(h
        .storage
        .get_media(&HouseholdId::new("amzn-owner"), &ids[3])
        .await
        .unwrap()
        .is_none());
    assert!(h
        .blobs
        .log
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.starts_with("delete originals/")));
    assert_eq!(
        h.owner(Method::Post, &format!("/media/{}/restore", ids[3]), None)
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn cleanup_and_delete_tools_need_tokens_and_open_proposals() {
    let h = harness();
    let init = h
        .owner(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": "shot.png", "bytes": 10, "sha256": sha(50) })),
        )
        .await;
    let shot = MediaId::new(init.body["media_id"].as_str().unwrap());
    let mut m = h
        .storage
        .get_media(&HouseholdId::new("amzn-owner"), &shot)
        .await
        .unwrap()
        .unwrap();
    m.cleanup_reason = Some(mantelpiece_core::media::CleanupReason::Screenshot);
    h.storage.put_media(&m).await.unwrap();
    let candidates = h.owner(Method::Get, "/media/cleanup", None).await;
    assert_eq!(candidates.body[0]["reason"], "screenshot");

    assert_eq!(
        h.owner(
            Method::Post,
            "/media/cleanup/apply",
            Some(json!({ "media_ids": [shot] }))
        )
        .await
        .status,
        403
    );
    assert_eq!(
        h.owner(
            Method::Post,
            "/media/delete",
            Some(json!({ "media_ids": [shot] }))
        )
        .await
        .status,
        403
    );
    let opened = h
        .owner(
            Method::Post,
            "/proposals",
            Some(json!({ "action": { "action": "apply_cleanup", "media_ids": [shot] } })),
        )
        .await;
    assert_eq!(opened.status, 201, "{}", opened.body);
    assert_eq!(opened.body["kind"], "cleanup");
    assert_eq!(opened.body["samples"][0]["reason"], "screenshot");
    let feed = h.owner(Method::Get, "/events?since=0", None).await;
    assert!(feed.body["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "cleanup.proposal" && e["count"] == 1));
    let pid = opened.body["id"].as_str().unwrap().to_owned();
    let minted = h
        .owner(
            Method::Post,
            "/confirmations",
            Some(json!({ "action": "apply_cleanup", "media_ids": [shot], "proposal": pid })),
        )
        .await;
    let token = minted.body["token"].as_str().unwrap().to_owned();
    let applied = h
        .owner(
            Method::Post,
            "/media/cleanup/apply",
            Some(json!({ "media_ids": [shot], "confirmation": token })),
        )
        .await;
    assert_eq!(applied.body["removed"], 1, "{}", applied.body);
    assert_eq!(
        h.owner(Method::Get, "/media/cleanup", None).await.body,
        json!([])
    );

    // delete_media by the owner needs its own token; a guest cannot use the tool at all.
    let init = h
        .owner(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": "x.jpg", "bytes": 10, "sha256": sha(51) })),
        )
        .await;
    let x = MediaId::new(init.body["media_id"].as_str().unwrap());
    let minted = h
        .owner(
            Method::Post,
            "/confirmations",
            Some(json!({ "action": "delete_media", "media_ids": [x] })),
        )
        .await;
    let token = minted.body["token"].as_str().unwrap().to_owned();
    let (maya, _) = h.guest("Maya").await;
    assert_eq!(
        h.bearer(
            Method::Post,
            "/media/delete",
            Some(json!({ "media_ids": [x], "confirmation": token })),
            &maya
        )
        .await
        .status,
        403
    );
    let deleted = h
        .owner(
            Method::Post,
            "/media/delete",
            Some(json!({ "media_ids": [x], "confirmation": token })),
        )
        .await;
    assert_eq!(deleted.body["removed"], 1, "{}", deleted.body);
    assert_eq!(
        h.owner(
            Method::Post,
            "/media/delete",
            Some(json!({ "media_ids": [x], "confirmation": token }))
        )
        .await
        .status,
        403
    );
}

#[tokio::test]
async fn a_second_household_joins_by_code_sees_the_trip_and_pins_privately() {
    let h = harness();
    let mut kyoto = trip("amzn-owner", "kyoto");
    kyoto.contributors = vec![mantelpiece_core::ids::ContributorId::new("amzn-owner")];
    let photo = enriched(&h, "temple.jpg", 61, &["Temple"], None).await;
    let other = enriched(&h, "ramen.jpg", 62, &["Ramen"], None).await;
    kyoto.days = vec![mantelpiece_core::trip::TripDay {
        landmarks: vec![],
        index: 1,
        date: time::macros::date!(2026 - 04 - 03),
        media: vec![photo.clone(), other.clone()],
        route: Route::default(),
        title: None,
        story: None,
        undated: vec![],
    }];
    kyoto.highlights = vec![photo.clone(), other.clone()];
    h.storage.put_trip(&kyoto).await.unwrap();
    for id in [&photo, &other] {
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
        .put_household(&mantelpiece_core::household::Household::new(
            HouseholdId::new("amzn-owner"),
            "Hanyu's living room",
            h.app.now(),
        ))
        .await
        .unwrap();
    h.storage
        .put_household(&mantelpiece_core::household::Household::new(
            HouseholdId::new("amzn-ruth"),
            "Ruth's living room",
            h.app.now(),
        ))
        .await
        .unwrap();
    let ruth = h.app.signer.sign(&mantelpiece_api::tokens::Claims {
        session: mantelpiece_api::tokens::Session::Tv {
            household: HouseholdId::new("amzn-ruth"),
            subject: "amzn-ruth".into(),
        },
        exp: h.app.now() + Duration::days(1),
    });

    let code = h
        .owner(
            Method::Post,
            "/trips/kyoto/code",
            Some(json!({ "expires_in_days": 7 })),
        )
        .await;
    let code = code.body["code"].as_str().unwrap().to_owned();
    // The owner cannot join their own trip; a household code opens no trip.
    assert_eq!(
        h.owner(Method::Post, "/memberships", Some(json!({ "code": code })))
            .await
            .status,
        400
    );
    let hc = h.owner(Method::Post, "/household/code", None).await;
    assert_eq!(
        h.bearer(
            Method::Post,
            "/memberships",
            Some(json!({ "code": hc.body["code"] })),
            &ruth
        )
        .await
        .status,
        400
    );

    let joined = h
        .bearer(
            Method::Post,
            "/memberships",
            Some(json!({ "code": code.to_lowercase() })),
            &ruth,
        )
        .await;
    assert_eq!(joined.status, 201, "{}", joined.body);
    assert_eq!(joined.body["name"], "Kyoto, April 2026");
    let again = h
        .bearer(
            Method::Post,
            "/memberships",
            Some(json!({ "code": code })),
            &ruth,
        )
        .await;
    assert_eq!(again.body["already"], true);
    // The owner's ticker hears about it; the trip lists the member.
    let feed = h.owner(Method::Get, "/events?since=0", None).await;
    assert!(feed.body["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "membership.joined" && e["member_name"] == "Ruth's living room"));
    assert_eq!(
        h.owner(
            Method::Post,
            "/trips/kyoto/code",
            Some(json!({ "expires_in_days": 7 }))
        )
        .await
        .body["member_households"],
        1
    );

    // Ruth's home lists the trip with a shared-by badge and can open it.
    let trips = h.bearer(Method::Get, "/trips", None, &ruth).await;
    assert_eq!(trips.body[0]["id"], "kyoto");
    assert_eq!(trips.body[0]["shared_by"], "Hanyu's living room");
    let view = h.bearer(Method::Get, "/trips/kyoto", None, &ruth).await;
    assert_eq!(view.status, 200, "{}", view.body);
    assert_eq!(view.body["shared_by"], "Hanyu's living room");

    // Ruth pins the ramen photo: first in her highlights, unchanged for Hanyu (F-163).
    let pinned = h
        .bearer(Method::Post, &format!("/media/{other}/pin"), None, &ruth)
        .await;
    assert_eq!(pinned.status, 200, "{}", pinned.body);
    assert_eq!(pinned.body["pinned"], true);
    let hers = h.bearer(Method::Get, "/trips/kyoto", None, &ruth).await;
    assert_eq!(hers.body["highlights"][0], other.as_str());
    let his = h.owner(Method::Get, "/trips/kyoto", None).await;
    assert_eq!(his.body["highlights"][0], photo.as_str());
    let stored = h
        .storage
        .get_media(&HouseholdId::new("amzn-owner"), &other)
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.pinned && stored.pinned_by.contains(&HouseholdId::new("amzn-ruth")));
    // And hides it: gone from her day, still on his.
    h.bearer(Method::Post, &format!("/media/{other}/hide"), None, &ruth)
        .await;
    let hers = h.bearer(Method::Get, "/trips/kyoto", None, &ruth).await;
    assert_eq!(hers.body["days"][0]["media"].as_array().unwrap().len(), 1);
    assert_eq!(
        h.owner(Method::Get, "/trips/kyoto", None).await.body["days"][0]["media"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Member feeds mirror trip events they may see, and nothing else.
    h.app
        .emit(
            &HouseholdId::new("amzn-owner"),
            mantelpiece_core::events::EventKind::ContributionReceived {
                trip: Some(TripId::new("kyoto")),
                contributor: mantelpiece_core::ids::ContributorId::new("c"),
                display_name: "Maya".into(),
                count: 3,
            },
        )
        .await
        .unwrap();
    h.app
        .emit(
            &HouseholdId::new("amzn-owner"),
            mantelpiece_core::events::EventKind::UploadProgress {
                contributor: mantelpiece_core::ids::ContributorId::new("c"),
                done: 1,
                total: 3,
            },
        )
        .await
        .unwrap();
    let ruth_feed = h.bearer(Method::Get, "/events?since=0", None, &ruth).await;
    let kinds: Vec<&str> = ruth_feed.body["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["contribution.received"]);

    // Members never edit; leaving removes the badge and the pointer.
    assert_eq!(
        h.bearer(
            Method::Patch,
            "/trips/kyoto",
            Some(json!({ "name": "Mine now" })),
            &ruth
        )
        .await
        .status,
        403
    );
    assert_eq!(
        h.bearer(Method::Post, &format!("/media/{photo}/keeper"), None, &ruth)
            .await
            .status,
        403
    );
    assert_eq!(
        h.bearer(Method::Delete, "/memberships/kyoto", None, &ruth)
            .await
            .status,
        204
    );
    assert_eq!(
        h.bearer(Method::Get, "/trips", None, &ruth).await.body,
        json!([])
    );
    assert_eq!(
        h.bearer(Method::Get, "/trips/kyoto", None, &ruth)
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn roles_rename_and_guests_never_edit() {
    let h = harness();
    let mut kyoto = trip("amzn-owner", "kyoto");
    kyoto.contributors = vec![mantelpiece_core::ids::ContributorId::new("amzn-owner")];
    h.storage.put_trip(&kyoto).await.unwrap();
    let (ken_token, ken) = h.co_editor("amzn-ken", "Ken", Some("kyoto")).await;
    // Demote then promote through the route; a guest can never be promoted.
    let mut t = h
        .storage
        .get_trip(&HouseholdId::new("amzn-owner"), &TripId::new("kyoto"))
        .await
        .unwrap()
        .unwrap();
    t.contributors.push(ken.clone());
    h.storage.put_trip(&t).await.unwrap();
    let demoted = h
        .owner(
            Method::Post,
            "/trips/kyoto/roles",
            Some(json!({ "contributor": ken, "role": "contributor" })),
        )
        .await;
    assert_eq!(demoted.status, 200, "{}", demoted.body);
    assert_eq!(
        h.bearer(
            Method::Patch,
            "/trips/kyoto",
            Some(json!({ "name": "Kyoto in bloom" })),
            &ken_token
        )
        .await
        .status,
        403
    );
    let promoted = h
        .owner(
            Method::Post,
            "/trips/kyoto/roles",
            Some(json!({ "contributor": ken, "role": "co_editor" })),
        )
        .await;
    assert_eq!(promoted.body["role"], "co_editor");
    let renamed = h
        .bearer(
            Method::Patch,
            "/trips/kyoto",
            Some(json!({ "name": "Kyoto in bloom" })),
            &ken_token,
        )
        .await;
    assert_eq!(renamed.status, 200, "{}", renamed.body);
    assert_eq!(
        h.owner(Method::Get, "/trips/kyoto", None).await.body["name"],
        "Kyoto in bloom"
    );
    assert_eq!(
        h.owner(Method::Get, "/trips/kyoto", None).await.body["contributor_roles"][ken.as_str()],
        "co_editor"
    );

    let code = h
        .owner(
            Method::Post,
            "/trips/kyoto/code",
            Some(json!({ "expires_in_days": 7 })),
        )
        .await;
    let joined = h
        .call(
            Method::Post,
            "/join",
            Some(json!({ "code": code.body["code"], "display_name": "Maya" })),
            None,
            None,
        )
        .await;
    let maya_token = joined.body["token"].as_str().unwrap().to_owned();
    let maya = joined.body["contributor"].as_str().unwrap().to_owned();
    let guest_promo = h
        .owner(
            Method::Post,
            "/trips/kyoto/roles",
            Some(json!({ "contributor": maya, "role": "co_editor" })),
        )
        .await;
    assert_eq!(guest_promo.status, 400);
    assert!(guest_promo.body["error"]
        .as_str()
        .unwrap()
        .contains("guests"));

    // F-165: no edit route accepts a guest.
    let init = h
        .owner(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": "a.jpg", "bytes": 10, "sha256": sha(70) })),
        )
        .await;
    let media = init.body["media_id"].as_str().unwrap().to_owned();
    for (method, path, body) in [
        (Method::Post, format!("/media/{media}/pin"), None),
        (Method::Post, format!("/media/{media}/hide"), None),
        (Method::Post, format!("/media/{media}/keeper"), None),
        (
            Method::Patch,
            "/trips/kyoto".to_owned(),
            Some(json!({ "name": "x" })),
        ),
        (
            Method::Post,
            "/trips/kyoto/roles".to_owned(),
            Some(json!({ "contributor": maya, "role": "contributor" })),
        ),
        (
            Method::Post,
            "/trips/kyoto/narrative".to_owned(),
            Some(json!({ "text": "x", "model_id": "m" })),
        ),
        (
            Method::Post,
            "/trips/kyoto/narrative/regenerate".to_owned(),
            None,
        ),
        (
            Method::Post,
            "/proposals".to_owned(),
            Some(json!({ "action": { "action": "apply_cleanup", "media_ids": [] } })),
        ),
        (
            Method::Post,
            "/confirmations".to_owned(),
            Some(json!({ "action": "apply_cleanup", "media_ids": [] })),
        ),
        (
            Method::Post,
            "/memberships".to_owned(),
            Some(json!({ "code": "x" })),
        ),
        (
            Method::Post,
            "/playlists".to_owned(),
            Some(json!({ "name": "n", "media": [media], "source": "s" })),
        ),
    ] {
        let resp = h.bearer(method, &path, body, &maya_token).await;
        assert_eq!(resp.status, 403, "{method:?} {path}: {}", resp.body);
    }
    // The read-only web view has no session at all.
    assert_eq!(
        h.call(
            Method::Post,
            &format!(
                "/media/{media}/pin?code={}",
                code.body["code"].as_str().unwrap()
            ),
            None,
            None,
            None
        )
        .await
        .status,
        401
    );
}

/// An owner-uploaded keeper with a capture time, GPS and score, for memories tests.
async fn seeded(
    h: &Harness,
    name: &str,
    seed: u8,
    when: OffsetDateTime,
    gps: Option<(f64, f64)>,
    score: f32,
) -> MediaId {
    let house = HouseholdId::new("amzn-owner");
    let init = h
        .owner(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": name, "bytes": 10, "sha256": sha(seed) })),
        )
        .await;
    let id = MediaId::new(init.body["media_id"].as_str().unwrap());
    let mut m = h.storage.get_media(&house, &id).await.unwrap().unwrap();
    m.is_keeper = true;
    m.captured = Some(mantelpiece_core::media::CaptureTime::new(when, None));
    m.gps = gps.map(|(a, b)| mantelpiece_core::geo::GeoPoint::new(a, b));
    m.quality_score = Some(score);
    h.storage.put_media(&m).await.unwrap();
    id
}

#[tokio::test]
async fn memories_home_and_the_all_trips_map() {
    let h = harness();
    // Today is 2026-09-08 in the harness clock.
    let then = seeded(
        &h,
        "then.jpg",
        81,
        datetime!(2024-09-08 10:00:00 UTC),
        Some((35.0394, 135.7292)),
        7.0,
    )
    .await;
    let now = seeded(
        &h,
        "now.jpg",
        82,
        datetime!(2026-09-08 10:00:00 UTC),
        Some((35.0395, 135.7293)),
        8.0,
    )
    .await;
    let august = seeded(
        &h,
        "aug.jpg",
        83,
        datetime!(2026-08-20 10:00:00 UTC),
        None,
        9.0,
    )
    .await;
    let last_year = seeded(
        &h,
        "ly.jpg",
        84,
        datetime!(2025-03-01 10:00:00 UTC),
        None,
        6.0,
    )
    .await;

    let home = h.owner(Method::Get, "/memories/home", None).await;
    assert_eq!(home.status, 200, "{}", home.body);
    assert_eq!(home.body["date"], "2026-09-08");
    assert_eq!(home.body["on_this_day"], json!([then]));
    assert_eq!(home.body["then_and_now"][0]["then"], then.as_str());
    assert_eq!(home.body["then_and_now"][0]["now"], now.as_str());
    assert_eq!(home.body["then_and_now"][0]["years_apart"], 2);
    assert_eq!(home.body["best_of_month"]["month"], "August");
    assert_eq!(home.body["best_of_month"]["media"], json!([august]));
    assert_eq!(home.body["best_of_year"]["media"], json!([last_year]));
    assert_eq!(
        h.owner(Method::Get, "/memories/home?date=nope", None)
            .await
            .status,
        400
    );
    let other_day = h
        .owner(Method::Get, "/memories/home?date=2026-03-01", None)
        .await;
    assert_eq!(other_day.body["on_this_day"], json!([last_year]));

    // The map needs trips with a route; first call draws, the second is served from S3.
    let empty = h.owner(Method::Get, "/map", None).await;
    assert!(empty.body["url"].is_null());
    let mut kyoto = trip("amzn-owner", "kyoto");
    kyoto.route.points.push(mantelpiece_core::trip::RoutePoint {
        estimated: false,
        at: mantelpiece_core::geo::GeoPoint::new(35.0, 135.7),
        time: kyoto.start,
        media: then.clone(),
    });
    h.storage.put_trip(&kyoto).await.unwrap();
    let drawn = h.owner(Method::Get, "/map", None).await;
    assert_eq!(drawn.status, 200, "{}", drawn.body);
    assert!(drawn.body["url"]
        .as_str()
        .unwrap()
        .contains("maps/amzn-owner/all-1.png"));
    assert_eq!(drawn.body["pins"][0]["name"], "Kyoto, April 2026");
    let again = h.owner(Method::Get, "/map", None).await;
    assert_eq!(again.body["url"], drawn.body["url"]);
    assert_eq!(
        h.blobs
            .log
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.starts_with("put maps/"))
            .count(),
        1
    );
}

#[tokio::test]
async fn people_opt_in_naming_merging_search_and_deletion() {
    let h = harness();
    let house = HouseholdId::new("amzn-owner");
    let listed = h.owner(Method::Get, "/people", None).await;
    assert_eq!(listed.body["people_enabled"], false);
    assert_eq!(
        h.owner(Method::Post, "/people/enable", None).await.body["people_enabled"],
        true
    );
    assert_eq!(
        h.owner(Method::Get, "/me", None).await.body["people_enabled"],
        true
    );

    // Two clusters from ingest (simulated), on a beach photo and a temple photo.
    let beach = enriched(&h, "beach.jpg", 91, &["Beach"], None).await;
    let temple = enriched(&h, "temple.jpg", 92, &["Temple"], None).await;
    let maya =
        mantelpiece_core::person::Person::new(house.clone(), "f1", beach.clone(), h.app.now());
    let mut maya_again =
        mantelpiece_core::person::Person::new(house.clone(), "f2", temple.clone(), h.app.now());
    maya_again.add_face("f3", beach.clone());
    h.storage.put_person(&maya).await.unwrap();
    h.storage.put_person(&maya_again).await.unwrap();
    for (id, people) in [
        (&beach, vec![maya.id.clone(), maya_again.id.clone()]),
        (&temple, vec![maya_again.id.clone()]),
    ] {
        let mut m = h.storage.get_media(&house, id).await.unwrap().unwrap();
        m.people = people;
        h.storage.put_media(&m).await.unwrap();
    }
    let listed = h.owner(Method::Get, "/people", None).await;
    assert_eq!(listed.body["people"].as_array().unwrap().len(), 2);
    assert_eq!(
        listed.body["people"][0]["media_count"], 2,
        "{}",
        listed.body
    );

    // Name one, merge the other into it: one person, media re-pointed.
    let named = h
        .owner(
            Method::Patch,
            &format!("/people/{}", maya.id),
            Some(json!({ "name": "Maya" })),
        )
        .await;
    assert_eq!(named.body["name"], "Maya");
    assert_eq!(
        h.owner(
            Method::Patch,
            &format!("/people/{}", maya.id),
            Some(json!({ "merge_into": maya.id }))
        )
        .await
        .status,
        400
    );
    let merged = h
        .owner(
            Method::Patch,
            &format!("/people/{}", maya_again.id),
            Some(json!({ "merge_into": maya.id })),
        )
        .await;
    assert_eq!(merged.status, 200, "{}", merged.body);
    assert_eq!(merged.body["media_count"], 3);
    assert_eq!(
        h.owner(Method::Get, "/people", None).await.body["people"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        h.storage
            .get_media(&house, &temple)
            .await
            .unwrap()
            .unwrap()
            .people,
        vec![maya.id.clone()]
    );

    // "Maya at the beach" returns only her photos; an unknown name is a 404.
    let found = h
        .owner(
            Method::Post,
            "/search",
            Some(json!({ "query": "beach temple", "people": ["maya"] })),
        )
        .await;
    assert_eq!(found.status, 200, "{}", found.body);
    let ids: Vec<&str> = found.body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["media"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&beach.as_str()) && ids.contains(&temple.as_str()));
    assert_eq!(
        h.owner(
            Method::Post,
            "/search",
            Some(json!({ "query": "beach", "people": ["Ken"] }))
        )
        .await
        .status,
        404
    );

    // Turning People off deletes the collection and every person; media forget them.
    assert_eq!(
        h.owner(Method::Post, "/people/disable", None).await.body["people_enabled"],
        false
    );
    assert_eq!(
        h.owner(Method::Get, "/people", None).await.body["people"],
        json!([])
    );
    assert_eq!(
        h.storage
            .get_media(&house, &beach)
            .await
            .unwrap()
            .unwrap()
            .people,
        Vec::<mantelpiece_core::ids::PersonId>::new()
    );

    // Account deletion: uploads answer 410 while it runs, and everything is gone after.
    let had_trips = !h.storage.list_trips(&house).await.unwrap().is_empty();
    let deleted = h.owner(Method::Delete, "/me", None).await;
    assert_eq!(deleted.status, 200, "{}", deleted.body);
    assert_eq!(deleted.body["media"], 2);
    assert_eq!(
        h.owner(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": "late.jpg", "bytes": 10, "sha256": sha(93) }))
        )
        .await
        .status,
        410
    );
    assert_eq!(
        h.storage.list_media(&house).await.unwrap(),
        Vec::<mantelpiece_core::media::MediaItem>::new()
    );
    assert_eq!(
        h.storage.list_trips(&house).await.unwrap(),
        Vec::<Trip>::new()
    );
    assert!(h
        .blobs
        .log
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.starts_with("delete originals/")));
    assert!(
        !had_trips
            || h.blobs
                .log
                .lock()
                .unwrap()
                .iter()
                .any(|l| l.contains("sketch/")),
        "sketch maps go with the trips (F-712)"
    );
    assert!(h
        .storage
        .get_household(&house)
        .await
        .unwrap()
        .unwrap()
        .is_deleting());
}

#[tokio::test]
async fn reactions_from_contributors_viewers_and_members_reach_the_ticker() {
    let h = harness();
    let mut kyoto = trip("amzn-owner", "kyoto");
    let photo = enriched(&h, "ramen.jpg", 101, &["Ramen"], None).await;
    kyoto.days = vec![mantelpiece_core::trip::TripDay {
        landmarks: vec![],
        index: 3,
        date: time::macros::date!(2026 - 04 - 05),
        media: vec![photo.clone()],
        route: Route::default(),
        title: None,
        story: None,
        undated: vec![],
    }];
    h.storage.put_trip(&kyoto).await.unwrap();
    let code = h
        .owner(
            Method::Post,
            "/trips/kyoto/code",
            Some(json!({ "expires_in_days": 7 })),
        )
        .await;
    let code = code.body["code"].as_str().unwrap().to_owned();

    // Grandma on the read-only page, by code with a name; liking twice counts once.
    let liked = h
        .call(
            Method::Post,
            &format!("/trips/kyoto/reactions?code={code}"),
            Some(json!({ "target": "day", "index": 3, "kind": "like", "name": "Grandma Ruth" })),
            None,
            None,
        )
        .await;
    assert_eq!(liked.status, 201, "{}", liked.body);
    h.call(
        Method::Post,
        &format!("/trips/kyoto/reactions?code={code}"),
        Some(json!({ "target": "day", "index": 3, "kind": "like", "name": "Grandma Ruth" })),
        None,
        None,
    )
    .await;
    let nameless = h
        .call(
            Method::Post,
            &format!("/trips/kyoto/reactions?code={code}"),
            Some(json!({ "target": "day", "index": 3, "kind": "like" })),
            None,
            None,
        )
        .await;
    assert_eq!(nameless.status, 400);
    let long = "x".repeat(201);
    let too_long = h.call(Method::Post, &format!("/trips/kyoto/reactions?code={code}"), Some(json!({ "target": "media", "id": photo, "kind": "comment", "text": long, "name": "Grandma Ruth" })), None, None).await;
    assert_eq!(too_long.status, 400);
    let commented = h.call(Method::Post, &format!("/trips/kyoto/reactions?code={code}"), Some(json!({ "target": "media", "id": photo, "kind": "comment", "text": "That ramen!", "name": "Grandma Ruth" })), None, None).await;
    assert_eq!(commented.status, 201, "{}", commented.body);

    // A contributor with an upload session likes the photo too.
    let joined = h
        .call(
            Method::Post,
            "/join",
            Some(json!({ "code": code, "display_name": "Maya" })),
            None,
            None,
        )
        .await;
    let maya = joined.body["token"].as_str().unwrap().to_owned();
    assert_eq!(
        h.bearer(
            Method::Post,
            "/trips/kyoto/reactions",
            Some(json!({ "target": "media", "id": photo, "kind": "like" })),
            &maya
        )
        .await
        .status,
        201
    );
    assert_eq!(
        h.bearer(
            Method::Post,
            "/trips/lisbon/reactions",
            Some(json!({ "target": "day", "index": 1, "kind": "like" })),
            &maya
        )
        .await
        .status,
        403
    );

    let listed = h
        .call(
            Method::Get,
            &format!("/trips/kyoto/reactions?code={code}"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(listed.status, 200, "{}", listed.body);
    assert_eq!(listed.body["reactions"].as_array().unwrap().len(), 3);
    assert_eq!(listed.body["summary"]["day:3"]["likes"], 1);
    assert_eq!(listed.body["summary"][format!("media:{photo}")]["likes"], 1);
    assert_eq!(
        listed.body["summary"][format!("media:{photo}")]["latest_comment"]["text"],
        "That ramen!"
    );
    assert_eq!(
        listed.body["summary"][format!("media:{photo}")]["latest_comment"]["by"],
        "Grandma Ruth"
    );

    // The owner's ticker heard every one.
    let feed = h.owner(Method::Get, "/events?since=0", None).await;
    let heard: Vec<String> = feed.body["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "reaction.received")
        .map(|e| {
            format!(
                "{} {}",
                e["actor_name"].as_str().unwrap(),
                e["kind"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        heard,
        vec![
            "Grandma Ruth like",
            "Grandma Ruth like",
            "Grandma Ruth comment",
            "Maya like"
        ]
    );
    assert_eq!(
        h.owner(Method::Get, "/trips/kyoto/reactions", None)
            .await
            .status,
        200
    );
    assert_eq!(
        h.call(Method::Get, "/trips/kyoto/reactions", None, None, None)
            .await
            .status,
        401
    );
}

#[tokio::test]
async fn audit_log_records_sign_in_shares_and_gated_tools_with_their_tokens() {
    let h = harness();
    let a = enriched(&h, "a.jpg", 111, &["Temple"], None).await;
    let playlist = h
        .owner(
            Method::Post,
            "/playlists",
            Some(json!({ "name": "Temples", "media": [a], "source": "ask:temple" })),
        )
        .await
        .body["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        h.owner(
            Method::Post,
            "/shares",
            Some(json!({ "playlist": playlist }))
        )
        .await
        .status,
        403
    );
    let token = h
        .owner(
            Method::Post,
            "/confirmations",
            Some(json!({ "action": "share_album", "album_id": playlist })),
        )
        .await
        .body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        h.owner(
            Method::Post,
            "/shares",
            Some(json!({ "playlist": playlist, "confirmation": token }))
        )
        .await
        .status,
        201
    );
    h.owner(Method::Get, "/trips", None).await;
    h.owner(Method::Get, "/shares", None).await;

    let log = h.owner(Method::Get, "/audit", None).await;
    assert_eq!(log.status, 200);
    let entries = log.body.as_array().unwrap();
    let actions: Vec<&str> = entries
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    // Newest first; reads are not logged.
    assert_eq!(
        actions,
        vec!["share.create", "confirmation.mint", "share.create"]
    );
    assert_eq!(entries[0]["confirmation"], token);
    assert_eq!(entries[0]["outcome"], 201);
    assert_eq!(entries[0]["actor"], "amzn-owner");
    assert_eq!(entries[2]["outcome"], 403);
    assert!(entries[2]["confirmation"].is_null());
    // Nobody but the owner reads it.
    let (guest, _) = h.guest("Maya").await;
    assert_eq!(
        h.bearer(Method::Get, "/audit", None, &guest).await.status,
        403
    );

    // OAuth metadata for the Alexa+ add-on (R6): PKCE only, one scope, points at the Gateway.
    let doc = h
        .call(
            Method::Get,
            "/.well-known/oauth-authorization-server",
            None,
            None,
            None,
        )
        .await;
    assert_eq!(doc.status, 200);
    assert_eq!(
        doc.body["code_challenge_methods_supported"],
        json!(["S256"])
    );
    assert_eq!(
        doc.body["authorization_endpoint"],
        "https://mantelpiece-test.auth.us-east-1.amazoncognito.com/oauth2/authorize"
    );
    assert_eq!(doc.body["scopes_supported"][0], "mantelpiece/tools");
    let res = h
        .call(
            Method::Get,
            "/.well-known/oauth-protected-resource",
            None,
            None,
            None,
        )
        .await;
    assert_eq!(res.body["resource"], "https://gateway.mantelpiece.test/mcp");
    assert_eq!(res.body["mcp_protocol_version"], "2025-11-25");
}

#[tokio::test]
async fn share_links_need_a_confirmation_and_open_publicly_until_they_expire() {
    let h = harness();
    h.storage
        .put_household(&mantelpiece_core::household::Household::new(
            HouseholdId::new("amzn-owner"),
            "Living room",
            h.app.now(),
        ))
        .await
        .unwrap();
    let a = enriched(&h, "a.jpg", 111, &["Temple"], None).await;
    let b = enriched(&h, "b.jpg", 112, &["Temple"], None).await;
    let hidden = enriched(&h, "c.jpg", 113, &["Temple"], None).await;
    h.owner(Method::Post, &format!("/media/{hidden}/hide"), None)
        .await;
    let playlist = h
        .owner(
            Method::Post,
            "/playlists",
            Some(
                json!({ "name": "Kyoto temples", "media": [a, b, hidden], "source": "ask:temple" }),
            ),
        )
        .await
        .body["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // No token, or a token for another album: refused.
    assert_eq!(
        h.owner(
            Method::Post,
            "/shares",
            Some(json!({ "playlist": playlist }))
        )
        .await
        .status,
        403
    );
    let wrong = h
        .owner(
            Method::Post,
            "/confirmations",
            Some(json!({ "action": "share_album", "album_id": "other" })),
        )
        .await
        .body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        h.owner(
            Method::Post,
            "/shares",
            Some(json!({ "playlist": playlist, "confirmation": wrong }))
        )
        .await
        .status,
        403
    );

    // A TV-minted ShareAlbum token publishes the playlist for seven days and emails it.
    let token = h
        .owner(
            Method::Post,
            "/confirmations",
            Some(json!({ "action": "share_album", "album_id": playlist })),
        )
        .await
        .body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        h.owner(
            Method::Post,
            "/shares",
            Some(json!({ "playlist": playlist, "confirmation": token, "email": "grandma" }))
        )
        .await
        .status,
        400,
        "bad email"
    );
    let made = h
        .owner(
            Method::Post,
            "/shares",
            Some(
                json!({ "playlist": playlist, "confirmation": token, "email": "ruth@example.com" }),
            ),
        )
        .await;
    assert_eq!(made.status, 201, "{}", made.body);
    let id = made.body["id"].as_str().unwrap().to_owned();
    assert_eq!(
        made.body["link"],
        format!("https://upload.mantelpiece.test/s.html?s={id}")
    );
    assert_eq!(made.body["expires_at"], "2026-09-15T10:00:00Z");
    let sent = h.mailer.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, "ruth@example.com");
    assert!(sent[0].text.contains(&format!("s.html?s={id}")));
    assert!(sent[0].subject.contains("Kyoto temples"));
    // The token was single use.
    assert_eq!(
        h.owner(
            Method::Post,
            "/shares",
            Some(json!({ "playlist": playlist, "confirmation": token }))
        )
        .await
        .status,
        403
    );

    // Public, no session: live unhidden photos with presigned renditions.
    let open = h
        .call(Method::Get, &format!("/s/{id}"), None, None, None)
        .await;
    assert_eq!(open.status, 200, "{}", open.body);
    assert_eq!(open.body["name"], "Kyoto temples");
    assert_eq!(open.body["shared_by"], "Living room");
    let ids: Vec<&str> = open.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![a.as_str(), b.as_str()]);
    assert!(open.body["items"][0]["url"]
        .as_str()
        .unwrap()
        .contains("tv.jpg"));
    assert_eq!(
        h.call(Method::Get, "/s/nope", None, None, None)
            .await
            .status,
        404
    );

    // Listed for the owner; expired after a week; revocable before that.
    assert_eq!(
        h.owner(Method::Get, "/shares", None)
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    *h.now.lock().unwrap() += time::Duration::days(8);
    assert_eq!(
        h.call(Method::Get, &format!("/s/{id}"), None, None, None)
            .await
            .status,
        410
    );
    *h.now.lock().unwrap() -= time::Duration::days(8);
    assert_eq!(
        h.call(Method::Get, &format!("/s/{id}"), None, None, None)
            .await
            .status,
        200
    );
    assert_eq!(
        h.owner(Method::Delete, &format!("/shares/{id}"), None)
            .await
            .body["revoked"],
        true
    );
    assert_eq!(
        h.call(Method::Get, &format!("/s/{id}"), None, None, None)
            .await
            .status,
        410
    );
}

#[tokio::test]
async fn narration_is_behind_a_flag_and_the_ambient_playlist_honours_pins_and_hides() {
    let h = harness();
    let house = HouseholdId::new("amzn-owner");
    let mut kyoto = trip("amzn-owner", "kyoto");
    kyoto.days = vec![
        mantelpiece_core::trip::TripDay {
            landmarks: vec![],
            index: 1,
            date: time::macros::date!(2026 - 04 - 03),
            media: vec![],
            route: Route::default(),
            title: None,
            story: Some("Hanyu started at Kinkaku-ji.".into()),
            undated: vec![],
        },
        mantelpiece_core::trip::TripDay {
            landmarks: vec![],
            index: 2,
            date: time::macros::date!(2026 - 04 - 04),
            media: vec![],
            route: Route::default(),
            title: None,
            story: None,
            undated: vec![],
        },
    ];
    h.storage.put_trip(&kyoto).await.unwrap();

    // Off by default: the tool refuses; the flag flips through PATCH /me.
    assert_eq!(
        h.owner(
            Method::Post,
            "/trips/kyoto/narration",
            Some(json!({ "day": 1 }))
        )
        .await
        .status,
        409
    );
    let on = h
        .owner(
            Method::Patch,
            "/me",
            Some(json!({ "narration_enabled": true })),
        )
        .await;
    assert_eq!(on.body["narration_enabled"], true);
    assert_eq!(
        h.owner(Method::Get, "/trips/kyoto/narration?day=1", None)
            .await
            .status,
        404
    );
    let made = h
        .owner(
            Method::Post,
            "/trips/kyoto/narration",
            Some(json!({ "day": 1 })),
        )
        .await;
    assert_eq!(made.status, 201, "{}", made.body);
    assert_eq!(
        made.body["audio_key"],
        "narration/amzn-owner/kyoto/day-1.mp3"
    );
    assert_eq!(
        h.blobs
            .get("narration/amzn-owner/kyoto/day-1.mp3")
            .await
            .unwrap(),
        b"Hanyu started at Kinkaku-ji."
    );
    assert_eq!(
        h.owner(
            Method::Post,
            "/trips/kyoto/narration",
            Some(json!({ "day": 2 }))
        )
        .await
        .status,
        409,
        "no story yet"
    );
    assert_eq!(
        h.owner(
            Method::Post,
            "/trips/kyoto/narration",
            Some(json!({ "day": 9 }))
        )
        .await
        .status,
        404
    );
    let url = h
        .owner(Method::Get, "/trips/kyoto/narration?day=1", None)
        .await;
    assert!(url.body["url"].as_str().unwrap().ends_with("day-1.mp3"));
    let feed = h.owner(Method::Get, "/events?since=0", None).await;
    assert!(feed.body["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "narration.ready" && e["day"] == 1));

    // The ambient playlist comes back with this household's pins first and hides gone.
    let a = enriched(&h, "a.jpg", 111, &["Beach"], None).await;
    let b = enriched(&h, "b.jpg", 112, &["Temple"], None).await;
    let c = enriched(&h, "c.jpg", 113, &["Ramen"], None).await;
    let created = h.owner(Method::Post, "/playlists", Some(json!({ "name": "Ambient", "media": [a, b, c], "source": "ambient from recent highlights" }))).await;
    let id = created.body["id"].as_str().unwrap().to_owned();
    h.owner(Method::Post, &format!("/media/{c}/pin"), None)
        .await;
    h.owner(Method::Post, &format!("/media/{b}/hide"), None)
        .await;
    let got = h
        .owner(Method::Get, &format!("/playlists/{id}"), None)
        .await;
    assert_eq!(got.body["media"], json!([c, a]));
    let _ = house;
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
    assert_eq!(none.body["results"], json!([]));
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
    assert_eq!(
        h.owner(Method::Get, "/presence", None).await.body,
        json!([])
    );

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
    kyoto.contributors = vec![mantelpiece_core::ids::ContributorId::new("amzn-owner")];
    kyoto.days = (1..=2)
        .map(|i| mantelpiece_core::trip::TripDay {
            landmarks: vec![],
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
        .put_contributor(&mantelpiece_core::contributor::Contributor::owner(
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
            m.state = mantelpiece_core::media::ProcessingState::Enriched;
            m.enrichment.scene_tags = vec![mantelpiece_core::media::Tag {
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

#[tokio::test]
async fn an_alexa_skill_trades_its_cognito_token_for_an_owner_session() {
    let h = harness();
    let anon = h
        .call(Method::Post, "/session/amazon", None, None, None)
        .await;
    assert_eq!(anon.status, 401);

    let first = h.owner(Method::Post, "/session/amazon", None).await;
    assert_eq!(first.status, 200, "{}", first.body);
    assert_eq!(first.body["household"], "amzn-owner");
    assert!(first.body["household_name"].is_null());
    let token = first.body["token"].as_str().unwrap().to_owned();
    let me = h.bearer(Method::Get, "/me", None, &token).await;
    assert_eq!(me.body["role"], "owner");
    assert_eq!(me.body["household"], "amzn-owner");

    // The owner contributor exists after a first sign-in, named from the JWT.
    let contributors = h.bearer(Method::Get, "/contributors", None, &token).await;
    assert_eq!(contributors.status, 200, "{}", contributors.body);
    let names: Vec<&str> = contributors
        .body
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["display_name"].as_str())
        .collect();
    assert!(names.contains(&"Hanyu"), "{names:?}");

    // Trips are visible through the session like on the TV.
    let trips = h.bearer(Method::Get, "/trips", None, &token).await;
    assert_eq!(trips.status, 200);
}

#[tokio::test]
async fn a_play_request_reaches_the_tv_through_the_feed() {
    let h = harness();
    let mut kyoto = trip("amzn-owner", "kyoto");
    kyoto.days = vec![mantelpiece_core::trip::TripDay {
        landmarks: vec![],
        index: 1,
        date: time::macros::date!(2026 - 04 - 03),
        media: vec![MediaId::new("m1"), MediaId::new("m2")],
        route: Route::default(),
        title: None,
        story: None,
        undated: vec![],
    }];
    h.storage.put_trip(&kyoto).await.unwrap();
    let session = h.owner(Method::Post, "/session/amazon", None).await;
    let token = session.body["token"].as_str().unwrap().to_owned();

    let missing = h
        .bearer(Method::Post, "/trips/nope/play", None, &token)
        .await;
    assert_eq!(missing.status, 404);

    let asked = h
        .bearer(
            Method::Post,
            "/trips/kyoto/play",
            Some(json!({ "source": "alexa" })),
            &token,
        )
        .await;
    assert_eq!(asked.status, 200, "{}", asked.body);
    assert_eq!(asked.body["requested"], true);
    assert_eq!(asked.body["trip"], "kyoto");

    let feed = h.bearer(Method::Get, "/events?since=0", None, &token).await;
    let events = feed.body["events"].as_array().unwrap();
    let play = events
        .iter()
        .find(|e| e["type"] == "play.requested")
        .expect("play.requested on the feed");
    assert_eq!(play["trip"], "kyoto");
    assert_eq!(play["source"], "alexa");
    assert!(play["playlist"].is_null());
    assert!(play.get("start").is_none(), "no start when none was asked");

    // "Play this photo on the TV" from the phone: the event carries where to open.
    let at_photo = h
        .bearer(
            Method::Post,
            "/trips/kyoto/play",
            Some(json!({ "start": "m2" })),
            &token,
        )
        .await;
    assert_eq!(at_photo.status, 200, "{}", at_photo.body);
    assert_eq!(at_photo.body["start"], "m2");
    let feed = h.bearer(Method::Get, "/events?since=0", None, &token).await;
    let last = feed.body["events"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["type"], "play.requested");
    assert_eq!(last["start"], "m2");
    assert_eq!(last["source"], "phone");
    // A photo that is not in the trip is refused, and nothing is emitted.
    let elsewhere = h
        .bearer(
            Method::Post,
            "/trips/kyoto/play",
            Some(json!({ "start": "zzz" })),
            &token,
        )
        .await;
    assert_eq!(elsewhere.status, 400, "{}", elsewhere.body);
    let feed = h.bearer(Method::Get, "/events?since=0", None, &token).await;
    let owner_events = feed.body["events"].as_array().unwrap().len();
    assert_eq!(owner_events, 2);

    // A member household plays a joined trip on its own TVs (F-162): the event lands on
    // the member's feed, not the owner's.
    let ruth = h
        .member_household("amzn-ruth", "Ruth's living room", "kyoto")
        .await;
    let by_member = h
        .bearer(
            Method::Post,
            "/trips/kyoto/play",
            Some(json!({ "start": "m1" })),
            &ruth,
        )
        .await;
    assert_eq!(by_member.status, 200, "{}", by_member.body);
    let ruth_feed = h.bearer(Method::Get, "/events?since=0", None, &ruth).await;
    let ruth_last = ruth_feed.body["events"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(ruth_last["type"], "play.requested");
    assert_eq!(ruth_last["trip"], "kyoto");
    assert_eq!(ruth_last["start"], "m1");
    let feed = h.bearer(Method::Get, "/events?since=0", None, &token).await;
    assert_eq!(feed.body["events"].as_array().unwrap().len(), owner_events);

    // Playlists play the same way, with an empty body allowed.
    let created = h
        .bearer(
            Method::Post,
            "/playlists",
            Some(json!({ "name": "Beach days", "media": ["m1"], "source": "search" })),
            &token,
        )
        .await;
    assert_eq!(created.status, 201, "{}", created.body);
    let id = created.body["id"].as_str().unwrap().to_owned();
    let asked = h
        .bearer(Method::Post, &format!("/playlists/{id}/play"), None, &token)
        .await;
    assert_eq!(asked.status, 200, "{}", asked.body);
    let feed = h.bearer(Method::Get, "/events?since=0", None, &token).await;
    let last = feed.body["events"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["type"], "play.requested");
    assert_eq!(last["playlist"], id);
    assert_eq!(last["name"], "Beach days");
    assert_eq!(last["source"], "agent");
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
    kyoto.contributors = vec![mantelpiece_core::ids::ContributorId::new("amzn-owner")];
    h.storage.put_trip(&kyoto).await.unwrap();
    h.storage
        .put_contributor(&mantelpiece_core::contributor::Contributor::owner(
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
    kyoto.days = vec![mantelpiece_core::trip::TripDay {
        landmarks: vec![],
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
        .put_playlist(&mantelpiece_core::trip::Playlist {
            id: mantelpiece_core::ids::PlaylistId::new("p1"),
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
            &mantelpiece_core::ids::PlaylistId::new("p1"),
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
    assert_eq!(none.body["events"], json!([]));
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
    assert_eq!(
        h.bearer(Method::Get, "/uploads/mine", None, &other)
            .await
            .body,
        json!([])
    );
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
    let expired = h.app.signer.sign(&mantelpiece_api::tokens::Claims {
        session: mantelpiece_api::tokens::Session::Tv {
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
    use mantelpiece_api::app::TripAccess;
    use mantelpiece_core::contributor::Role;
    use mantelpiece_core::ids::ContributorId;
    use mantelpiece_core::membership::Membership;

    impl Harness {
        /// A friend with an Amazon account joins by household code and is promoted to
        /// co-editor. Returns their upload token and contributor id.
        pub(crate) async fn co_editor(
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
        pub(crate) async fn member_household(
            &self,
            household: &str,
            name: &str,
            trip: &str,
        ) -> String {
            let m = Membership::join(
                HouseholdId::new(household),
                HouseholdId::new("amzn-owner"),
                TripId::new(trip),
                name,
                self.app.now(),
            )
            .unwrap();
            self.storage.put_membership(&m).await.unwrap();
            self.app.signer.sign(&mantelpiece_api::tokens::Claims {
                session: mantelpiece_api::tokens::Session::Phone {
                    household: HouseholdId::new(household),
                    subject: household.into(),
                },
                exp: self.app.now() + Duration::days(1),
            })
        }

        pub(crate) async fn access(
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

#[tokio::test]
async fn a_phone_pairs_with_the_tv_by_name_and_waits_for_approval() {
    let h = harness();
    // First launch: the TV names its household, no account anywhere.
    let setup = h
        .call(
            Method::Post,
            "/tv/setup",
            Some(json!({ "device_id": "tv-1", "name": "  Living  room " })),
            None,
            None,
        )
        .await;
    assert_eq!(setup.status, 201, "{:?}", setup.body);
    assert_eq!(setup.body["household_name"], "Living room");
    let tv = setup.body["token"].as_str().unwrap().to_owned();

    let issued = h
        .call(
            Method::Post,
            "/pair/code",
            Some(json!({ "device_id": "tv-1" })),
            Some(&tv),
            None,
        )
        .await;
    assert_eq!(issued.status, 201, "{:?}", issued.body);
    let code = issued.body["code"].as_str().unwrap().to_owned();
    assert!(issued.body["link"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/?p={code}")));

    // The phone sees which TV it is about to join, then asks with a name.
    let seen = h
        .call(Method::Get, &format!("/pair/code/{code}"), None, None, None)
        .await;
    assert_eq!(seen.body["tv_name"], "Living room");
    // The household comes too, so a phone paired with another TV knows to ask again.
    assert!(seen.body["household"]
        .as_str()
        .is_some_and(|h| !h.is_empty()));
    let nameless = h
        .call(
            Method::Post,
            "/pair/requests",
            Some(json!({ "code": code, "name": "  " })),
            None,
            None,
        )
        .await;
    assert_eq!(nameless.status, 400);
    let asked = h
        .call(
            Method::Post,
            "/pair/requests",
            Some(json!({ "code": code, "name": "Maya" })),
            None,
            None,
        )
        .await;
    assert_eq!(asked.status, 201, "{:?}", asked.body);
    let waiting = asked.body["token"].as_str().unwrap().to_owned();
    let request = asked.body["request"].as_str().unwrap().to_owned();

    // Waiting: no access to anything, and the status says pending.
    assert_eq!(
        h.call(Method::Get, "/trips", None, Some(&waiting), None)
            .await
            .status,
        401
    );
    let pending = h
        .call(Method::Get, "/pair/status", None, Some(&waiting), None)
        .await;
    assert_eq!(pending.body["state"], "pending");

    // The TV hears about it and sees the queue.
    let feed = h
        .call(Method::Get, "/events?since=0", None, Some(&tv), None)
        .await;
    assert!(
        feed.body.to_string().contains("phone.requested"),
        "{:?}",
        feed.body
    );
    let queue = h
        .call(Method::Get, "/pair/requests", None, Some(&tv), None)
        .await;
    assert_eq!(queue.body["pending"][0]["name"], "Maya");
    assert_eq!(queue.body["phones"].as_array().unwrap().len(), 0);

    // Only the TV approves.
    let path = format!("/pair/requests/{request}/approve");
    assert_eq!(
        h.call(Method::Post, &path, None, Some(&waiting), None)
            .await
            .status,
        403
    );
    assert_eq!(
        h.call(Method::Post, &path, None, Some(&tv), None)
            .await
            .status,
        200
    );

    let approved = h
        .call(Method::Get, "/pair/status", None, Some(&waiting), None)
        .await;
    assert_eq!(approved.body["state"], "approved");
    assert_eq!(approved.body["tv_name"], "Living room");
    let phone = approved.body["token"].as_str().unwrap().to_owned();

    // The phone now acts for the household under its own name.
    let me = h.call(Method::Get, "/me", None, Some(&phone), None).await;
    assert_eq!(me.body["display_name"], "Maya");
    assert_eq!(me.body["household_name"], "Living room");
    assert_eq!(
        h.call(Method::Get, "/trips", None, Some(&phone), None)
            .await
            .status,
        200
    );
    let init = h
        .call(
            Method::Post,
            "/uploads/init",
            Some(json!({ "filename": "a.jpg", "bytes": 10, "sha256": "ab".repeat(32), "content_type": "image/jpeg" })),
            Some(&phone),
            None,
        )
        .await;
    assert!(init.status < 300, "{:?}", init.body);

    // Grouping faces is the household's call: the phone may look, only the TV switches.
    for path in ["/people/enable", "/people/disable"] {
        let refused = h.call(Method::Post, path, None, Some(&phone), None).await;
        assert_eq!(refused.status, 403, "{:?}", refused.body);
    }
    assert_eq!(
        h.call(Method::Get, "/people", None, Some(&phone), None)
            .await
            .status,
        200
    );
    let on = h
        .call(Method::Post, "/people/enable", None, Some(&tv), None)
        .await;
    assert_eq!(on.body["people_enabled"], true, "{:?}", on.body);

    // Phones never rename the household, approve other phones, or delete everything.
    assert_eq!(
        h.call(
            Method::Patch,
            "/household",
            Some(json!({ "name": "Mine" })),
            Some(&phone),
            None
        )
        .await
        .status,
        403
    );
    assert_eq!(
        h.call(Method::Delete, "/me", None, Some(&phone), None)
            .await
            .status,
        403
    );
    let renamed = h
        .call(
            Method::Patch,
            "/household",
            Some(json!({ "name": "Den" })),
            Some(&tv),
            None,
        )
        .await;
    assert_eq!(renamed.body["household_name"], "Den");

    // Forgotten on the TV: the phone's session stops working at once.
    let queue = h
        .call(Method::Get, "/pair/requests", None, Some(&tv), None)
        .await;
    assert_eq!(queue.body["phones"][0]["name"], "Maya");
    let deny = format!("/pair/requests/{request}/deny");
    assert_eq!(
        h.call(Method::Post, &deny, None, Some(&tv), None)
            .await
            .status,
        200
    );
    assert_eq!(
        h.call(Method::Get, "/trips", None, Some(&phone), None)
            .await
            .status,
        401
    );
    let after = h
        .call(Method::Get, "/pair/status", None, Some(&waiting), None)
        .await;
    assert_eq!(after.body["state"], "denied");

    // An expired code lets nobody ask.
    h.advance(Duration::minutes(11));
    let late = h
        .call(
            Method::Post,
            "/pair/requests",
            Some(json!({ "code": code, "name": "Ben" })),
            None,
            None,
        )
        .await;
    assert_eq!(late.status, 404);
}

/// An access token as Cognito would mint it. Unsigned: the Gateway checks signatures, the
/// interceptor reads the claims.
fn access_token(
    h: &Harness,
    sub: &str,
    client: &str,
    issuer: Option<&str>,
    lifetime: i64,
) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let claims = json!({
        "iss": issuer.unwrap_or(&h.app.config.oauth_issuer), "sub": sub, "client_id": client,
        "token_use": "access", "exp": h.app.now().unix_timestamp() + lifetime,
    });
    format!(
        "{}.{}.sig",
        URL_SAFE_NO_PAD.encode(b"{}"),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    )
}

/// One tool call the way the Gateway makes it: through the interceptor, then the target.
async fn via_gateway(
    h: &Harness,
    bearer: &str,
    tool: &str,
    args: Value,
) -> Result<Value, mantelpiece_api::ApiError> {
    use mantelpiece_api::tools;
    let event = json!({ "interceptorInputVersion": "1.0", "mcp": { "gatewayRequest": {
        "path": "/mcp", "httpMethod": "POST", "headers": { "Authorization": format!("Bearer {bearer}") },
        "body": { "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": tool, "arguments": args } },
    } } });
    let out = tools::intercept(&h.app, &event);
    assert_eq!(out["interceptorOutputVersion"], "1.0");
    if let Some(refused) = out["mcp"].get("transformedGatewayResponse") {
        return Err(mantelpiece_api::ApiError::unauthorized(
            refused["body"]["error"]["message"]
                .as_str()
                .unwrap_or_default(),
        ));
    }
    let sealed = &out["mcp"]["transformedGatewayRequest"]["body"]["params"];
    tools::call(
        &h.app,
        sealed["name"].as_str().unwrap(),
        &sealed["arguments"],
    )
    .await
}

#[tokio::test]
async fn gateway_tools_verify_who_is_calling() {
    use mantelpiece_api::tools;
    let h = harness();
    let agent = h.app.config.agent_client_id.clone();
    h.storage
        .put_trip(&trip("amzn-owner", "kyoto"))
        .await
        .unwrap();
    h.storage
        .put_trip(&trip("someone-else", "rome"))
        .await
        .unwrap();

    // A signed-in person acts for their own household, whatever the arguments claim.
    let person = access_token(&h, "amzn-owner", "alexa-client", None, 3600);
    let mine = via_gateway(
        &h,
        &person,
        "api___list_trips",
        json!({ "household": "someone-else" }),
    )
    .await
    .unwrap();
    assert_eq!(
        mine.as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["kyoto"]
    );
    let theirs = via_gateway(
        &h,
        &person,
        "api___get_trip",
        json!({ "household": "someone-else", "trip_id": "rome" }),
    )
    .await;
    assert_eq!(theirs.unwrap_err().status, 404);

    // The Concierge's client acts for the household an event names.
    let concierge = access_token(&h, &agent, &agent, None, 3600);
    let named = via_gateway(
        &h,
        &concierge,
        "api___list_trips",
        json!({ "household": "someone-else" }),
    )
    .await
    .unwrap();
    assert_eq!(named[0]["id"], "rome");
    assert_eq!(
        via_gateway(&h, &concierge, "api___list_trips", json!({}))
            .await
            .unwrap_err()
            .status,
        400
    );

    // Refused at the interceptor: another issuer, an expired token, an id token, no header.
    for bad in [
        access_token(
            &h,
            "amzn-owner",
            "alexa-client",
            Some("https://evil.example"),
            3600,
        ),
        access_token(&h, "amzn-owner", "alexa-client", None, -5),
        "not-a-jwt".to_owned(),
    ] {
        let refused = via_gateway(&h, &bad, "api___list_trips", json!({}))
            .await
            .unwrap_err();
        assert!(
            refused.to_string().contains("caller not accepted"),
            "{refused}"
        );
    }
    let headerless = json!({ "interceptorInputVersion": "1.0", "mcp": { "gatewayRequest": { "body": { "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "api___list_trips", "arguments": {} } } } } });
    assert!(
        tools::intercept(&h.app, &headerless)["mcp"]["transformedGatewayResponse"]["body"]["error"]
            ["message"]
            .as_str()
            .unwrap()
            .contains("no authorization header")
    );

    // Anything but a tool call passes through untouched.
    let listing = json!({ "interceptorInputVersion": "1.0", "mcp": { "gatewayRequest": { "body": { "jsonrpc": "2.0", "id": 2, "method": "tools/list" } } } });
    assert_eq!(
        tools::intercept(&h.app, &listing)["mcp"]["transformedGatewayRequest"]["body"]["method"],
        "tools/list"
    );

    // Straight to the target, the seal is everything: absent, forged, for another tool, stale.
    assert_eq!(
        tools::call(
            &h.app,
            "api___list_trips",
            &json!({ "household": "amzn-owner" })
        )
        .await
        .unwrap_err()
        .status,
        401
    );
    let forged =
        mantelpiece_api::tokens::Signer::new("someone-elses-key").seal(&tools::ToolCaller {
            kind: tools::CallerKind::Agent,
            subject: agent.clone(),
            tool: "api___list_trips".into(),
            exp: h.app.now() + Duration::minutes(1),
        });
    assert_eq!(
        tools::call(
            &h.app,
            "api___list_trips",
            &json!({ "household": "amzn-owner", "_caller": forged })
        )
        .await
        .unwrap_err()
        .status,
        401
    );
    let for_listing = h.app.signer.seal(&tools::ToolCaller {
        kind: tools::CallerKind::Agent,
        subject: agent.clone(),
        tool: "api___list_trips".into(),
        exp: h.app.now() + Duration::minutes(1),
    });
    let reused = tools::call(
        &h.app,
        "api___purge_expired",
        &json!({ "household": "amzn-owner", "_caller": for_listing }),
    )
    .await;
    assert_eq!(reused.unwrap_err().status, 401);
    h.advance(Duration::minutes(2));
    let stale = tools::call(
        &h.app,
        "api___list_trips",
        &json!({ "household": "amzn-owner", "_caller": for_listing }),
    )
    .await;
    assert_eq!(stale.unwrap_err().status, 401);
}

#[tokio::test]
async fn gateway_tools_run_the_same_routes_for_the_household_they_name() {
    use mantelpiece_api::tools;
    let h = harness();
    let agent = h.app.config.agent_client_id.clone();
    let bearer = access_token(&h, &agent, &agent, None, 3600);
    let setup = h
        .call(
            Method::Post,
            "/tv/setup",
            Some(json!({ "device_id": "tv-1", "name": "Den" })),
            None,
            None,
        )
        .await;
    let household = setup.body["household"].as_str().unwrap().to_owned();
    let mut seeded_trip = trip(&household, "kyoto");
    seeded_trip.days = vec![mantelpiece_core::trip::TripDay {
        index: 1,
        date: time::macros::date!(2026 - 04 - 03),
        media: vec![],
        route: Route::default(),
        title: None,
        story: None,
        undated: vec![],
        landmarks: vec![],
    }];
    h.storage.put_trip(&seeded_trip).await.unwrap();
    let trip = "kyoto".to_owned();

    assert_eq!(tools::tool_of("api___get_trip"), "get_trip");
    let me = via_gateway(
        &h,
        &bearer,
        "api___get_household",
        json!({ "household": household }),
    )
    .await
    .unwrap();
    assert_eq!(me["household_name"], "Den");
    let trips = via_gateway(
        &h,
        &bearer,
        "api___list_trips",
        json!({ "household": household }),
    )
    .await
    .unwrap();
    assert_eq!(trips.as_array().unwrap().len(), 1);
    let got = via_gateway(
        &h,
        &bearer,
        "api___get_trip",
        json!({ "household": household, "trip_id": trip }),
    )
    .await
    .unwrap();
    assert_eq!(got["id"], trip.as_str());
    assert!(got["days"][0]["date"].is_string(), "{got}");

    // Another household's id opens nothing of this one.
    let other = via_gateway(
        &h,
        &bearer,
        "api___get_trip",
        json!({ "household": "someone-else", "trip_id": trip }),
    )
    .await;
    assert_eq!(other.unwrap_err().status, 404);

    // The story is saved through the same fact check as the route.
    let saved = via_gateway(&h, &bearer, "api___save_narrative", json!({ "household": household, "trip_id": trip, "text": "A quiet trip.", "model_id": "test", "stories": ["A quiet trip."] }),
    )
    .await
    .unwrap();
    assert_eq!(saved["check"], "passed", "{saved}");
    let withheld = via_gateway(
        &h,
        &bearer,
        "api___save_narrative",
        json!({ "household": household, "trip_id": trip, "model_id": "test", "withhold": true }),
    )
    .await
    .unwrap();
    assert_eq!(withheld["check"], "withheld");

    // Events arrive as JSON text from an agent, and land on the feed.
    let pushed = via_gateway(&h, &bearer, "api___push_event", json!({ "household": household, "event": format!("{{\"type\":\"memory.ready\",\"trip\":\"{trip}\"}}") }),
    )
    .await
    .unwrap();
    assert!(pushed["seq"].as_u64().unwrap() > 0);

    // Irreversible tools still need the TV's token; unknown and unbuilt tools say so.
    let denied = via_gateway(
        &h,
        &bearer,
        "api___apply_cleanup",
        json!({ "household": household, "media_ids": ["m1"], "confirmation": "made-up" }),
    )
    .await;
    assert_eq!(denied.unwrap_err().status, 403);
    assert_eq!(
        via_gateway(
            &h,
            &bearer,
            "api___read_memory",
            json!({ "household": household })
        )
        .await
        .unwrap_err()
        .status,
        501
    );
    assert_eq!(
        via_gateway(&h, &bearer, "api___nope", json!({ "household": household }))
            .await
            .unwrap_err()
            .status,
        404
    );
    assert_eq!(
        via_gateway(&h, &bearer, "api___list_trips", json!({}))
            .await
            .unwrap_err()
            .status,
        400
    );
}

#[tokio::test]
async fn landmark_facts_reach_the_trip_and_trivia_must_stay_inside_the_summary() {
    let h = harness();
    let house = HouseholdId::new("amzn-owner");
    let gold = enriched(&h, "gold.jpg", 71, &["Temple"], None).await;
    let plain = enriched(&h, "plain.jpg", 72, &["Street"], None).await;
    let mut kyoto = trip("amzn-owner", "kyoto");
    let day = mantelpiece_core::trip::TripDay {
        index: 1,
        date: time::macros::date!(2026 - 04 - 04),
        media: vec![gold.clone(), plain.clone()],
        route: Route::default(),
        title: Some("Day 1: Kinkaku-ji".into()),
        story: None,
        undated: vec![],
        landmarks: vec![],
    };
    kyoto.days = vec![day];
    h.storage.put_trip(&kyoto).await.unwrap();
    let maya =
        mantelpiece_core::person::Person::new(house.clone(), "f1", gold.clone(), h.app.now());
    h.storage.put_person(&maya).await.unwrap();
    for id in [&gold, &plain] {
        let mut m = h.storage.get_media(&house, id).await.unwrap().unwrap();
        m.trip = Some(TripId::new("kyoto"));
        if id == &gold {
            m.enrichment.landmark = Some(mantelpiece_core::media::Tag {
                name: "Kinkaku-ji".into(),
                confidence: 0.85,
            });
            m.people = vec![maya.id.clone()];
        }
        h.storage.put_media(&m).await.unwrap();
    }
    h.storage
        .put_landmark_facts(&mantelpiece_core::landmark::LandmarkFacts {
            key: "kinkaku-ji".into(),
            name: "Kinkaku-ji".into(),
            title: "Kinkaku-ji".into(),
            tagline: Some("Buddhist temple in Kyoto, Japan".into()),
            summary: "Kinkaku-ji is a Zen Buddhist temple in Kyoto, Japan. The top two floors of the pavilion are covered in gold leaf. The pavilion was burned down in 1950 by a monk and rebuilt in 1955.".into(),
            url: "https://en.wikipedia.org/wiki/Kinkaku-ji".into(),
            credit: "Wikipedia, CC BY-SA 4.0".into(),
            at: None,
            trivia: vec![],
            fetched_at: h.app.now(),
        })
        .await
        .unwrap();

    // The trip carries facts per landmark, and per photo the landmark and people in it.
    let detail = h.owner(Method::Get, "/trips/kyoto", None).await;
    assert_eq!(detail.status, 200, "{}", detail.body);
    let facts = &detail.body["landmark_facts"]["kinkaku-ji"];
    assert_eq!(facts["credit"], "Wikipedia, CC BY-SA 4.0");
    assert!(facts["lines"][0]
        .as_str()
        .unwrap()
        .starts_with("Kinkaku-ji is a Zen Buddhist temple"));
    let tags = &detail.body["media_tags"];
    assert_eq!(tags[gold.as_str()]["landmark"], "kinkaku-ji");
    assert_eq!(tags[gold.as_str()]["people"][0], maya.id.as_str());
    assert!(tags[plain.as_str()].is_null());
    assert_eq!(
        detail.body["days"][0]["landmark_keys"],
        json!(["kinkaku-ji"])
    );
    assert_eq!(detail.body["people"][0]["id"], maya.id.as_str());
    assert_eq!(detail.body["people"][0]["media_count"], 1);

    // Trivia the summary backs is kept; trivia that invents is refused and not stored.
    let invented = h
        .owner(
            Method::Post,
            "/landmarks/kinkaku-ji/trivia",
            Some(json!({ "lines": ["Legend says Mishima hid a sword here in 1936."] })),
        )
        .await;
    assert_eq!(invented.status, 422, "{}", invented.body);
    let kept = h
        .owner(
            Method::Post,
            "/landmarks/Kinkaku-ji/trivia",
            Some(json!({ "lines": [
                "The top two floors are covered in gold leaf.",
                "A monk burned it down in 1950; it was rebuilt in 1955."
            ] })),
        )
        .await;
    assert_eq!(kept.status, 200, "{}", kept.body);
    let got = h.owner(Method::Get, "/landmarks/kinkaku-ji", None).await;
    assert_eq!(got.body["has_trivia"], true);
    assert_eq!(got.body["lines"].as_array().unwrap().len(), 2);

    // A household only reads landmarks it has been to.
    assert_eq!(
        h.owner(Method::Get, "/landmarks/eiffel-tower", None)
            .await
            .status,
        404
    );

    // The same through the Gateway: Alexa+ asks by name, the Concierge writes trivia.
    let person = access_token(&h, "amzn-owner", "alexa-client", None, 3600);
    let asked = via_gateway(
        &h,
        &person,
        "api___get_landmark_facts",
        json!({ "landmark": "Kinkaku-ji" }),
    )
    .await
    .unwrap();
    assert_eq!(asked["title"], "Kinkaku-ji");
    let agent = h.app.config.agent_client_id.clone();
    let concierge = access_token(&h, &agent, &agent, None, 3600);
    let refused = via_gateway(
        &h,
        &concierge,
        "api___save_landmark_trivia",
        json!({ "household": "amzn-owner", "landmark": "kinkaku-ji", "lines": ["Built by aliens in 1234."] }),
    )
    .await
    .unwrap();
    assert!(
        refused["error"].as_str().unwrap().contains("1234"),
        "the agent gets the reasons: {refused}"
    );
}

/// A TV named `tv_name` with one approved phone called `phone_name`. Returns
/// (tv token, phone token, phone's contributor id).
async fn paired(h: &Harness, tv_name: &str, phone_name: &str) -> (String, String, String) {
    let setup = h
        .call(
            Method::Post,
            "/tv/setup",
            Some(json!({ "device_id": "tv-1", "name": tv_name })),
            None,
            None,
        )
        .await;
    let tv = setup.body["token"].as_str().unwrap().to_owned();
    let issued = h
        .call(
            Method::Post,
            "/pair/code",
            Some(json!({ "device_id": "tv-1" })),
            Some(&tv),
            None,
        )
        .await;
    let code = issued.body["code"].as_str().unwrap().to_owned();
    let asked = h
        .call(
            Method::Post,
            "/pair/requests",
            Some(json!({ "code": code, "name": phone_name })),
            None,
            None,
        )
        .await;
    let waiting = asked.body["token"].as_str().unwrap().to_owned();
    let request = asked.body["request"].as_str().unwrap().to_owned();
    let path = format!("/pair/requests/{request}/approve");
    assert_eq!(
        h.call(Method::Post, &path, None, Some(&tv), None)
            .await
            .status,
        200
    );
    let approved = h
        .call(Method::Get, "/pair/status", None, Some(&waiting), None)
        .await;
    let phone = approved.body["token"].as_str().unwrap().to_owned();
    (tv, phone, request)
}

#[tokio::test]
async fn the_tv_loves_an_arrival_and_the_senders_phone_hears_about_it() {
    let h = harness();
    let (tv, phone, maya) = paired(&h, "Grandma's TV", "Maya").await;

    // The phone turns notifications on with its browser subscription.
    let key = h.call(Method::Get, "/push/key", None, None, None).await;
    assert_eq!(key.status, 200);
    assert!(key.body["key"].as_str().is_some_and(|k| !k.is_empty()));
    let sub = json!({ "endpoint": "https://push.example.com/abc", "keys": { "p256dh": "BPk", "auth": "au" } });
    let insecure = json!({ "endpoint": "http://push.example.com/abc", "keys": { "p256dh": "BPk", "auth": "au" } });
    assert_eq!(
        h.bearer(Method::Put, "/push/subscription", Some(insecure), &phone)
            .await
            .status,
        400
    );
    assert_eq!(
        h.bearer(Method::Put, "/push/subscription", Some(sub.clone()), &phone)
            .await
            .status,
        200
    );
    // The TV has no inbox and no push; the phone cannot love on the TV's behalf.
    assert_eq!(
        h.bearer(Method::Put, "/push/subscription", Some(sub), &tv)
            .await
            .status,
        403
    );
    let love = json!({ "contributor": maya, "count": 14, "cover": "m1" });
    assert_eq!(
        h.bearer(Method::Post, "/arrivals/love", Some(love.clone()), &phone)
            .await
            .status,
        403
    );

    // One button on the TV: the note lands and the push goes out.
    let loved = h
        .bearer(Method::Post, "/arrivals/love", Some(love.clone()), &tv)
        .await;
    assert_eq!(loved.status, 200, "{:?}", loved.body);
    assert_eq!(loved.body["pushed"], true);
    {
        let sent = h.push.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0.endpoint, "https://push.example.com/abc");
        assert_eq!(sent[0].1.title, "Grandma's TV");
        assert_eq!(sent[0].1.body, "loved your 14 photos");
        assert!(sent[0].1.url.ends_with("/?inbox=1"));
    }
    let inbox = h.bearer(Method::Get, "/inbox", None, &phone).await;
    assert_eq!(inbox.status, 200);
    assert_eq!(inbox.body["push"], true);
    assert_eq!(inbox.body["notes"][0]["from"], "Grandma's TV");
    assert_eq!(inbox.body["notes"][0]["count"], 14);
    assert_eq!(inbox.body["notes"][0]["cover"], "m1");

    // A subscription the push service forgot is dropped; the note still arrives.
    h.push
        .gone
        .lock()
        .unwrap()
        .push("https://push.example.com/abc".into());
    let again = h
        .bearer(Method::Post, "/arrivals/love", Some(love), &tv)
        .await;
    assert_eq!(again.body["pushed"], false);
    let inbox = h.bearer(Method::Get, "/inbox", None, &phone).await;
    assert_eq!(inbox.body["push"], false);
    assert_eq!(inbox.body["notes"].as_array().unwrap().len(), 2);

    // No session, no inbox.
    assert_eq!(
        h.call(Method::Get, "/inbox", None, None, None).await.status,
        401
    );
}

/// Another phone on the same TV, paired and approved.
async fn another_phone(h: &Harness, tv: &str, name: &str) -> String {
    let asked = h
        .bearer(
            Method::Post,
            "/pair/code",
            Some(json!({ "device_id": "tv-1" })),
            tv,
        )
        .await;
    let code = asked.body["code"].as_str().unwrap().to_owned();
    let req = h
        .call(
            Method::Post,
            "/pair/requests",
            Some(json!({ "code": code, "name": name })),
            None,
            None,
        )
        .await;
    let waiting = req.body["token"].as_str().unwrap().to_owned();
    let id = req.body["request"].as_str().unwrap().to_owned();
    h.bearer(
        Method::Post,
        &format!("/pair/requests/{id}/approve"),
        None,
        tv,
    )
    .await;
    let ok = h.bearer(Method::Get, "/pair/status", None, &waiting).await;
    ok.body["token"].as_str().unwrap().to_owned()
}

/// The phone asks, the TV grants: the phone's answer, with the stream key.
async fn go_live(h: &Harness, tv: &str, phone: &str) -> Value {
    let asked = h.bearer(Method::Post, "/live", None, phone).await;
    assert_eq!(asked.status, 202, "{:?}", asked.body);
    let id = asked.body["request"].as_str().unwrap().to_owned();
    let granted = h
        .bearer(
            Method::Post,
            &format!("/live/requests/{id}/grant"),
            None,
            tv,
        )
        .await;
    assert_eq!(granted.status, 200, "{:?}", granted.body);
    h.bearer(Method::Get, "/live/request", None, phone)
        .await
        .body
}

#[tokio::test]
async fn a_phone_asks_the_tv_grants_and_the_channel_goes_when_it_ends() {
    let h = harness();
    let (tv, phone, _) = paired(&h, "Grandma's TV", "Maya").await;
    assert_eq!(h.bearer(Method::Post, "/live", None, &tv).await.status, 403);

    // Asking makes no channel and tells the TV who wants in.
    let asked = h.bearer(Method::Post, "/live", None, &phone).await;
    assert_eq!(asked.status, 202, "{:?}", asked.body);
    assert_eq!(asked.body["status"], "waiting");
    assert_eq!(asked.body["tv"], "Grandma's TV");
    assert!(h.live.channels.lock().unwrap().is_empty());
    let id = asked.body["request"].as_str().unwrap().to_owned();
    let feed = h.bearer(Method::Get, "/events?since=0", None, &tv).await;
    assert!(feed.body.to_string().contains("live.requested"));
    let now = h.bearer(Method::Get, "/live", None, &tv).await;
    assert_eq!(now.body["live"], Value::Null);
    assert_eq!(now.body["requests"][0]["display_name"], "Maya");
    let mine = h.bearer(Method::Get, "/live/request", None, &phone).await;
    assert_eq!(mine.body["status"], "waiting");
    assert!(mine.body.get("stream_key").is_none());

    // Only the TV answers.
    let grant = format!("/live/requests/{id}/grant");
    assert_eq!(
        h.bearer(Method::Post, &grant, None, &phone).await.status,
        403
    );
    assert_eq!(h.bearer(Method::Post, &grant, None, &tv).await.status, 200);
    let mine = h.bearer(Method::Get, "/live/request", None, &phone).await;
    assert_eq!(mine.body["status"], "granted");
    assert!(mine.body["stream_key"]
        .as_str()
        .is_some_and(|k| k.starts_with("sk_")));
    assert!(mine.body["ingest_endpoint"].as_str().is_some());
    // A second grant of the same request does nothing.
    assert_eq!(h.bearer(Method::Post, &grant, None, &tv).await.status, 409);
    assert_eq!(h.live.channels.lock().unwrap().len(), 1);
    let first_arn = h
        .live
        .channels
        .lock()
        .unwrap()
        .keys()
        .next()
        .cloned()
        .unwrap();

    // The TV learns where to watch and never sees the stream key.
    let feed = h.bearer(Method::Get, "/events?since=0", None, &tv).await;
    let text = feed.body.to_string();
    assert!(text.contains("live.started"), "{text}");
    assert!(text.contains("live.request_closed"), "{text}");
    assert!(!text.contains("sk_"), "{text}");
    let now = h.bearer(Method::Get, "/live", None, &tv).await;
    assert_eq!(now.body["live"]["display_name"], "Maya");
    assert_eq!(now.body["requests"], json!([]));
    assert!(!now.body.to_string().contains("sk_"));
    // Asking again while live is refused.
    assert_eq!(
        h.bearer(Method::Post, "/live", None, &phone).await.status,
        409
    );

    // The TV ends it; IVS refuses the delete for now, so the channel waits.
    h.live.busy.lock().unwrap().push(first_arn.clone());
    let stopped = h.bearer(Method::Post, "/live/stop", None, &tv).await;
    assert_eq!(stopped.body["ended"], true);
    assert!(h.live.stopped.lock().unwrap().contains(&first_arn));
    assert!(h.live.channels.lock().unwrap().contains_key(&first_arn));
    assert!(h
        .bearer(Method::Get, "/events?since=0", None, &tv)
        .await
        .body
        .to_string()
        .contains("live.ended"));
    // The key goes with the broadcast.
    let mine = h.bearer(Method::Get, "/live/request", None, &phone).await;
    assert_eq!(mine.body["status"], "ended");
    assert!(mine.body.get("stream_key").is_none());

    // Going live again sweeps the old channel and makes a fresh one.
    let again = go_live(&h, &tv, &phone).await;
    assert_eq!(again["status"], "granted");
    {
        let channels = h.live.channels.lock().unwrap();
        assert!(!channels.contains_key(&first_arn));
        assert_eq!(channels.len(), 1);
    }
    // Only the live phone or the TV may end it.
    let ben = another_phone(&h, &tv, "Ben").await;
    assert_eq!(
        h.bearer(Method::Post, "/live/stop", None, &ben)
            .await
            .status,
        403
    );
    assert_eq!(
        h.bearer(Method::Post, "/live/stop", None, &phone)
            .await
            .body["ended"],
        true
    );
    assert!(h.live.channels.lock().unwrap().is_empty());

    // A broadcast nobody ended stops showing after four hours.
    go_live(&h, &tv, &phone).await;
    h.advance(Duration::hours(5));
    assert_eq!(
        h.bearer(Method::Get, "/live", None, &tv).await.body["live"],
        Value::Null
    );
}

#[tokio::test]
async fn a_phone_that_asks_to_keep_its_broadcast_gets_a_recorded_channel() {
    // No recording configuration: the ask to keep it is answered honestly with no.
    let h = harness();
    let (tv, phone, _) = paired(&h, "Grandma's TV", "Maya").await;
    let asked = h
        .bearer(Method::Post, "/live", Some(json!({ "save": true })), &phone)
        .await;
    assert_eq!(asked.status, 202, "{:?}", asked.body);
    assert_eq!(asked.body["saving"], false);

    let h = harness_with(MemoryLive {
        recording: true,
        ..MemoryLive::default()
    });
    let (tv2, phone2, _) = paired(&h, "Grandma's TV", "Maya").await;
    let asked = h
        .bearer(
            Method::Post,
            "/live",
            Some(json!({ "save": true, "utc_offset_minutes": -420 })),
            &phone2,
        )
        .await;
    assert_eq!(asked.body["saving"], true);
    let now = h.bearer(Method::Get, "/live", None, &tv2).await;
    assert_eq!(now.body["requests"][0]["saving"], true);
    let id = asked.body["request"].as_str().unwrap().to_owned();
    let granted = h
        .bearer(
            Method::Post,
            &format!("/live/requests/{id}/grant"),
            None,
            &tv2,
        )
        .await;
    assert_eq!(granted.body["saving"], true);

    let arn = h.live.recorded.lock().unwrap()[0].clone();
    let claim = h
        .blobs
        .objects
        .lock()
        .unwrap()
        .get(&mantelpiece_core::household::RecordingClaim::key(&arn))
        .cloned()
        .expect("claim written beside the channel");
    let claim: mantelpiece_core::household::RecordingClaim =
        serde_json::from_slice(&claim).unwrap();
    assert_eq!(claim.display_name, "Maya");
    assert_eq!(claim.save.utc_offset_seconds, Some(-420 * 60));
    assert_eq!(
        h.bearer(Method::Get, "/live/request", None, &phone2)
            .await
            .body["saving"],
        true
    );
    assert_eq!(
        h.bearer(Method::Get, "/live", None, &tv2).await.body["live"]["saving"],
        true
    );
    assert!(h
        .bearer(Method::Get, "/events?since=0", None, &tv2)
        .await
        .body
        .to_string()
        .contains("\"saving\":true"));

    // Without asking, nothing is recorded.
    h.bearer(Method::Post, "/live/stop", None, &tv2).await;
    go_live(&h, &tv2, &phone2).await;
    assert_eq!(h.live.recorded.lock().unwrap().len(), 1);
    let _ = (tv, phone);
}

#[tokio::test]
async fn a_second_phone_goes_live_only_when_the_tv_switches_to_it() {
    let h = harness();
    let (tv, maya, _) = paired(&h, "Grandma's TV", "Maya").await;
    let sam = another_phone(&h, &tv, "Sam").await;
    let first = go_live(&h, &tv, &maya).await;
    let first_broadcast = first["broadcast"].as_str().unwrap().to_owned();

    // Sam asks: Maya stays live until the TV decides, and the TV hears who it would end.
    let asked = h.bearer(Method::Post, "/live", None, &sam).await;
    assert_eq!(asked.status, 202);
    assert_eq!(asked.body["replacing"], "Maya");
    let now = h.bearer(Method::Get, "/live", None, &tv).await;
    assert_eq!(now.body["live"]["id"], first_broadcast.as_str());
    assert_eq!(now.body["requests"][0]["display_name"], "Sam");
    assert_eq!(h.live.channels.lock().unwrap().len(), 1);

    // Not now: Sam hears no, Maya carries on.
    let id = asked.body["request"].as_str().unwrap().to_owned();
    let declined = h
        .bearer(
            Method::Post,
            &format!("/live/requests/{id}/decline"),
            None,
            &tv,
        )
        .await;
    assert_eq!(declined.status, 204);
    let answer = h.bearer(Method::Get, "/live/request", None, &sam).await;
    assert_eq!(answer.body["status"], "declined");
    assert_eq!(
        h.bearer(Method::Get, "/live", None, &tv).await.body["live"]["id"],
        first_broadcast.as_str()
    );
    assert_eq!(
        h.bearer(
            Method::Post,
            &format!("/live/requests/{id}/grant"),
            None,
            &tv
        )
        .await
        .status,
        409
    );

    // Sam asks again and the TV switches: Maya's broadcast ends, Sam's begins.
    let second = go_live(&h, &tv, &sam).await;
    assert_eq!(second["status"], "granted");
    let now = h.bearer(Method::Get, "/live", None, &tv).await;
    assert_eq!(now.body["live"]["display_name"], "Sam");
    assert_ne!(now.body["live"]["id"], first_broadcast.as_str());
    assert_eq!(h.live.stopped.lock().unwrap().len(), 1);
    assert_eq!(h.live.channels.lock().unwrap().len(), 1);
    // Maya's phone learns it is off: its key is gone and its broadcast is not the TV's.
    let maya_now = h.bearer(Method::Get, "/live/request", None, &maya).await;
    assert_eq!(maya_now.body["status"], "ended");
    assert!(maya_now.body.get("stream_key").is_none());
    // Maya cannot end Sam's broadcast.
    assert_eq!(
        h.bearer(Method::Post, "/live/stop", None, &maya)
            .await
            .status,
        403
    );
}

#[tokio::test]
async fn an_unanswered_request_lapses_and_a_phone_can_take_it_back() {
    let h = harness();
    let (tv, phone, _) = paired(&h, "Grandma's TV", "Maya").await;
    let asked = h.bearer(Method::Post, "/live", None, &phone).await;
    let id = asked.body["request"].as_str().unwrap().to_owned();
    h.advance(Duration::seconds(61));
    assert_eq!(
        h.bearer(Method::Get, "/live/request", None, &phone)
            .await
            .body["status"],
        "expired"
    );
    assert_eq!(
        h.bearer(Method::Get, "/live", None, &tv).await.body["requests"],
        json!([])
    );
    assert_eq!(
        h.bearer(
            Method::Post,
            &format!("/live/requests/{id}/grant"),
            None,
            &tv
        )
        .await
        .status,
        409
    );
    assert!(h.live.channels.lock().unwrap().is_empty());

    // Asking again replaces the old request; cancelling closes it on the TV.
    let asked = h.bearer(Method::Post, "/live", None, &phone).await;
    let id = asked.body["request"].as_str().unwrap().to_owned();
    assert_eq!(
        h.bearer(Method::Delete, "/live/request", None, &phone)
            .await
            .status,
        204
    );
    assert_eq!(
        h.bearer(Method::Get, "/live/request", None, &phone)
            .await
            .body["status"],
        "cancelled"
    );
    assert_eq!(
        h.bearer(
            Method::Post,
            &format!("/live/requests/{id}/grant"),
            None,
            &tv
        )
        .await
        .status,
        409
    );
    let feed = h.bearer(Method::Get, "/events?since=0", None, &tv).await;
    assert!(feed.body.to_string().contains("live.request_closed"));
}

/// A highlight in Kyoto with a field note: a drawing in the bucket, an original to share.
async fn field_noted(h: &Harness) -> MediaId {
    use mantelpiece_core::field_note::{
        Caption, Certainty, Feature, FeatureKind, FieldNote, Ink, MapMode, NormBox,
    };
    let id = enriched(
        h,
        "IMG_9.jpg",
        9,
        &["Temple"],
        Some(("Kiyomizu-dera", "A wooden stage over the trees.")),
    )
    .await;
    let household = HouseholdId::new("amzn-owner");
    let mut m = h.storage.get_media(&household, &id).await.unwrap().unwrap();
    let at = mantelpiece_core::geo::GeoPoint::new(34.9949, 135.7850);
    m.gps = Some(at);
    m.captured = Some(mantelpiece_core::media::CaptureTime::new(
        datetime!(2026-04-04 05:00:00 UTC),
        Some(time::UtcOffset::from_hms(9, 0, 0).unwrap()),
    ));
    m.trip = Some(TripId::new("kyoto"));
    let drawing = mantelpiece_fieldnotes::drawing::Drawing {
        version: 1,
        aspect: 1.5,
        mode: MapMode::Town,
        ink: Ink::Light,
        fills: vec![],
        strokes: vec![],
        labels: vec![],
        marker: mantelpiece_fieldnotes::drawing::Marker {
            kind: mantelpiece_fieldnotes::drawing::MarkerKind::Dot,
            x: 0.5,
            y: 0.5,
            r: 0.0,
            heading: None,
        },
        title: None,
        scale: None,
        caption: Some("4 April 2026".into()),
        coords: Some("34.99° N, 135.79° E".into()),
        fade: 0.22,
        grain_seed: 1,
        credit: "© OpenStreetMap contributors".into(),
    };
    let (hh, mm) = ("amzn-owner", id.as_str());
    h.blobs
        .put(
            &FieldNote::drawing_key(hh, mm),
            serde_json::to_vec(&drawing).unwrap(),
            "application/json",
        )
        .await
        .unwrap();
    let mut original = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut original, 90)
        .encode_image(&image::RgbImage::from_pixel(
            300,
            200,
            image::Rgb([90, 140, 200]),
        ))
        .unwrap();
    h.blobs
        .put(&m.original_key, original, "image/jpeg")
        .await
        .unwrap();
    m.field_note = Some(FieldNote {
        placement: NormBox::new(0.55, 0.05, 0.4, 0.4).unwrap(),
        mode: MapMode::Town,
        certainty: Certainty::Precise { at },
        ink: Ink::Light,
        caption: Caption {
            place: Some("Kiyomizu-dera".into()),
            date: Some("4 April 2026".into()),
            coords: Some("34.99° N, 135.79° E".into()),
        },
        source: "openstreetmap".into(),
        credit: "© OpenStreetMap contributors".into(),
        drawing_key: FieldNote::drawing_key(hh, mm),
        fingerprint: "fp1".into(),
        made_at: h.app.now(),
        feature: Some(Feature {
            name: "Kiyomizu-dera".into(),
            kind: FeatureKind::Landmark,
            at,
            detail: None,
            country: Some("Japan".into()),
        }),
    });
    h.storage.put_media(&m).await.unwrap();
    let mut kyoto = trip("amzn-owner", "kyoto");
    kyoto.highlights = vec![id.clone()];
    kyoto.contributors = vec![mantelpiece_core::ids::ContributorId::new("amzn-owner")];
    h.storage.put_trip(&kyoto).await.unwrap();
    h.storage
        .put_landmark_facts(&serde_json::from_value(json!({
            "key": mantelpiece_core::landmark::key("Kiyomizu-dera"), "name": "Kiyomizu-dera", "title": "Kiyomizu-dera",
            "tagline": "Buddhist temple in Kyoto", "summary": "Kiyomizu-dera is an independent Buddhist temple in eastern Kyoto.",
            "url": "https://en.wikipedia.org/wiki/Kiyomizu-dera", "credit": "Wikipedia, CC BY-SA 4.0", "at": null, "fetched_at": "2026-04-01T00:00:00Z"
        })).unwrap())
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn field_notes_are_served_in_the_form_each_viewer_may_see() {
    use mantelpiece_core::field_note::FieldNote;
    let h = harness();
    let id = field_noted(&h).await;

    // The owner household sees the note as made.
    let url = h
        .owner(
            Method::Get,
            &format!("/media/{id}/url?rendition=fieldnote"),
            None,
        )
        .await;
    assert_eq!(url.status, 200, "{}", url.body);
    assert!(url.body["url"]
        .as_str()
        .unwrap()
        .ends_with(&FieldNote::layer_key("amzn-owner", id.as_str())));
    let big = h
        .owner(
            Method::Get,
            &format!("/media/{id}/url?rendition=fieldnote_map"),
            None,
        )
        .await;
    assert!(big.body["url"]
        .as_str()
        .unwrap()
        .ends_with(&FieldNote::map_key("amzn-owner", id.as_str())));
    let t = h.owner(Method::Get, "/trips/kyoto", None).await;
    assert_eq!(t.body["field_notes"][id.as_str()]["certainty"], "precise");

    // "This place": when, where, what, and the description, with coordinates for the owner.
    let p = h
        .owner(Method::Get, &format!("/trips/kyoto/places/{id}"), None)
        .await;
    assert_eq!(p.status, 200, "{}", p.body);
    assert_eq!(p.body["taken"], "Saturday 4 April 2026, 14:00");
    assert_eq!(p.body["place"], "Kiyomizu-dera, Japan");
    assert_eq!(p.body["coords"], "34.99° N, 135.79° E");
    assert_eq!(
        p.body["feature"]["summary"],
        "Kiyomizu-dera is an independent Buddhist temple in eastern Kyoto."
    );
    assert_eq!(p.body["feature"]["credit"], "Wikipedia, CC BY-SA 4.0");
    assert_eq!(p.body["description"], "A wooden stage over the trees.");

    // The read-only view gets the public form only.
    let code = h
        .owner(
            Method::Post,
            "/trips/kyoto/code",
            Some(json!({ "expires_in_days": 7 })),
        )
        .await;
    let code = code.body["code"].as_str().unwrap().to_owned();
    let view = h
        .call(Method::Get, &format!("/view?code={code}"), None, None, None)
        .await;
    assert_eq!(
        view.body["trip"]["field_notes"][id.as_str()]["certainty"],
        "approximate"
    );
    let public = h
        .call(
            Method::Get,
            &format!("/media/{id}/url?rendition=fieldnote&code={code}"),
            None,
            None,
            None,
        )
        .await;
    assert!(public.body["url"]
        .as_str()
        .unwrap()
        .ends_with(&FieldNote::public_layer_key("amzn-owner", id.as_str())));

    // A photo without a note has no field-note rendition.
    let plain = enriched(&h, "IMG_10.jpg", 10, &[], None).await;
    assert_eq!(
        h.owner(
            Method::Get,
            &format!("/media/{plain}/url?rendition=fieldnote"),
            None
        )
        .await
        .status,
        404
    );
}

#[tokio::test]
async fn the_owner_can_share_a_field_note_at_full_resolution() {
    use mantelpiece_core::field_note::FieldNote;
    let h = harness();
    let id = field_noted(&h).await;
    let full = h
        .owner(Method::Post, &format!("/media/{id}/fieldnote/full"), None)
        .await;
    assert_eq!(full.status, 200, "{}", full.body);
    let key = FieldNote::full_key("amzn-owner", id.as_str());
    let jpeg = h.blobs.get(&key).await.unwrap();
    let img = image::load_from_memory(&jpeg).unwrap();
    assert_eq!(
        (img.width(), img.height()),
        (300, 200),
        "the original's own size"
    );
    // Made once while the note is unchanged.
    let puts = |h: &Harness| {
        h.blobs
            .log
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.starts_with(&format!("put {key} ")))
            .count()
    };
    assert_eq!(puts(&h), 1);
    h.owner(Method::Post, &format!("/media/{id}/fieldnote/full"), None)
        .await;
    assert_eq!(puts(&h), 1);
}

#[tokio::test]
async fn sketch_maps_are_served_in_the_form_each_viewer_may_see() {
    use mantelpiece_core::sketch::{SketchKeys, Which};
    let h = harness();
    // The trip exists through the usual upload and organize path of other tests; here it is
    // enough that the owner can see "kyoto" and that its maps were drawn.
    let _ = field_noted(&h).await;
    let keys = SketchKeys::new(&HouseholdId::new("amzn-owner"), &TripId::new("kyoto"));
    assert_eq!(
        h.owner(Method::Get, "/trips/kyoto/map", None).await.status,
        404,
        "no map until one is drawn"
    );
    let mut trip = h
        .storage
        .get_trip(&HouseholdId::new("amzn-owner"), &TripId::new("kyoto"))
        .await
        .unwrap()
        .unwrap();
    trip.days.push(
        serde_json::from_value(json!({
            "index": 1, "date": "2026-04-04", "media": [], "title": "Day 1: Kyoto",
            "route": { "points": [], "inferred": false, "map_key": null }
        }))
        .unwrap(),
    );
    h.storage.put_trip(&trip).await.unwrap();
    let first_day = 1;
    for which in [Which::Trip, Which::Day(first_day)] {
        h.blobs
            .put(&keys.picture(which), vec![1], "image/jpeg")
            .await
            .unwrap();
        h.blobs
            .put(&keys.public_picture(which), vec![2], "image/jpeg")
            .await
            .unwrap();
        h.blobs
            .put(
                &keys.marker(which),
                SketchKeys::marker_body("f", "© OpenStreetMap contributors"),
                "text/plain",
            )
            .await
            .unwrap();
    }
    let own = h.owner(Method::Get, "/trips/kyoto/map", None).await;
    assert_eq!(own.status, 200, "{}", own.body);
    assert!(own.body["url"]
        .as_str()
        .unwrap()
        .ends_with("sketch/amzn-owner/kyoto/trip.jpg"));
    assert_eq!(own.body["credit"], "© OpenStreetMap contributors");
    let day = h
        .owner(
            Method::Get,
            &format!("/trips/kyoto/map?day={first_day}"),
            None,
        )
        .await;
    assert!(
        day.body["url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("day-{first_day}.jpg")),
        "{}",
        day.body
    );

    // A member household gets the public form.
    let ruth = h
        .member_household("amzn-ruth", "Ruth's living room", "kyoto")
        .await;
    let theirs = h.bearer(Method::Get, "/trips/kyoto/map", None, &ruth).await;
    assert_eq!(theirs.status, 200, "{}", theirs.body);
    assert!(theirs.body["url"]
        .as_str()
        .unwrap()
        .ends_with("trip-public.jpg"));

    // So does the read-only view.
    let code = h
        .owner(
            Method::Post,
            "/trips/kyoto/code",
            Some(json!({ "expires_in_days": 7 })),
        )
        .await;
    let code = code.body["code"].as_str().unwrap().to_owned();
    let view = h
        .call(Method::Get, &format!("/view?code={code}"), None, None, None)
        .await;
    assert!(
        view.body["map"]["url"]
            .as_str()
            .unwrap()
            .ends_with("trip-public.jpg"),
        "{}",
        view.body
    );
}

/// F-713, F-714: a reel is planned on demand with its music presigned; a day film is served
/// once stored and can be queued by the owner.
#[tokio::test]
async fn reels_are_planned_on_demand_and_day_films_are_served_and_queued() {
    use mantelpiece_core::film::FilmKeys;
    let h = harness();
    let owner = HouseholdId::new("amzn-owner");
    let mut ids = Vec::new();
    for i in 0..6u8 {
        let id = enriched(&h, &format!("IMG_{i}.jpg"), 40 + i, &["Street"], None).await;
        let mut m = h.storage.get_media(&owner, &id).await.unwrap().unwrap();
        m.trip = Some(TripId::new("rome"));
        m.signals.width = Some(4032);
        m.signals.height = Some(3024);
        if i == 1 {
            m.enrichment.landmark = Some(mantelpiece_core::media::Tag {
                name: "Colosseum".into(),
                confidence: 0.9,
            });
        }
        h.storage.put_media(&m).await.unwrap();
        ids.push(id.to_string());
    }
    let trip: mantelpiece_core::trip::Trip = serde_json::from_value(json!({
        "id": "rome", "household": "amzn-owner", "name": "Rome",
        "start": "2026-04-04T05:00:00Z", "end": "2026-04-05T20:00:00Z",
        "days": [
            { "index": 1, "date": "2026-04-04", "media": ids[..4], "title": null,
              "route": { "points": [], "inferred": false, "map_key": null } },
            { "index": 2, "date": "2026-04-05", "media": ids[4..], "title": null,
              "route": { "points": [], "inferred": false, "map_key": null } }
        ],
        "route": { "points": [], "inferred": false, "map_key": null },
        "highlights": [], "contributors": [], "clock_offsets": [], "narrative": null, "hero": null
    }))
    .unwrap();
    h.storage.put_trip(&trip).await.unwrap();

    let day = h.owner(Method::Get, "/trips/rome/reel?day=1", None).await;
    assert_eq!(day.status, 200, "{}", day.body);
    assert_eq!(day.body["title"]["text"], "Rome");
    assert_eq!(day.body["title"]["sub"], "Day 1");
    assert_eq!(day.body["slides"].as_array().unwrap().len(), 4);
    assert_eq!(day.body["slides"][1]["label"]["text"], "Colosseum");
    let url = day.body["track"]["url"].as_str().unwrap();
    assert!(
        url.starts_with("https://blobs.test/music/")
            && std::path::Path::new(url)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("m4a")),
        "{url}"
    );
    assert!(day.body["credit"]["text"]
        .as_str()
        .unwrap()
        .contains("CC BY"));

    let whole = h.owner(Method::Get, "/trips/rome/reel", None).await;
    assert_eq!(whole.status, 200, "{}", whole.body);
    assert_eq!(whole.body["slides"].as_array().unwrap().len(), 6);
    assert!(whole.body["title"]["sub"].is_null());
    assert_eq!(
        h.owner(Method::Get, "/trips/rome/reel?day=2", None)
            .await
            .status,
        422,
        "two photos are too few"
    );
    assert_eq!(
        h.owner(Method::Get, "/trips/rome/reel?day=9", None)
            .await
            .status,
        404
    );

    assert_eq!(
        h.owner(Method::Get, "/trips/rome/film?day=1", None)
            .await
            .status,
        404,
        "no film yet"
    );
    let keys = FilmKeys::new(&owner, &TripId::new("rome"));
    h.blobs
        .put(&keys.film(1), vec![1], "video/mp4")
        .await
        .unwrap();
    h.blobs
        .put(
            &keys.marker(1),
            FilmKeys::marker_body("f", 30_000, "2026-04-04T21:00:00Z"),
            "text/plain",
        )
        .await
        .unwrap();
    let film = h.owner(Method::Get, "/trips/rome/film?day=1", None).await;
    assert_eq!(film.status, 200, "{}", film.body);
    assert!(film.body["url"]
        .as_str()
        .unwrap()
        .ends_with("films/amzn-owner/rome/day-1.mp4"));
    assert_eq!(film.body["duration_ms"], 30_000);
    assert_eq!(film.body["made_at"], "2026-04-04T21:00:00Z");

    let queued = h
        .owner(Method::Post, "/trips/rome/film", Some(json!({ "day": 2 })))
        .await;
    assert_eq!(queued.status, 202, "{}", queued.body);
    assert_eq!(
        h.films.queued.lock().unwrap().as_slice(),
        &[(owner.clone(), TripId::new("rome"), 2)]
    );
    assert_eq!(
        h.owner(Method::Post, "/trips/rome/film", Some(json!({ "day": 9 })))
            .await
            .status,
        404
    );
}
