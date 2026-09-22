//! Gateway tools (contracts/tools.md). AgentCore Gateway invokes this Lambda directly, not
//! through API Gateway: the event is the tool's arguments and the tool's name rides in the
//! client context as `bedrockAgentCoreToolName` (`<target>___<tool>`). Each tool is one of
//! the API's own routes, run here as an internal request for the household the call names,
//! so a tool and its route can never drift apart.
//!
//! The Gateway does not hand the caller's identity to a Lambda target, so this function is
//! also the Gateway's REQUEST interceptor ([`intercept`]): it reads the bearer token the
//! Gateway's JWT authorizer accepted, and seals who is calling, for which tool, into the
//! arguments under this API's own key. [`call`] refuses any tool call without a fresh seal
//! for that tool. A signed-in person always acts for their own household, whatever the
//! arguments say; only the Concierge agent's client may name the household. Irreversible
//! tools still need a confirmation token minted on the TV, which no tool can mint.

use std::collections::HashMap;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use mantelpiece_core::events::EventKind;
use mantelpiece_core::ids::HouseholdId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use time::{Duration, OffsetDateTime};

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Method, Req};
use crate::tokens::{Claims, Session};

/// Client-context key the Gateway sets on every tool invocation.
pub const TOOL_NAME_KEY: &str = "bedrockAgentCoreToolName";

/// Argument the interceptor adds and the dispatcher consumes. Callers cannot forge it: it
/// is overwritten on the way in and signed with a key only this function holds.
pub const CALLER_ARG: &str = "_caller";
/// A seal is good for one tool call made right now.
const SEAL_TTL: Duration = Duration::seconds(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallerKind {
    /// Someone signed in through Cognito; their `sub` is their household.
    Person,
    /// The Concierge agent's client-credentials identity.
    Agent,
}

/// Who the Gateway authenticated, as sealed by the interceptor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCaller {
    pub kind: CallerKind,
    /// Cognito `sub`: the person's household, or the agent's client id.
    pub subject: String,
    /// The qualified tool this seal was made for.
    pub tool: String,
    #[serde(with = "time::serde::timestamp")]
    pub exp: OffsetDateTime,
}

/// The claims of a JWT the Gateway's authorizer has already verified. The signature is the
/// Gateway's to check; issuer, expiry and token use are checked again here.
fn accepted_claims(app: &App, bearer: &str) -> Result<Value, String> {
    let token = bearer
        .strip_prefix("Bearer ")
        .or_else(|| bearer.strip_prefix("bearer "))
        .unwrap_or(bearer);
    let payload = token.split('.').nth(1).ok_or("not a JWT")?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .map_err(|_| "unreadable JWT")?;
    let claims: Value = serde_json::from_slice(&bytes).map_err(|_| "unreadable JWT")?;
    if claims["iss"].as_str() != Some(app.config.oauth_issuer.as_str()) {
        return Err("token from another issuer".into());
    }
    if claims["token_use"].as_str() != Some("access") {
        return Err("an access token is required".into());
    }
    if claims["exp"].as_i64().unwrap_or(0) <= app.now().unix_timestamp() {
        return Err("token expired".into());
    }
    Ok(claims)
}

fn caller_from(app: &App, claims: &Value, tool: &str) -> Result<ToolCaller, String> {
    let client = claims["client_id"].as_str().unwrap_or_default();
    let subject = claims["sub"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("token without a subject")?;
    let kind = if !client.is_empty() && client == app.config.agent_client_id {
        CallerKind::Agent
    } else {
        CallerKind::Person
    };
    Ok(ToolCaller {
        kind,
        subject: subject.to_owned(),
        tool: tool.to_owned(),
        exp: app.now() + SEAL_TTL,
    })
}

/// The Gateway's REQUEST interceptor. Anything but a tool call passes through untouched; a
/// tool call gets the caller sealed into its arguments, or is answered with an error here.
pub fn intercept(app: &App, event: &Value) -> Value {
    let request = &event["mcp"]["gatewayRequest"];
    let mut body = request["body"].clone();
    if body["method"].as_str() != Some("tools/call") {
        return json!({ "interceptorOutputVersion": "1.0", "mcp": { "transformedGatewayRequest": { "body": body } } });
    }
    let bearer = request["headers"].as_object().and_then(|h| {
        h.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            .and_then(|(_, v)| v.as_str())
    });
    let tool = body["params"]["name"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let sealed = bearer
        .ok_or_else(|| "no authorization header reached the interceptor".to_owned())
        .and_then(|b| accepted_claims(app, b))
        .and_then(|claims| caller_from(app, &claims, &tool));
    match sealed {
        Ok(caller) => {
            if !body["params"]["arguments"].is_object() {
                body["params"]["arguments"] = json!({});
            }
            body["params"]["arguments"][CALLER_ARG] = Value::String(app.signer.seal(&caller));
            json!({ "interceptorOutputVersion": "1.0", "mcp": { "transformedGatewayRequest": { "body": body } } })
        }
        Err(reason) => {
            tracing::warn!(tool = %tool, reason = %reason, "tool call refused at the interceptor");
            json!({
                "interceptorOutputVersion": "1.0",
                "mcp": { "transformedGatewayResponse": {
                    "statusCode": 200,
                    "body": { "jsonrpc": "2.0", "id": body["id"], "error": { "code": -32001, "message": format!("caller not accepted: {reason}") } },
                } },
            })
        }
    }
}

/// The household this call acts for: a person's own, always; the one named, for the agent.
fn household_for(app: &App, tool: &str, args: &Value) -> Result<HouseholdId, ApiError> {
    let sealed = args[CALLER_ARG]
        .as_str()
        .ok_or_else(|| ApiError::unauthorized("tool calls come through the Gateway"))?;
    let caller: ToolCaller = app
        .signer
        .open(sealed)
        .map_err(|e| ApiError::unauthorized(format!("caller seal refused: {e}")))?;
    if app.now() >= caller.exp {
        return Err(ApiError::unauthorized("caller seal expired"));
    }
    if tool_of(&caller.tool) != tool_of(tool) {
        return Err(ApiError::unauthorized(
            "caller seal was made for another tool",
        ));
    }
    match caller.kind {
        CallerKind::Person => Ok(HouseholdId::new(caller.subject)),
        CallerKind::Agent => Ok(HouseholdId::new(arg(args, "household")?)),
    }
}

/// `api___get_trip` names the tool `get_trip` on the target `api`.
pub fn tool_of(qualified: &str) -> &str {
    qualified.rsplit_once("___").map_or(qualified, |(_, t)| t)
}

fn arg<'a>(args: &'a Value, name: &str) -> Result<&'a str, ApiError> {
    args[name]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad_request(format!("{name} required")))
}

fn opt<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args[name].as_str().filter(|s| !s.is_empty())
}

/// A JSON argument that may arrive as an object or as a string holding JSON.
fn json_arg(args: &Value, name: &str) -> Result<Value, ApiError> {
    match &args[name] {
        Value::String(s) => serde_json::from_str(s)
            .map_err(|e| ApiError::bad_request(format!("{name} is not JSON: {e}"))),
        Value::Null => Err(ApiError::bad_request(format!("{name} required"))),
        other => Ok(other.clone()),
    }
}

/// One internal request, as the household's owner, for the length of this call.
async fn route(
    app: &App,
    household: &HouseholdId,
    method: Method,
    path: String,
    query: &[(&str, Option<&str>)],
    body: Option<Value>,
) -> Result<Value, ApiError> {
    let bearer = app.signer.sign(&Claims {
        session: Session::Tv {
            household: household.clone(),
            subject: household.to_string(),
        },
        exp: app.now() + Duration::minutes(5),
    });
    let query: HashMap<String, String> = query
        .iter()
        .filter_map(|(k, v)| v.map(|v| ((*k).to_owned(), v.to_owned())))
        .collect();
    let resp = app
        .handle(Req {
            method: Some(method),
            path,
            query,
            bearer: Some(bearer),
            body: body.map(|b| b.to_string().into_bytes()).unwrap_or_default(),
            jwt: None,
            device: None,
        })
        .await;
    // A withheld story answers 422 with the reasons; the agent needs those, not an error.
    if resp.status < 300 || resp.status == 422 {
        Ok(resp.body)
    } else {
        let message = resp.body["error"].as_str().unwrap_or("tool failed");
        Err(ApiError::new(resp.status, message))
    }
}

/// Run one tool for the caller the interceptor sealed into `args`.
#[allow(clippy::too_many_lines)]
pub async fn call(app: &App, tool: &str, args: &Value) -> Result<Value, ApiError> {
    let household = household_for(app, tool, args)?;
    let h = &household;
    // The seal has done its work; routes that take the arguments whole never see it.
    let mut args = args.clone();
    if let Some(map) = args.as_object_mut() {
        map.remove(CALLER_ARG);
    }
    let args = &args;
    match tool_of(tool) {
        "get_household" => route(app, h, Method::Get, "/me".into(), &[], None).await,
        "list_trips" => route(app, h, Method::Get, "/trips".into(), &[], None).await,
        "get_trip" => {
            let path = format!("/trips/{}", arg(args, "trip_id")?);
            route(app, h, Method::Get, path, &[], None).await
        }
        "get_trip_code" => {
            let path = format!("/trips/{}/code", arg(args, "trip_id")?);
            route(app, h, Method::Post, path, &[], Some(json!({}))).await
        }
        "search_media" => {
            let body = json!({ "query": arg(args, "query")?, "trip": opt(args, "trip_id"), "people": args["people"].as_array().cloned().unwrap_or_default() });
            route(app, h, Method::Post, "/search".into(), &[], Some(body)).await
        }
        "list_cleanup_candidates" => {
            let q = [("trip", opt(args, "trip_id"))];
            route(app, h, Method::Get, "/media/cleanup".into(), &q, None).await
        }
        "get_landmark_facts" => {
            let key = mantelpiece_core::landmark::key(arg(args, "landmark")?);
            route(app, h, Method::Get, format!("/landmarks/{key}"), &[], None).await
        }
        "save_landmark_trivia" => {
            let key = mantelpiece_core::landmark::key(arg(args, "landmark")?);
            let body = json!({ "lines": args["lines"] });
            route(
                app,
                h,
                Method::Post,
                format!("/landmarks/{key}/trivia"),
                &[],
                Some(body),
            )
            .await
        }
        "memories_for" => {
            let q = [("date", opt(args, "date"))];
            route(app, h, Method::Get, "/memories/home".into(), &q, None).await
        }
        "get_now_showing" => {
            let shows = route(app, h, Method::Get, "/presence".into(), &[], None).await?;
            let tv = opt(args, "tv");
            Ok(shows
                .as_array()
                .and_then(|rows| {
                    rows.iter()
                        .find(|r| tv.is_none_or(|tv| r["device_id"] == tv))
                })
                .cloned()
                .unwrap_or(Value::Null))
        }
        "describe_photo" => {
            let body = json!({ "question": arg(args, "question")?, "media": opt(args, "media_id"), "tv": opt(args, "tv") });
            route(app, h, Method::Post, "/ask".into(), &[], Some(body)).await
        }
        "create_playlist" => {
            let body = json!({ "name": arg(args, "name")?, "media": args["media"], "source": opt(args, "source").unwrap_or("agent") });
            route(app, h, Method::Post, "/playlists".into(), &[], Some(body)).await
        }
        "save_narrative" => {
            let path = format!("/trips/{}/narrative", arg(args, "trip_id")?);
            let body = json!({ "text": opt(args, "text").unwrap_or_default(), "model_id": arg(args, "model_id")?, "stories": args["stories"], "withhold": args["withhold"].as_bool().unwrap_or(false) });
            route(app, h, Method::Post, path, &[], Some(body)).await
        }
        "push_event" => {
            let kind: EventKind = serde_json::from_value(json_arg(args, "event")?)
                .map_err(|e| ApiError::bad_request(format!("not a feed event: {e}")))?;
            let event = app.emit(h, kind).await?;
            Ok(json!({ "seq": event.seq }))
        }
        "set_keeper" => {
            let path = format!("/media/{}/keeper", arg(args, "media_id")?);
            route(app, h, Method::Post, path, &[], Some(json!({}))).await
        }
        "synthesize_narration" => {
            let path = format!("/trips/{}/narration", arg(args, "trip_id")?);
            let body = json!({ "day": args["day"] });
            route(app, h, Method::Post, path, &[], Some(body)).await
        }
        "purge_expired" => {
            route(
                app,
                h,
                Method::Post,
                "/media/purge".into(),
                &[],
                Some(json!({})),
            )
            .await
        }
        "request_confirmation" => {
            let body = json!({ "action": json_arg(args, "action")?, "before": args["before"], "after": args["after"] });
            route(app, h, Method::Post, "/proposals".into(), &[], Some(body)).await
        }
        "apply_dedup" => {
            route(
                app,
                h,
                Method::Post,
                "/media/dedup/apply".into(),
                &[],
                Some(args.clone()),
            )
            .await
        }
        "apply_cleanup" => {
            route(
                app,
                h,
                Method::Post,
                "/media/cleanup/apply".into(),
                &[],
                Some(args.clone()),
            )
            .await
        }
        "delete_media" => {
            route(
                app,
                h,
                Method::Post,
                "/media/delete".into(),
                &[],
                Some(args.clone()),
            )
            .await
        }
        "share_album" => {
            route(
                app,
                h,
                Method::Post,
                "/shares".into(),
                &[],
                Some(args.clone()),
            )
            .await
        }
        "remove_contributor" => {
            let path = format!("/contributors/{}", arg(args, "contributor_id")?);
            let q = [("confirmation", opt(args, "confirmation"))];
            route(app, h, Method::Delete, path, &q, None).await
        }
        // Declared in the contract, not built yet: say so instead of answering nothing.
        "list_duplicates" | "read_memory" | "write_memory" => Err(ApiError::new(
            501,
            format!("{} is not available yet", tool_of(tool)),
        )),
        other => Err(ApiError::not_found(format!("unknown tool {other}"))),
    }
}
