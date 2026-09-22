//! OAuth metadata for the Alexa+ MCP add-on (research R6, contracts/tools.md). Alexa+ reads
//! the authorization server document, runs OAuth 2.1 with PKCE against Cognito's hosted
//! endpoints with the `mantelpiece/tools` scope, and calls the Gateway with the token.
//! Shapes follow RFC 8414 and RFC 9728; not yet verified against the Alexa+ preview.

use serde_json::json;

use crate::app::App;
use crate::req::Resp;

pub const SCOPE: &str = "mantelpiece/tools";

#[must_use]
pub fn authorization_server(app: &App) -> Resp {
    let d = &app.config.oauth_domain;
    Resp {
        status: 200,
        body: json!({
            "issuer": app.config.oauth_issuer,
            "authorization_endpoint": format!("{d}/oauth2/authorize"),
            "token_endpoint": format!("{d}/oauth2/token"),
            "jwks_uri": format!("{}/.well-known/jwks.json", app.config.oauth_issuer),
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code", "refresh_token"],
            "code_challenge_methods_supported": ["S256"],
            "token_endpoint_auth_methods_supported": ["none"],
            "scopes_supported": [SCOPE, "openid"],
            "resource": app.config.gateway_url,
        }),
    }
}

#[must_use]
pub fn protected_resource(app: &App) -> Resp {
    Resp {
        status: 200,
        body: json!({
            "resource": app.config.gateway_url,
            "authorization_servers": [app.config.oauth_issuer],
            "scopes_supported": [SCOPE],
            "bearer_methods_supported": ["header"],
            "mcp_protocol_version": "2025-11-25",
        }),
    }
}
