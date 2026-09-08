//! Router and dependencies shared by all handlers.

use memorylane_core::ids::HouseholdId;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

use crate::error::ApiError;
use crate::handlers;
use crate::req::{Method, Req, Resp};
use crate::tokens::{Claims, Session, Signer};
use memorylane_store::{Blobs, Storage};

#[derive(Clone, Debug)]
pub struct Config {
    /// Base of the sign-in approval page, e.g. `https://link.example.com`.
    pub link_base: String,
    /// Base of the uploader page, e.g. `https://upload.example.com`.
    pub upload_base: String,
    /// How long `/events` and `/device/poll` hold before answering empty.
    pub poll_hold: Duration,
    /// Multipart part size in bytes.
    pub part_size: u64,
    /// Largest accepted original (F-101: 2 GB).
    pub max_file_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            link_base: "https://link.memorylane.test".into(),
            upload_base: "https://upload.memorylane.test".into(),
            poll_hold: Duration::seconds(25),
            part_size: 8 * 1024 * 1024,
            max_file_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

pub type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;

pub struct App {
    pub storage: Arc<dyn Storage>,
    pub blobs: Arc<dyn Blobs>,
    pub starter: Arc<dyn crate::starter::Starter>,
    pub signer: Signer,
    pub config: Config,
    pub clock: Clock,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Who is calling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    /// Household owner: the TV session, or the owner's phone with a Cognito JWT.
    Owner {
        household: HouseholdId,
        subject: String,
    },
    /// A contributor's upload session.
    Contributor(Session),
}

impl Caller {
    pub fn household(&self) -> &HouseholdId {
        match self {
            Self::Owner { household, .. } => household,
            Self::Contributor(s) => s.household(),
        }
    }
}

impl App {
    pub fn now(&self) -> OffsetDateTime {
        (self.clock)()
    }

    pub async fn handle(&self, req: Req) -> Resp {
        match self.route(&req).await {
            Ok(resp) => resp,
            Err(e) => e.into(),
        }
    }

    async fn route(&self, req: &Req) -> Result<Resp, ApiError> {
        let segs = req.segments();
        let method = req
            .method
            .ok_or_else(|| ApiError::new(405, "method not allowed"))?;
        match (method, segs.as_slice()) {
            (Method::Post, ["device", "code"]) => handlers::device::issue(self, req).await,
            (Method::Post, ["device", "approve"]) => handlers::device::approve(self, req).await,
            (Method::Get, ["device", "poll"]) => handlers::device::poll(self, req).await,

            (Method::Get, ["me"]) => handlers::session::me(self, req).await,
            // `/join/amazon` sits behind the Cognito JWT authorizer; `/join` is open for guests.
            (Method::Post, ["join"] | ["join", "amazon"]) => handlers::codes::join(self, req).await,

            (Method::Get, ["codes"]) => handlers::codes::list(self, req).await,
            (Method::Post, ["household", "code"]) => {
                handlers::codes::issue_household(self, req).await
            }
            (Method::Post, ["trips", trip, "code"]) => {
                handlers::codes::issue_trip(self, req, trip).await
            }
            (Method::Post, ["codes", code, "rotate"]) => {
                handlers::codes::rotate(self, req, code).await
            }
            (Method::Delete, ["codes", code]) => handlers::codes::revoke(self, req, code).await,

            (Method::Post, ["uploads", "init"]) => handlers::uploads::init(self, req).await,
            (Method::Post, ["uploads", "complete"]) => handlers::uploads::complete(self, req).await,
            (Method::Post, ["uploads", "abort"]) => handlers::uploads::abort(self, req).await,

            (Method::Get, ["events"]) => handlers::events::list(self, req).await,

            (Method::Get, ["trips"]) => handlers::trips::list(self, req).await,
            (Method::Get, ["trips", trip]) => handlers::trips::get(self, req, trip).await,

            (Method::Get, ["media", media, "url"]) => handlers::media::url(self, req, media).await,
            (Method::Delete, ["media", media]) => handlers::media::delete(self, req, media).await,
            (Method::Post, ["media", media, "pin"]) => {
                handlers::media::set_flag(self, req, media, "pin").await
            }
            (Method::Post, ["media", media, "hide"]) => {
                handlers::media::set_flag(self, req, media, "hide").await
            }

            (Method::Get, ["contributors"]) => handlers::contributors::list(self, req).await,
            (Method::Delete, ["contributors", id]) => {
                handlers::contributors::remove(self, req, id).await
            }

            (Method::Post, ["confirmations"]) => handlers::confirmations::mint(self, req).await,

            _ => Err(ApiError::not_found("no such route")),
        }
    }

    /// Parse the bearer token, if any.
    pub fn session(&self, req: &Req) -> Result<Option<Session>, ApiError> {
        match &req.bearer {
            None => Ok(None),
            Some(token) => {
                let claims: Claims = self
                    .signer
                    .verify(token, self.now())
                    .map_err(|e| ApiError::unauthorized(e.to_string()))?;
                Ok(Some(claims.session))
            }
        }
    }

    /// Anyone with a valid identity: owner or contributor.
    pub fn caller(&self, req: &Req) -> Result<Caller, ApiError> {
        if let Some(session) = self.session(req)? {
            return Ok(match session {
                Session::Tv { household, subject } | Session::Phone { household, subject } => {
                    Caller::Owner { household, subject }
                }
                upload @ Session::Upload { .. } => Caller::Contributor(upload),
            });
        }
        if let Some(jwt) = &req.jwt {
            return Ok(Caller::Owner {
                household: HouseholdId::new(jwt.sub.clone()),
                subject: jwt.sub.clone(),
            });
        }
        Err(ApiError::unauthorized("sign in required"))
    }

    /// Owner only.
    pub fn owner(&self, req: &Req) -> Result<HouseholdId, ApiError> {
        match self.caller(req)? {
            Caller::Owner { household, .. } => Ok(household),
            Caller::Contributor(_) => Err(ApiError::forbidden("owner only")),
        }
    }
}
