//! Lambda entry point. API Gateway reaches this function with an HTTP event, adapted onto
//! the transport-neutral router. AgentCore Gateway reaches it twice per tool call: first as
//! its REQUEST interceptor, which seals the caller's identity into the arguments, then as
//! the tool target, with the arguments as the event and the tool's name in the client
//! context (`tools.rs`). Wires the DynamoDB
//! and S3 backends from environment variables.

use lambda_http::{Body, Error, Request, RequestExt};
use lambda_runtime::{run, service_fn, LambdaEvent};
use mantelpiece_api::app::{App, Config};
use mantelpiece_api::films::{LambdaFilms, NoFilms};
use mantelpiece_api::req::{Jwt, Method, Req};
use mantelpiece_api::starter::SfnStarter;
use mantelpiece_api::tokens::Signer;
use mantelpiece_api::tools;
use mantelpiece_store::bus::EventBridgeBus;
use mantelpiece_store::dynamo::DynamoStorage;
use mantelpiece_store::faces::RekognitionFaces;
use mantelpiece_store::live::IvsLive;
use mantelpiece_store::mailer::SesMailer;
use mantelpiece_store::maps::LocationMaps;
use mantelpiece_store::push::WebPush;
use mantelpiece_store::s3::S3Blobs;
use mantelpiece_store::speech::PollySpeech;
use mantelpiece_store::vectors::DynamoVectors;
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
    let token_secret = env("TOKEN_SECRET")?;
    let upload_base = env("UPLOAD_BASE")?;
    let push = WebPush::from_secret(&token_secret, upload_base.clone())
        .map_err(|e| Error::from(e.to_string()))?;
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
        push: Arc::new(push),
        live: Arc::new(IvsLive::from_config(
            &aws,
            std::env::var("IVS_RECORDING_ARN").ok(),
        )),
        starter: Arc::new(SfnStarter::from_config(
            &aws,
            env("ORGANIZE_STATE_MACHINE_ARN")?,
        )),
        films: match std::env::var("FILM_FUNCTION_NAME").unwrap_or_default() {
            name if name.is_empty() => Arc::new(NoFilms),
            name => Arc::new(LambdaFilms::from_config(&aws, name)),
        },
        signer: Signer::new(token_secret),
        config: Config {
            link_base: env("LINK_BASE")?,
            upload_base,
            oauth_issuer: std::env::var("OAUTH_ISSUER").unwrap_or_default(),
            oauth_domain: std::env::var("OAUTH_DOMAIN").unwrap_or_default(),
            gateway_url: std::env::var("GATEWAY_URL").unwrap_or_default(),
            agent_client_id: std::env::var("AGENT_CLIENT_ID").unwrap_or_default(),
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

    run(service_fn(move |event: LambdaEvent<serde_json::Value>| {
        let app = app.clone();
        async move {
            let (payload, context) = event.into_parts();
            // AgentCore Gateway: the payload is the tool's arguments.
            let tool = context
                .client_context
                .as_ref()
                .and_then(|c| c.custom.get(tools::TOOL_NAME_KEY))
                .cloned();
            if let Some(tool) = tool {
                return match tools::call(&app, &tool, &payload).await {
                    Ok(result) => Ok::<_, Error>(result),
                    Err(e) => {
                        tracing::warn!(tool = %tool, error = %e, "tool call failed");
                        Ok(serde_json::json!({ "error": e.to_string() }))
                    }
                };
            }
            // The Gateway's REQUEST interceptor: seal who is calling into the tool call.
            if payload.get("interceptorInputVersion").is_some() {
                return Ok(tools::intercept(&app, &payload));
            }
            // API Gateway HTTP API.
            let event: Request = lambda_http::request::from_str(&payload.to_string())?;
            // A CORS preflight reaches here through the catch-all route. API Gateway adds
            // the allow headers, but the browser also needs a 2xx or it fails the real
            // request ("Load failed" in Safari).
            if event.method() == http::Method::OPTIONS {
                return Ok(serde_json::json!({ "statusCode": 204, "headers": {}, "body": "" }));
            }
            let resp = app.handle(to_req(event)).await;
            let body = if resp.body.is_null() {
                String::new()
            } else {
                resp.body.to_string()
            };
            Ok(serde_json::json!({
                "statusCode": resp.status,
                "headers": { "content-type": "application/json" },
                "body": body,
                "isBase64Encoded": false,
            }))
        }
    }))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_http_api_event_still_becomes_a_request() {
        let event = serde_json::json!({
            "version": "2.0", "routeKey": "ANY /{proxy+}", "rawPath": "/pair/requests",
            "rawQueryString": "a=1", "headers": { "authorization": "Bearer tok", "x-device-id": "d1" },
            "queryStringParameters": { "a": "1" },
            "requestContext": { "accountId": "1", "apiId": "a", "domainName": "x", "domainPrefix": "x",
                "http": { "method": "POST", "path": "/pair/requests", "protocol": "HTTP/1.1", "sourceIp": "1.1.1.1", "userAgent": "t" },
                "requestId": "r", "routeKey": "ANY /{proxy+}", "stage": "$default", "time": "t", "timeEpoch": 0 },
            "body": "{\"code\":\"X\"}", "isBase64Encoded": false
        });
        let request: Request = lambda_http::request::from_str(&event.to_string()).unwrap();
        let req = to_req(request);
        assert_eq!(req.method, Some(Method::Post));
        assert_eq!(req.path, "/pair/requests");
        assert_eq!(req.bearer.as_deref(), Some("tok"));
        assert_eq!(req.device.as_deref(), Some("d1"));
        assert_eq!(req.query.get("a").map(String::as_str), Some("1"));
        assert_eq!(req.body, b"{\"code\":\"X\"}");
    }
}
