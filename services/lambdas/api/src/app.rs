//! Router and dependencies shared by all handlers.

use memorylane_core::contributor::Contributor;
use memorylane_core::ids::{ContributorId, HouseholdId, TripId};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

use crate::error::ApiError;
use crate::handlers;
use crate::req::{Method, Req, Resp};
use crate::tokens::{Claims, Session, Signer};
use memorylane_store::{Blobs, Bus, Faces, Mailer, Maps, Speech, Storage, Vectors};

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
    /// Cognito issuer, e.g. `https://cognito-idp.us-east-1.amazonaws.com/us-east-1_abc`.
    pub oauth_issuer: String,
    /// Cognito hosted UI base, e.g. `https://memorylane-123.auth.us-east-1.amazoncognito.com`.
    pub oauth_domain: String,
    /// The AgentCore Gateway MCP endpoint Alexa+ calls (research R6).
    pub gateway_url: String,
    /// Cognito client id of the Concierge agent: the one Gateway caller that acts for
    /// whichever household an event names. Every other caller is a signed-in person.
    pub agent_client_id: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            link_base: "https://link.memorylane.test".into(),
            upload_base: "https://upload.memorylane.test".into(),
            poll_hold: Duration::seconds(25),
            part_size: 8 * 1024 * 1024,
            max_file_bytes: 2 * 1024 * 1024 * 1024,
            oauth_issuer: "https://cognito-idp.us-east-1.amazonaws.com/us-east-1_test".into(),
            oauth_domain: "https://memorylane-test.auth.us-east-1.amazoncognito.com".into(),
            gateway_url: "https://gateway.memorylane.test/mcp".into(),
            agent_client_id: "concierge-client".into(),
        }
    }
}

pub type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;

pub struct App {
    pub storage: Arc<dyn Storage>,
    pub blobs: Arc<dyn Blobs>,
    pub vectors: Arc<dyn Vectors>,
    pub bus: Arc<dyn Bus>,
    pub maps: Arc<dyn Maps>,
    pub faces: Arc<dyn Faces>,
    pub speech: Arc<dyn Speech>,
    pub mailer: Arc<dyn Mailer>,
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
    /// The paired phone behind an owner call: its session's subject is the contributor id,
    /// where the TV's and a signed-in Amazon account's subject is the household id.
    pub fn paired_phone(&self) -> Option<ContributorId> {
        match self {
            Self::Owner { household, subject } if subject != household.as_str() => {
                Some(ContributorId::new(subject.as_str()))
            }
            _ => None,
        }
    }

    pub fn household(&self) -> &HouseholdId {
        match self {
            Self::Owner { household, .. } => household,
            Self::Contributor(s) => s.household(),
        }
    }
}

/// The caller's standing on one trip (data-model access rules).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TripAccess {
    /// The trip's own household.
    Owner {
        household: HouseholdId,
        subject: String,
    },
    /// A contributor holding the co-editor role on the trip's household (F-164).
    CoEditor {
        household: HouseholdId,
        contributor: ContributorId,
    },
    /// A plain contributor whose code names this trip.
    Contributor {
        household: HouseholdId,
        contributor: ContributorId,
    },
    /// Another household that joined the trip by code (F-162).
    Member {
        household: HouseholdId,
        owner_household: HouseholdId,
    },
}

impl TripAccess {
    /// The household whose partition holds the trip.
    pub fn owner_household(&self) -> &HouseholdId {
        match self {
            Self::Owner { household, .. }
            | Self::CoEditor { household, .. }
            | Self::Contributor { household, .. } => household,
            Self::Member {
                owner_household, ..
            } => owner_household,
        }
    }

    /// The household whose pins and hides apply (F-163).
    pub fn viewing_household(&self) -> &HouseholdId {
        match self {
            Self::Member { household, .. } => household,
            other => other.owner_household(),
        }
    }

    pub fn can_edit(&self) -> bool {
        matches!(self, Self::Owner { .. } | Self::CoEditor { .. })
    }

    /// Who a confirmation token names when this caller confirms.
    pub fn subject(&self) -> String {
        match self {
            Self::Owner { subject, .. } => subject.clone(),
            Self::CoEditor { contributor, .. } | Self::Contributor { contributor, .. } => {
                contributor.to_string()
            }
            Self::Member { household, .. } => household.to_string(),
        }
    }
}

impl App {
    pub fn now(&self) -> OffsetDateTime {
        (self.clock)()
    }

    /// Append to the household feed with fan-out to member households (F-162).
    pub async fn emit(
        &self,
        household: &HouseholdId,
        kind: memorylane_core::events::EventKind,
    ) -> Result<memorylane_core::events::Event, ApiError> {
        Ok(
            memorylane_store::feed::emit(self.storage.as_ref(), household, self.now(), kind)
                .await?,
        )
    }

    pub async fn handle(&self, req: Req) -> Resp {
        let resp = match self.route(&req).await {
            Ok(resp) => resp,
            Err(e) => e.into(),
        };
        // F-605: sign-in, shares, deletions, memberships and every gated tool route leave
        // an entry, whatever the outcome; a failed write here never fails the request.
        if let Err(e) = handlers::audit::record(self, &req, &resp).await {
            tracing::warn!(error = %e, "audit entry not written");
        }
        resp
    }

    /// A paired phone's session is a signed token, so forgetting the phone on the TV is
    /// enforced here: its contributor must still stand approved.
    async fn refuse_forgotten_phone(&self, req: &Req) -> Result<(), ApiError> {
        let Ok(Some(Session::Phone { household, subject })) = self.session(req) else {
            return Ok(());
        };
        if subject == household.as_str() {
            return Ok(());
        }
        let paired = self
            .storage
            .get_contributor(&household, &ContributorId::new(subject))
            .await?
            .is_some_and(|c| c.is_active());
        if paired {
            Ok(())
        } else {
            Err(ApiError::unauthorized(
                "this phone is no longer paired with the TV",
            ))
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn route(&self, req: &Req) -> Result<Resp, ApiError> {
        self.refuse_forgotten_phone(req).await?;
        let segs = req.segments();
        let method = req
            .method
            .ok_or_else(|| ApiError::new(405, "method not allowed"))?;
        match (method, segs.as_slice()) {
            (Method::Post, ["device", "code"]) => handlers::device::issue(self, req).await,
            (Method::Post, ["device", "approve"]) => handlers::device::approve(self, req).await,
            (Method::Get, ["device", "poll"]) => handlers::device::poll(self, req).await,
            (Method::Post, ["tv", "setup"]) => handlers::pair::setup(self, req).await,
            (Method::Patch, ["household"]) => handlers::pair::rename(self, req).await,
            (Method::Post, ["pair", "code"]) => handlers::pair::code(self, req).await,
            (Method::Get, ["pair", "code", code]) => handlers::pair::describe(self, code).await,
            (Method::Post, ["pair", "requests"]) => handlers::pair::request(self, req).await,
            (Method::Get, ["pair", "requests"]) => handlers::pair::list(self, req).await,
            (Method::Get, ["pair", "status"]) => handlers::pair::status(self, req).await,
            (Method::Post, ["pair", "requests", id, "approve"]) => {
                handlers::pair::decide(self, req, id, true).await
            }
            (Method::Post, ["pair", "requests", id, "deny"]) => {
                handlers::pair::decide(self, req, id, false).await
            }
            (Method::Post, ["session", "amazon"]) => {
                handlers::device::session_from_amazon(self, req).await
            }

            (Method::Get, [".well-known", "oauth-authorization-server"]) => {
                Ok(handlers::wellknown::authorization_server(self))
            }
            (Method::Get, [".well-known", "oauth-protected-resource"]) => {
                Ok(handlers::wellknown::protected_resource(self))
            }
            (Method::Get, ["audit"]) => handlers::audit::list(self, req).await,
            (Method::Get, ["me"]) => handlers::session::me(self, req).await,
            (Method::Delete, ["me"]) => handlers::session::delete_account(self, req).await,
            (Method::Patch, ["me"]) => handlers::session::settings(self, req).await,
            (Method::Post, ["shares"]) => handlers::shares::create(self, req).await,
            (Method::Get, ["shares"]) => handlers::shares::list(self, req).await,
            (Method::Delete, ["shares", share]) => handlers::shares::revoke(self, req, share).await,
            (Method::Get, ["s", share]) => handlers::shares::open(self, share).await,
            (Method::Post, ["trips", trip, "narration"]) => {
                handlers::narration::synthesize(self, req, trip).await
            }
            (Method::Get, ["trips", trip, "narration"]) => {
                handlers::narration::url(self, req, trip).await
            }
            (Method::Post, ["people", "enable"]) => handlers::people::enable(self, req).await,
            (Method::Post, ["people", "disable"]) => handlers::people::disable(self, req).await,
            (Method::Get, ["people"]) => handlers::people::list(self, req).await,
            (Method::Patch, ["people", id]) => handlers::people::update(self, req, id).await,
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
            (Method::Get, ["uploads", "mine"]) => handlers::uploads::mine(self, req).await,

            (Method::Get, ["events"]) => handlers::events::list(self, req).await,

            (Method::Post, ["search"]) => handlers::search::search(self, req).await,
            (Method::Post, ["playlists"]) => handlers::playlists::create(self, req).await,
            (Method::Get, ["playlists"]) => handlers::playlists::list(self, req).await,
            (Method::Get, ["playlists", id]) => handlers::playlists::get(self, req, id).await,
            (Method::Post, ["playlists", id, "play"]) => {
                handlers::playlists::play(self, req, id).await
            }
            (Method::Put, ["presence"]) => handlers::presence::put(self, req).await,
            (Method::Get, ["presence"]) => handlers::presence::list(self, req).await,
            (Method::Post, ["ask"]) => handlers::ask::ask(self, req).await,
            (Method::Get, ["memories", "home"]) => handlers::memories::home(self, req).await,
            (Method::Get, ["map"]) => handlers::memories::map(self, req).await,

            (Method::Get, ["trips"]) => handlers::trips::list(self, req).await,
            (Method::Get, ["trips", trip]) => handlers::trips::get(self, req, trip).await,
            (Method::Get, ["trips", trip, "map"]) => handlers::trips::map(self, req, trip).await,
            (Method::Post, ["trips", trip, "play"]) => handlers::trips::play(self, req, trip).await,
            (Method::Patch, ["trips", trip]) => handlers::trips::rename(self, req, trip).await,
            (Method::Post, ["trips", trip, "roles"]) => {
                handlers::trips::set_role(self, req, trip).await
            }
            (Method::Post, ["memberships"]) => handlers::memberships::join(self, req).await,
            (Method::Delete, ["memberships", trip]) => {
                handlers::memberships::leave(self, req, trip).await
            }
            (Method::Post, ["trips", trip, "narrative"]) => {
                handlers::trips::save_narrative(self, req, trip).await
            }
            (Method::Post, ["trips", trip, "narrative", "regenerate"]) => {
                handlers::trips::regenerate(self, req, trip).await
            }
            (Method::Post, ["trips", trip, "reactions"]) => {
                handlers::reactions::create(self, req, trip).await
            }
            (Method::Get, ["trips", trip, "reactions"]) => {
                handlers::reactions::list(self, req, trip).await
            }
            // Read-only web view by trip code, no session (F-159).
            (Method::Get, ["view"]) => handlers::trips::view(self, req).await,

            // Literal media routes come before the `{media}` patterns.
            (Method::Get, ["media", "cleanup"]) => {
                handlers::media::cleanup_candidates(self, req).await
            }
            (Method::Post, ["media", "cleanup", "apply"]) => {
                handlers::media::apply_cleanup(self, req).await
            }
            (Method::Post, ["media", "dedup", "apply"]) => {
                handlers::media::apply_dedup(self, req).await
            }
            (Method::Post, ["media", "delete"]) => handlers::media::delete_many(self, req).await,
            (Method::Post, ["media", "purge"]) => handlers::media::purge_expired(self, req).await,
            (Method::Post, ["media", media, "restore"]) => {
                handlers::media::restore(self, req, media).await
            }
            (Method::Get, ["media", media, "url"]) => handlers::media::url(self, req, media).await,
            (Method::Delete, ["media", media]) => handlers::media::delete(self, req, media).await,
            (Method::Post, ["media", media, "pin"]) => {
                handlers::media::set_flag(self, req, media, "pin").await
            }
            (Method::Post, ["media", media, "hide"]) => {
                handlers::media::set_flag(self, req, media, "hide").await
            }
            (Method::Post, ["media", media, "keeper"]) => {
                handlers::media::set_keeper(self, req, media).await
            }

            (Method::Get, ["contributors"]) => handlers::contributors::list(self, req).await,
            (Method::Delete, ["contributors", id]) => {
                handlers::contributors::remove(self, req, id).await
            }
            (Method::Get, ["contributors", id, "export"]) => {
                handlers::contributors::export(self, req, id).await
            }

            (Method::Post, ["confirmations"]) => handlers::confirmations::mint(self, req).await,
            (Method::Post, ["proposals"]) => handlers::proposals::create(self, req).await,
            (Method::Get, ["proposals"]) => handlers::proposals::list(self, req).await,
            (Method::Get, ["proposals", id]) => handlers::proposals::get(self, req, id).await,
            (Method::Post, ["proposals", id, "cancel"]) => {
                handlers::proposals::cancel(self, req, id).await
            }

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

    /// Anyone with a valid identity: owner or contributor. A guest session bound to a
    /// device (F-153) is only accepted from that device.
    pub fn caller(&self, req: &Req) -> Result<Caller, ApiError> {
        if let Some(session) = self.session(req)? {
            if let Session::Upload {
                device: Some(bound),
                ..
            } = &session
            {
                if req.device.as_deref() != Some(bound.as_str()) {
                    return Err(ApiError::unauthorized("session is bound to another device"));
                }
            }
            return Ok(match session {
                Session::Tv { household, subject } | Session::Phone { household, subject } => {
                    Caller::Owner { household, subject }
                }
                upload @ Session::Upload { .. } => Caller::Contributor(upload),
                Session::Pairing { .. } => {
                    return Err(ApiError::unauthorized("waiting for approval on the TV"))
                }
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

    /// The read-only web view: a trip-scoped code in the query string stands in for a
    /// session (F-159). Rotated or revoked codes stop working; expiry only stops uploads.
    pub async fn viewer(&self, req: &Req) -> Result<(HouseholdId, TripId), ApiError> {
        let raw = req
            .query
            .get("code")
            .ok_or_else(|| ApiError::unauthorized("sign in required"))?;
        let code = memorylane_core::codes::normalize(raw)
            .ok_or_else(|| ApiError::bad_request("invalid code"))?;
        let code = self
            .storage
            .get_trip_code(&code)
            .await?
            .ok_or_else(|| ApiError::not_found("unknown code"))?;
        if code.revoked_at.is_some() {
            return Err(ApiError::gone("this code was replaced"));
        }
        match code.scope {
            memorylane_core::trip_code::CodeScope::Trip { trip } => Ok((code.household, trip)),
            memorylane_core::trip_code::CodeScope::Household => Err(ApiError::forbidden(
                "household codes do not open a trip view",
            )),
        }
    }

    /// Owner only.
    pub fn owner(&self, req: &Req) -> Result<HouseholdId, ApiError> {
        match self.caller(req)? {
            Caller::Owner { household, .. } => Ok(household),
            Caller::Contributor(_) => Err(ApiError::forbidden("owner only")),
        }
    }

    /// Resolve what the caller may do on `trip`: owner of the trip's household, member
    /// household that joined it, co-editor, or plain contributor bound to it. 404 when the
    /// owner has no such trip, 403 when a contributor's code names another trip.
    pub async fn trip_access(&self, req: &Req, trip: &TripId) -> Result<TripAccess, ApiError> {
        match self.caller(req)? {
            Caller::Owner { household, subject } => {
                if self.storage.get_trip(&household, trip).await?.is_some() {
                    return Ok(TripAccess::Owner { household, subject });
                }
                let joined = self
                    .storage
                    .list_memberships(&household)
                    .await?
                    .into_iter()
                    .find(|m| &m.trip == trip && m.is_active());
                match joined {
                    Some(m) => Ok(TripAccess::Member {
                        household,
                        owner_household: m.owner_household,
                    }),
                    None => Err(ApiError::not_found("no such trip")),
                }
            }
            Caller::Contributor(Session::Upload {
                household,
                contributor,
                trip: bound,
                ..
            }) => {
                if bound.as_ref() != Some(trip) {
                    return Err(ApiError::forbidden("not your trip"));
                }
                let c = self
                    .storage
                    .get_contributor(&household, &contributor)
                    .await?
                    .filter(Contributor::is_active)
                    .ok_or_else(|| ApiError::forbidden("contributor removed"))?;
                Ok(if c.can_edit() {
                    TripAccess::CoEditor {
                        household,
                        contributor,
                    }
                } else {
                    TripAccess::Contributor {
                        household,
                        contributor,
                    }
                })
            }
            Caller::Contributor(
                Session::Tv { household, subject } | Session::Phone { household, subject },
            ) => Ok(TripAccess::Owner { household, subject }),
            Caller::Contributor(Session::Pairing { .. }) => {
                Err(ApiError::unauthorized("waiting for approval on the TV"))
            }
        }
    }

    /// Edits on a trip: owner or co-editor (F-164). Guests and members never edit (F-165).
    pub async fn owner_or_co_editor(
        &self,
        req: &Req,
        trip: &TripId,
    ) -> Result<TripAccess, ApiError> {
        let access = self.trip_access(req, trip).await?;
        if access.can_edit() {
            Ok(access)
        } else {
            Err(ApiError::forbidden("owner or co-editor only"))
        }
    }

    /// Reads and per-household choices on a trip: the owner or a joined household.
    pub async fn member_or_owner(&self, req: &Req, trip: &TripId) -> Result<TripAccess, ApiError> {
        match self.trip_access(req, trip).await? {
            access @ (TripAccess::Owner { .. } | TripAccess::Member { .. }) => Ok(access),
            _ => Err(ApiError::forbidden("owner or member household only")),
        }
    }
}
