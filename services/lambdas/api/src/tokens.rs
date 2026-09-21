//! HMAC-signed bearer tokens issued by this API.
//!
//! Two kinds: a TV session minted when a device code is approved, and an upload session
//! minted when a contributor joins by trip code (open question 6: signed short-lived
//! tokens rather than Cognito unauthenticated identities). Both carry the household so
//! every later call is tenant-scoped without a lookup.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use memorylane_core::ids::{ContributorId, HouseholdId, TripId};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;
use time::{Duration, OffsetDateTime};

type HmacSha256 = Hmac<Sha256>;

pub const TV_TTL: Duration = Duration::days(90);
pub const UPLOAD_TTL: Duration = Duration::days(30);
pub const PHONE_TTL: Duration = Duration::days(30);
pub const PAIRING_TTL: Duration = Duration::hours(1);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Session {
    /// The signed-in TV. Acts as the household owner.
    Tv {
        household: HouseholdId,
        subject: String,
    },
    /// The owner's phone after approving a device code. Also acts as the owner, so the
    /// uploader page needs no Cognito JWT on later calls.
    Phone {
        household: HouseholdId,
        subject: String,
    },
    /// A phone that scanned the TV's pairing code and waits for approval. Good only for
    /// asking how the request stands; approval trades it for a `Phone` session whose
    /// subject is the contributor id.
    Pairing {
        household: HouseholdId,
        contributor: ContributorId,
    },
    /// A contributor's phone. May upload into, and read, the trip named here.
    Upload {
        household: HouseholdId,
        contributor: ContributorId,
        trip: Option<TripId>,
        code: String,
        /// Browser install the session is bound to (F-153); absent on older tokens.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device: Option<String>,
    },
}

impl Session {
    pub fn household(&self) -> &HouseholdId {
        match self {
            Self::Tv { household, .. }
            | Self::Phone { household, .. }
            | Self::Pairing { household, .. }
            | Self::Upload { household, .. } => household,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    #[serde(flatten)]
    pub session: Session,
    #[serde(with = "time::serde::timestamp")]
    pub exp: OffsetDateTime,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TokenError {
    #[error("malformed token")]
    Malformed,
    #[error("bad signature")]
    BadSignature,
    #[error("token expired")]
    Expired,
}

#[derive(Clone, Debug)]
pub struct Signer {
    secret: Vec<u8>,
}

impl Signer {
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    pub fn sign(&self, claims: &Claims) -> String {
        let payload = serde_json::to_vec(claims).expect("claims serialise");
        let payload_b64 = URL_SAFE_NO_PAD.encode(&payload);
        let sig = self.mac(payload_b64.as_bytes());
        format!("{payload_b64}.{}", URL_SAFE_NO_PAD.encode(sig))
    }

    pub fn verify(&self, token: &str, now: OffsetDateTime) -> Result<Claims, TokenError> {
        let (payload_b64, sig_b64) = token.split_once('.').ok_or(TokenError::Malformed)?;
        let sig = URL_SAFE_NO_PAD
            .decode(sig_b64)
            .map_err(|_| TokenError::Malformed)?;
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("hmac accepts any key length");
        mac.update(payload_b64.as_bytes());
        mac.verify_slice(&sig)
            .map_err(|_| TokenError::BadSignature)?;
        let payload = URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| TokenError::Malformed)?;
        let claims: Claims = serde_json::from_slice(&payload).map_err(|_| TokenError::Malformed)?;
        if now >= claims.exp {
            return Err(TokenError::Expired);
        }
        Ok(claims)
    }

    fn mac(&self, data: &[u8]) -> Vec<u8> {
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("hmac accepts any key length");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn claims(exp: OffsetDateTime) -> Claims {
        Claims {
            session: Session::Tv {
                household: HouseholdId::new("h"),
                subject: "s".into(),
            },
            exp,
        }
    }

    #[test]
    fn round_trip() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let s = Signer::new("secret");
        let token = s.sign(&claims(now + Duration::hours(1)));
        assert_eq!(
            s.verify(&token, now).unwrap(),
            claims(now + Duration::hours(1))
        );
    }

    #[test]
    fn tampered_wrong_key_and_expired_fail() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let s = Signer::new("secret");
        let token = s.sign(&claims(now + Duration::hours(1)));
        assert_eq!(
            Signer::new("other").verify(&token, now),
            Err(TokenError::BadSignature)
        );
        let mut tampered = token.clone();
        tampered.insert(3, 'x');
        assert!(s.verify(&tampered, now).is_err());
        assert_eq!(s.verify("nodot", now), Err(TokenError::Malformed));
        assert_eq!(
            s.verify(&token, now + Duration::hours(2)),
            Err(TokenError::Expired)
        );
    }

    #[test]
    fn upload_session_serialises_flat() {
        let c = Claims {
            session: Session::Upload {
                household: HouseholdId::new("h"),
                contributor: ContributorId::new("c"),
                trip: Some(TripId::new("t")),
                code: "ABC234".into(),
                device: None,
            },
            exp: datetime!(2026-09-08 10:00:00 UTC),
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["kind"], "upload");
        assert_eq!(v["trip"], "t");
        assert!(v["exp"].is_number());
    }
}
