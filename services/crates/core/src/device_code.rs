//! Sign-in on the TV by device code (architecture section 3, F-401).
//!
//! Cognito has no RFC 8628 device grant, so this is the small custom flow: the TV asks for
//! a code, shows it as text and QR, and polls. The phone opens the link, signs in with
//! Login with Amazon through Cognito, and approves. On the next poll the TV receives the
//! approval exactly once.

use crate::codes;
use crate::ids::{DeviceId, HouseholdId};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{Duration, OffsetDateTime};

pub const CODE_LEN: usize = 8;
pub const DEFAULT_TTL: Duration = Duration::minutes(10);
/// How long the TV should hold `/device/poll` before answering `Pending`.
pub const POLL_HOLD: Duration = Duration::seconds(25);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DeviceCodeState {
    Pending,
    Approved {
        household: HouseholdId,
        subject: String,
    },
    /// Approval was handed to the TV; the code can never be used again.
    Consumed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCode {
    pub code: String,
    pub device: DeviceId,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// DynamoDB TTL attribute; the row disappears after this.
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    #[serde(flatten)]
    pub state: DeviceCodeState,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DeviceCodeError {
    #[error("code expired")]
    Expired,
    #[error("code already approved")]
    AlreadyApproved,
    #[error("code already consumed")]
    Consumed,
    #[error("device id does not match")]
    WrongDevice,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Poll {
    Pending,
    Approved {
        household: HouseholdId,
        subject: String,
    },
    Expired,
}

impl DeviceCode {
    pub fn issue(rng: &mut impl RngCore, device: DeviceId, now: OffsetDateTime) -> Self {
        Self {
            code: codes::generate(rng, CODE_LEN),
            device,
            created_at: now,
            expires_at: now + DEFAULT_TTL,
            state: DeviceCodeState::Pending,
        }
    }

    pub fn is_expired(&self, now: OffsetDateTime) -> bool {
        now >= self.expires_at
    }

    /// Called from the phone after Cognito sign-in.
    pub fn approve(
        &mut self,
        household: HouseholdId,
        subject: String,
        now: OffsetDateTime,
    ) -> Result<(), DeviceCodeError> {
        if self.is_expired(now) {
            return Err(DeviceCodeError::Expired);
        }
        match self.state {
            DeviceCodeState::Pending => {
                self.state = DeviceCodeState::Approved { household, subject };
                Ok(())
            }
            DeviceCodeState::Approved { .. } => Err(DeviceCodeError::AlreadyApproved),
            DeviceCodeState::Consumed => Err(DeviceCodeError::Consumed),
        }
    }

    /// Called from the TV. An approval is returned once and the code is then consumed.
    pub fn poll(
        &mut self,
        device: &DeviceId,
        now: OffsetDateTime,
    ) -> Result<Poll, DeviceCodeError> {
        if &self.device != device {
            return Err(DeviceCodeError::WrongDevice);
        }
        if self.is_expired(now) {
            return Ok(Poll::Expired);
        }
        match std::mem::replace(&mut self.state, DeviceCodeState::Consumed) {
            DeviceCodeState::Pending => {
                self.state = DeviceCodeState::Pending;
                Ok(Poll::Pending)
            }
            DeviceCodeState::Approved { household, subject } => {
                Ok(Poll::Approved { household, subject })
            }
            DeviceCodeState::Consumed => Err(DeviceCodeError::Consumed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::test_rng::SeqRng;
    use time::macros::datetime;

    fn setup() -> (DeviceCode, OffsetDateTime) {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let dc = DeviceCode::issue(&mut SeqRng(1), DeviceId::new("tv-1"), now);
        (dc, now)
    }

    #[test]
    fn happy_path_approve_then_poll_once() {
        let (mut dc, now) = setup();
        assert_eq!(dc.code.len(), CODE_LEN);
        assert_eq!(dc.poll(&DeviceId::new("tv-1"), now), Ok(Poll::Pending));
        dc.approve(
            HouseholdId::new("h"),
            "sub".into(),
            now + Duration::minutes(1),
        )
        .unwrap();
        let polled = dc
            .poll(&DeviceId::new("tv-1"), now + Duration::minutes(2))
            .unwrap();
        assert_eq!(
            polled,
            Poll::Approved {
                household: HouseholdId::new("h"),
                subject: "sub".into()
            }
        );
        assert_eq!(
            dc.poll(&DeviceId::new("tv-1"), now + Duration::minutes(2)),
            Err(DeviceCodeError::Consumed)
        );
    }

    #[test]
    fn expired_code_rejects_approval_and_reports_expired() {
        let (mut dc, now) = setup();
        let late = now + DEFAULT_TTL;
        assert_eq!(
            dc.approve(HouseholdId::new("h"), "s".into(), late),
            Err(DeviceCodeError::Expired)
        );
        assert_eq!(dc.poll(&DeviceId::new("tv-1"), late), Ok(Poll::Expired));
    }

    #[test]
    fn double_approval_rejected() {
        let (mut dc, now) = setup();
        dc.approve(HouseholdId::new("h"), "s".into(), now).unwrap();
        assert_eq!(
            dc.approve(HouseholdId::new("h2"), "s2".into(), now),
            Err(DeviceCodeError::AlreadyApproved)
        );
    }

    #[test]
    fn other_device_cannot_poll() {
        let (mut dc, now) = setup();
        assert_eq!(
            dc.poll(&DeviceId::new("tv-2"), now),
            Err(DeviceCodeError::WrongDevice)
        );
    }

    #[test]
    fn approved_state_flattens_in_json() {
        let (mut dc, now) = setup();
        dc.approve(HouseholdId::new("h"), "s".into(), now).unwrap();
        let v = serde_json::to_value(&dc).unwrap();
        assert_eq!(v["state"], "approved");
        assert_eq!(v["household"], "h");
    }
}
