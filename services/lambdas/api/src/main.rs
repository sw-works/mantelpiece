//! Lambda entry point: adapts API Gateway HTTP API events onto the transport-neutral
//! router and wires the DynamoDB and S3 backends from environment variables.

use lambda_http::{run, service_fn, Body, Error, Request, RequestExt, Response};
use memorylane_api::app::{App, Config};
use memorylane_api::req::{Jwt, Method, Req};
use memorylane_api::starter::SfnStarter;
use memorylane_api::tokens::Signer;
use memorylane_store::bus::EventBridgeBus;
use memorylane_store::dynamo::DynamoStorage;
use memorylane_store::faces::RekognitionFaces;
use memorylane_store::mailer::SesMailer;
use memorylane_store::maps::LocationMaps;
use memorylane_store::s3::S3Blobs;
use memorylane_store::speech::PollySpeech;
use memorylane_store::vectors::DynamoVectors;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

fn env(name: &str) -> Result<String, Error> {
    std::env::var(name).map_err(|_| Error::from(format!("missing env {name}")))
}

fn to_req(event: Request) -> Req {
    let method = match *event.method() {
        http::Method::GET => Some(Method::Get),
        http::Method::POST => Some(Method::Post),
        http::Method::PUT => Some(Method::Put),
        http::Method::PATCH => Some(Method::Patch),
        http::Method::DELETE => Some(Method::Delete),
        _ => None,
    };
    let query = event
        .query_string_parameters()
        .iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    let bearer = event
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .map(str::to_owned);
    let jwt = event.request_context_ref().and_then(|ctx| match ctx {
        lambda_http::request::RequestContext::ApiGatewayV2(v2) => v2
            .authorizer
            .as_ref()
            .and_then(|a| a.jwt.as_ref())
            .map(|jwt| Jwt {
                sub: jwt.claims.get("sub").cloned().unwrap_or_default(),
                name: jwt.claims.get("name").cloned(),
            }),
        _ => None,
    });
    let device = event
        .headers()
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let path = event.raw_http_path().to_owned();
    let body = match event.into_body() {
        Body::Text(t) => t.into_bytes(),
        Body::Binary(b) => b,
        _ => Vec::new(),
    };
    Req {
        method,
        path,
        query,
        bearer,
        body,
        jwt,
        device,
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let aws = aws_config::load_from_env().await;
    let app = Arc::new(App {
        storage: Arc::new(DynamoStorage::from_config(&aws, env("TABLE_NAME")?)),
        blobs: Arc::new(S3Blobs::from_config(&aws, env("MEDIA_BUCKET")?)),
        vectors: Arc::new(DynamoVectors::from_config(&aws, env("TABLE_NAME")?)),
        bus: Arc::new(EventBridgeBus::from_config(&aws, env("EVENT_BUS")?)),
        maps: Arc::new(LocationMaps::from_config(&aws)),
        faces: Arc::new(RekognitionFaces::from_config(&aws)),
        speech: Arc::new(PollySpeech::from_config(&aws)),
        mailer: Arc::new(SesMailer::from_config(
            &aws,
            std::env::var("SHARE_SENDER").ok(),
        )),
        starter: Arc::new(SfnStarter::from_config(
            &aws,
            env("ORGANIZE_STATE_MACHINE_ARN")?,
        )),
        signer: Signer::new(env("TOKEN_SECRET")?),
        config: Config {
            link_base: env("LINK_BASE")?,
            upload_base: env("UPLOAD_BASE")?,
            oauth_issuer: std::env::var("OAUTH_ISSUER").unwrap_or_default(),
            oauth_domain: std::env::var("OAUTH_DOMAIN").unwrap_or_default(),
            gateway_url: std::env::var("GATEWAY_URL").unwrap_or_default(),
            poll_hold: Duration::seconds(
                std::env::var("POLL_HOLD_SECONDS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(25),
            ),
            ..Config::default()
        },
        clock: Arc::new(OffsetDateTime::now_utc),
    });

    run(service_fn(move |event: Request| {
        let app = app.clone();
        async move {
            // A CORS preflight reaches here through the catch-all route. API Gateway adds
            // the allow headers, but the browser also needs a 2xx or it fails the real
            // request ("Load failed" in Safari).
            if event.method() == http::Method::OPTIONS {
                return Ok::<_, Error>(Response::builder().status(204).body(Body::Empty)?);
            }
            let resp = app.handle(to_req(event)).await;
            let body = if resp.body.is_null() {
                Body::Empty
            } else {
                Body::Text(resp.body.to_string())
            };
            Ok::<_, Error>(
                Response::builder()
                    .status(resp.status)
                    .header("content-type", "application/json")
                    .body(body)?,
            )
        }
    }))
    .await
}
