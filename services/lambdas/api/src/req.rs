//! Transport-neutral request and response.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;

use crate::ApiError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// Identity established by API Gateway's JWT authorizer, when the caller signed in with
/// Login with Amazon through Cognito.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Jwt {
    pub sub: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Req {
    pub method: Option<Method>,
    pub path: String,
    pub query: HashMap<String, String>,
    pub bearer: Option<String>,
    pub body: Vec<u8>,
    pub jwt: Option<Jwt>,
    /// `x-device-id` header: the browser or TV install a guest session is bound to.
    pub device: Option<String>,
}

impl Req {
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, ApiError> {
        if self.body.is_empty() {
            return Err(ApiError::bad_request("empty body"));
        }
        Ok(serde_json::from_slice(&self.body)?)
    }

    pub fn query_param(&self, name: &str) -> Result<&str, ApiError> {
        self.query
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| ApiError::bad_request(format!("missing query parameter {name}")))
    }

    /// Path split on `/`, empty segments dropped.
    pub fn segments(&self) -> Vec<&str> {
        self.path.split('/').filter(|s| !s.is_empty()).collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resp {
    pub status: u16,
    pub body: serde_json::Value,
}

impl Resp {
    pub fn ok<T: Serialize>(value: T) -> Result<Self, ApiError> {
        Ok(Self {
            status: 200,
            body: serde_json::to_value(value).map_err(|e| ApiError::internal(e.to_string()))?,
        })
    }

    pub fn created<T: Serialize>(value: T) -> Result<Self, ApiError> {
        Ok(Self {
            status: 201,
            body: serde_json::to_value(value).map_err(|e| ApiError::internal(e.to_string()))?,
        })
    }

    pub fn no_content() -> Result<Self, ApiError> {
        Ok(Self {
            status: 204,
            body: serde_json::Value::Null,
        })
    }
}

impl From<ApiError> for Resp {
    fn from(e: ApiError) -> Self {
        Self {
            status: e.status,
            body: e.body(),
        }
    }
}
