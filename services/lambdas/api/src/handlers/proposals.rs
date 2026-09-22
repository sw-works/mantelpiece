//! Proposals (F-410, constitution III): what an agent or co-editor wants to change, with
//! before-and-after counts and samples, waiting for the remote. `POST /proposals` is the
//! `request_confirmation` tool target; the TV lists them, cancels them, or mints the token
//! through `/confirmations` with the proposal id.

use mantelpiece_core::confirmation::ConfirmedAction;
use mantelpiece_core::events::EventKind;
use mantelpiece_core::ids::{MediaId, ProposalId, TripId};
use mantelpiece_core::proposal::{Proposal, ProposalKind, ProposalState, Sample};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::app::{App, Caller};
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

#[derive(Deserialize)]
struct CreateBody {
    action: ConfirmedAction,
    trip: Option<TripId>,
    #[serde(default)]
    before: Option<u32>,
    #[serde(default)]
    after: Option<u32>,
    #[serde(default)]
    samples: Vec<Sample>,
}

fn kind_of(action: &ConfirmedAction) -> ProposalKind {
    match action {
        ConfirmedAction::ApplyDedup { .. } => ProposalKind::Dedup,
        ConfirmedAction::ApplyCleanup { .. } => ProposalKind::Cleanup,
        ConfirmedAction::DeleteMedia { .. } | ConfirmedAction::RemoveMedia { .. } => {
            ProposalKind::RemoveMedia
        }
        ConfirmedAction::RemoveContributor { .. } => ProposalKind::RemoveContributor,
        ConfirmedAction::ShareAlbum { .. } => ProposalKind::Share,
    }
}

/// What the TV shows plus the JSON the phone or agent reads.
pub fn view(p: &Proposal, now: time::OffsetDateTime) -> Value {
    let mut v = serde_json::to_value(p).unwrap_or(Value::Null);
    v["state"] = json!(p.state_at(now));
    v["expires_at"] = json!(p
        .expires_at()
        .format(&time::format_description::well_known::Rfc3339)
        .ok());
    v
}

#[allow(clippy::too_many_lines)]
pub async fn create(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let household = caller.household().clone();
    let body: CreateBody = req.json()?;
    let requested_by = match &caller {
        Caller::Owner { .. } => "agent".to_owned(),
        Caller::Contributor(Session::Upload { contributor, .. }) => {
            let c = app
                .storage
                .get_contributor(&household, contributor)
                .await?
                .ok_or_else(|| ApiError::forbidden("unknown contributor"))?;
            if !c.can_edit() {
                return Err(ApiError::forbidden("co-editors only"));
            }
            contributor.to_string()
        }
        Caller::Contributor(_) => return Err(ApiError::forbidden("owner only")),
    };
    let now = app.now();
    let kind = kind_of(&body.action);
    let all = app.storage.list_media(&household).await?;
    let live = |ids: &[String]| {
        all.iter()
            .filter(|m| ids.contains(&m.id.to_string()) && !m.is_removed())
            .count()
    };

    // Counts and samples the TV must show before anything is minted (constitution III).
    let (before, after, samples) = match &body.action {
        ConfirmedAction::ApplyDedup { group_ids } => {
            let members: Vec<_> = all
                .iter()
                .filter(|m| {
                    !m.is_removed()
                        && m.burst_group
                            .as_ref()
                            .is_some_and(|g| group_ids.contains(g))
                })
                .collect();
            let removable = members.iter().filter(|m| !m.is_keeper).count();
            let total = all.iter().filter(|m| !m.is_removed()).count();
            let samples = group_ids
                .iter()
                .take(4)
                .map(|g| Sample {
                    keep: members
                        .iter()
                        .find(|m| m.burst_group.as_deref() == Some(g) && m.is_keeper)
                        .map(|m| m.id.clone()),
                    remove: members
                        .iter()
                        .filter(|m| m.burst_group.as_deref() == Some(g) && !m.is_keeper)
                        .map(|m| m.id.clone())
                        .collect(),
                    reason: "near duplicate".into(),
                })
                .collect();
            (count(total), count(total - removable), samples)
        }
        ConfirmedAction::ApplyCleanup { media_ids } => {
            let total = all.iter().filter(|m| !m.is_removed()).count();
            let removable = live(media_ids);
            let samples = all
                .iter()
                .filter(|m| media_ids.contains(&m.id.to_string()) && !m.is_removed())
                .take(4)
                .map(|m| Sample {
                    keep: None,
                    remove: vec![m.id.clone()],
                    reason: m
                        .cleanup_reason
                        .map_or("cleanup", |r| r.as_str())
                        .to_owned(),
                })
                .collect();
            (count(total), count(total - removable), samples)
        }
        ConfirmedAction::DeleteMedia { media_ids }
        | ConfirmedAction::RemoveMedia { media_ids, .. } => {
            let total = all.iter().filter(|m| !m.is_removed()).count();
            let removable = live(media_ids);
            let samples = media_ids
                .iter()
                .take(4)
                .map(|id| Sample {
                    keep: None,
                    remove: vec![MediaId::new(id.as_str())],
                    reason: "removal".into(),
                })
                .collect();
            (count(total), count(total - removable), samples)
        }
        ConfirmedAction::ShareAlbum { .. } | ConfirmedAction::RemoveContributor { .. } => (
            body.before.unwrap_or(0),
            body.after.unwrap_or(0),
            body.samples.clone(),
        ),
    };

    let proposal = {
        let mut p = Proposal::open(
            household.clone(),
            kind,
            body.action.clone(),
            requested_by,
            now,
        )
        .with_counts(before, after)
        .with_samples(samples);
        if let Some(t) = body.trip.clone() {
            p = p.for_trip(t);
        }
        p
    };
    app.storage.put_proposal(&proposal).await?;
    let event = match &body.action {
        ConfirmedAction::ApplyDedup { group_ids } => Some(EventKind::DedupProposal {
            trip: body.trip.clone(),
            groups: count(group_ids.len()),
            removable: before.saturating_sub(after),
        }),
        ConfirmedAction::ApplyCleanup { .. } => Some(EventKind::CleanupProposal {
            trip: body.trip.clone(),
            count: before.saturating_sub(after),
            reasons: proposal
                .samples
                .iter()
                .map(|s| s.reason.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        }),
        _ => None,
    };
    if let Some(kind) = event {
        app.emit(&household, kind).await?;
    }
    Resp::created(view(&proposal, now))
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Open and confirmed proposals, newest first. Expired ones fall away.
pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let now = app.now();
    let mut all = app.storage.list_proposals(caller.household()).await?;
    all.retain(|p| {
        matches!(
            p.state_at(now),
            ProposalState::Open | ProposalState::Confirmed
        )
    });
    all.reverse();
    Resp::ok(all.iter().map(|p| view(p, now)).collect::<Vec<_>>())
}

/// One proposal, with its confirmation token once the TV confirmed it. This is what the
/// `request_confirmation` tool polls.
pub async fn get(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let p = app
        .storage
        .get_proposal(caller.household(), &ProposalId::new(id))
        .await?
        .ok_or_else(|| ApiError::not_found("no such proposal"))?;
    Resp::ok(view(&p, app.now()))
}

/// "Keep everything": nothing changes, ever, for this proposal.
pub async fn cancel(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    if let Caller::Contributor(Session::Upload { contributor, .. }) = &caller {
        let c = app
            .storage
            .get_contributor(caller.household(), contributor)
            .await?;
        if !c.is_some_and(|c| c.can_edit()) {
            return Err(ApiError::forbidden("owner or co-editor only"));
        }
    }
    let mut p = app
        .storage
        .get_proposal(caller.household(), &ProposalId::new(id))
        .await?
        .ok_or_else(|| ApiError::not_found("no such proposal"))?;
    p.cancel(app.now())
        .map_err(|e| ApiError::conflict(e.to_string()))?;
    app.storage.put_proposal(&p).await?;
    Resp::ok(view(&p, app.now()))
}

/// After a token was redeemed by an irreversible tool, close the proposal it came from.
pub async fn mark_applied(
    app: &App,
    household: &mantelpiece_core::ids::HouseholdId,
    token: &str,
) -> Result<(), ApiError> {
    for mut p in app.storage.list_proposals(household).await? {
        if p.confirmation.as_deref() == Some(token) && p.state == ProposalState::Confirmed {
            let _ = p.apply();
            app.storage.put_proposal(&p).await?;
        }
    }
    Ok(())
}
