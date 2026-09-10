//! A proposed irreversible change waiting for the remote (F-410, constitution III).
//!
//! Agents and co-editors open proposals; only the TV's Review Changes modal moves one to
//! `Confirmed` by minting a token, and only the tool that redeems the token marks it
//! `Applied`.

use crate::confirmation::ConfirmedAction;
use crate::ids::{HouseholdId, MediaId, ProposalId, TripId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{Duration, OffsetDateTime};

/// Open proposals expire unanswered after this long.
pub const OPEN_TTL: Duration = Duration::hours(24);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    Dedup,
    Cleanup,
    RemoveMedia,
    RemoveContributor,
    Share,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalState {
    Open,
    Cancelled,
    Confirmed,
    Applied,
    Expired,
}

/// One before-and-after example shown on the TV: a group's keeper and what goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sample {
    pub keep: Option<MediaId>,
    pub remove: Vec<MediaId>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: ProposalId,
    pub household: HouseholdId,
    pub trip: Option<TripId>,
    pub kind: ProposalKind,
    pub action: ConfirmedAction,
    /// Counts shown before the token is minted (constitution III).
    pub before: u32,
    pub after: u32,
    pub samples: Vec<Sample>,
    pub state: ProposalState,
    /// `agent` or a co-editor's contributor id.
    pub requested_by: String,
    #[serde(with = "time::serde::rfc3339")]
    pub opened_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProposalError {
    #[error("proposal is {0:?}, not open")]
    NotOpen(ProposalState),
    #[error("proposal is {0:?}, not confirmed")]
    NotConfirmed(ProposalState),
}

impl Proposal {
    /// Open a proposal with no counts yet; set them with [`Proposal::with_counts`] before
    /// it is shown (constitution III wants before-and-after on screen).
    pub fn open(
        household: HouseholdId,
        kind: ProposalKind,
        action: ConfirmedAction,
        requested_by: impl Into<String>,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            id: ProposalId::random(),
            household,
            trip: None,
            kind,
            action,
            before: 0,
            after: 0,
            samples: vec![],
            state: ProposalState::Open,
            requested_by: requested_by.into(),
            opened_at: now,
            confirmation: None,
        }
    }

    #[must_use]
    pub fn for_trip(mut self, trip: TripId) -> Self {
        self.trip = Some(trip);
        self
    }

    #[must_use]
    pub fn with_counts(mut self, before: u32, after: u32) -> Self {
        self.before = before;
        self.after = after;
        self
    }

    #[must_use]
    pub fn with_samples(mut self, samples: Vec<Sample>) -> Self {
        self.samples = samples;
        self
    }

    pub fn expires_at(&self) -> OffsetDateTime {
        self.opened_at + OPEN_TTL
    }

    /// Current state, folding in expiry.
    pub fn state_at(&self, now: OffsetDateTime) -> ProposalState {
        if self.state == ProposalState::Open && now >= self.expires_at() {
            ProposalState::Expired
        } else {
            self.state
        }
    }

    pub fn cancel(&mut self, now: OffsetDateTime) -> Result<(), ProposalError> {
        self.transition_from_open(now, ProposalState::Cancelled)
    }

    /// Keep the proposal id to token binding so `/confirmations` can only be replayed
    /// against the same open proposal.
    pub fn confirm(&mut self, token: &str, now: OffsetDateTime) -> Result<(), ProposalError> {
        self.transition_from_open(now, ProposalState::Confirmed)?;
        self.confirmation = Some(token.to_owned());
        Ok(())
    }

    pub fn apply(&mut self) -> Result<(), ProposalError> {
        if self.state != ProposalState::Confirmed {
            return Err(ProposalError::NotConfirmed(self.state));
        }
        self.state = ProposalState::Applied;
        Ok(())
    }

    fn transition_from_open(
        &mut self,
        now: OffsetDateTime,
        to: ProposalState,
    ) -> Result<(), ProposalError> {
        match self.state_at(now) {
            ProposalState::Open => {
                self.state = to;
                Ok(())
            }
            other => {
                self.state = other;
                Err(ProposalError::NotOpen(other))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn proposal(now: OffsetDateTime) -> Proposal {
        Proposal::open(
            HouseholdId::new("h"),
            ProposalKind::Dedup,
            ConfirmedAction::ApplyDedup {
                group_ids: vec!["g1".into()],
            },
            "agent",
            now,
        )
        .for_trip(TripId::new("t"))
        .with_counts(20, 0)
        .with_samples(vec![Sample {
            keep: Some(MediaId::new("m1")),
            remove: vec![MediaId::new("m2")],
            reason: "near duplicate".into(),
        }])
    }

    #[test]
    fn open_confirm_apply() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let mut p = proposal(now);
        assert_eq!(
            p.apply(),
            Err(ProposalError::NotConfirmed(ProposalState::Open))
        );
        assert_eq!(p.confirm("tok", now), Ok(()));
        assert_eq!(p.confirmation.as_deref(), Some("tok"));
        assert_eq!(
            p.cancel(now),
            Err(ProposalError::NotOpen(ProposalState::Confirmed))
        );
        assert_eq!(p.apply(), Ok(()));
        assert_eq!(p.state, ProposalState::Applied);
    }

    #[test]
    fn open_expires_after_a_day() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let mut p = proposal(now);
        assert_eq!(p.state_at(now + Duration::hours(23)), ProposalState::Open);
        assert_eq!(p.state_at(now + OPEN_TTL), ProposalState::Expired);
        assert_eq!(
            p.confirm("tok", now + OPEN_TTL),
            Err(ProposalError::NotOpen(ProposalState::Expired))
        );
        assert_eq!(p.state, ProposalState::Expired);
        let mut q = proposal(now);
        assert_eq!(q.cancel(now), Ok(()));
        assert_eq!(q.state, ProposalState::Cancelled);
    }
}
