//! Web push to paired phones (family channel): the TV loves a batch of photos and the
//! sender's phone lights up. RFC 8030 delivery, RFC 8291 `aes128gcm` payload encryption
//! and RFC 8292 VAPID, all pure Rust through `web-push-native`, so the Lambda needs no
//! OpenSSL and no push provider account.

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use memorylane_core::contributor::PushSubscription;
use serde::Serialize;
use sha2::Sha256;
use std::sync::Mutex;
use web_push_native::jwt_simple::algorithms::ES256KeyPair;
use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint;
use web_push_native::p256::PublicKey;
use web_push_native::{Auth, WebPushBuilder};

use crate::{Result, StorageError};

/// What the phone shows. The service worker turns it into a notification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PushMessage {
    pub title: String,
    pub body: String,
    /// Page the notification opens.
    pub url: String,
}

/// How a delivery went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushOutcome {
    Sent,
    /// The push service says the subscription no longer exists (404 or 410): forget it.
    Gone,
}

#[async_trait]
pub trait Push: Send + Sync {
    /// The VAPID public key, base64url of the uncompressed point: the browser's
    /// `applicationServerKey`.
    fn public_key(&self) -> String;
    async fn send(&self, to: &PushSubscription, message: &PushMessage) -> Result<PushOutcome>;
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

/// Records what would have been sent; endpoints listed in `gone` answer [`PushOutcome::Gone`].
#[derive(Debug, Default)]
pub struct MemoryPush {
    pub sent: Mutex<Vec<(PushSubscription, PushMessage)>>,
    pub gone: Mutex<Vec<String>>,
}

#[async_trait]
impl Push for MemoryPush {
    fn public_key(&self) -> String {
        "BMemoryPushPublicKey".into()
    }

    async fn send(&self, to: &PushSubscription, message: &PushMessage) -> Result<PushOutcome> {
        if self.gone.lock().unwrap().contains(&to.endpoint) {
            return Ok(PushOutcome::Gone);
        }
        self.sent
            .lock()
            .unwrap()
            .push((to.clone(), message.clone()));
        Ok(PushOutcome::Sent)
    }
}

/// VAPID keys derived from the API's token secret, so the stack needs no second secret and
/// the key survives redeploys. `subject` is the contact URL push services may use.
pub struct WebPush {
    key_pair: ES256KeyPair,
    public: String,
    subject: String,
}

impl std::fmt::Debug for WebPush {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebPush")
            .field("public", &self.public)
            .field("subject", &self.subject)
            .finish_non_exhaustive()
    }
}

impl WebPush {
    /// Derive the P-256 key from `secret`. A scalar outside the curve order is vanishingly
    /// unlikely; the counter walks past one anyway.
    pub fn from_secret(secret: &str, subject: impl Into<String>) -> Result<Self> {
        for counter in 0u8..=255 {
            let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(backend)?;
            mac.update(b"memorylane vapid v1");
            mac.update(&[counter]);
            let bytes = mac.finalize().into_bytes();
            if let (Ok(key_pair), Ok(secret)) = (
                ES256KeyPair::from_bytes(&bytes),
                web_push_native::p256::SecretKey::from_slice(&bytes),
            ) {
                let public =
                    URL_SAFE_NO_PAD.encode(secret.public_key().to_encoded_point(false).as_bytes());
                return Ok(Self {
                    key_pair,
                    public,
                    subject: subject.into(),
                });
            }
        }
        Err(StorageError::Backend("no usable VAPID key".into()))
    }

    /// The encrypted, VAPID-signed request for one subscription.
    pub fn request(
        &self,
        to: &PushSubscription,
        message: &PushMessage,
    ) -> Result<http::Request<Vec<u8>>> {
        let endpoint = to.endpoint.parse().map_err(backend)?;
        let ua_public = PublicKey::from_sec1_bytes(&decode(&to.p256dh)?).map_err(backend)?;
        let auth = decode(&to.auth)?;
        if auth.len() != 16 {
            return Err(StorageError::Backend("push auth must be 16 bytes".into()));
        }
        let payload =
            serde_json::to_vec(message).map_err(|e| StorageError::Serde(e.to_string()))?;
        WebPushBuilder::new(endpoint, ua_public, Auth::clone_from_slice(&auth))
            .with_vapid(&self.key_pair, &self.subject)
            .build(payload)
            .map_err(backend)
    }
}

fn decode(s: &str) -> Result<Vec<u8>> {
    let trimmed = s.trim_end_matches('=');
    URL_SAFE_NO_PAD
        .decode(trimmed.replace('+', "-").replace('/', "_"))
        .map_err(backend)
}

#[async_trait]
impl Push for WebPush {
    fn public_key(&self) -> String {
        self.public.clone()
    }

    async fn send(&self, to: &PushSubscription, message: &PushMessage) -> Result<PushOutcome> {
        let request = self.request(to, message)?;
        let status = tokio::task::spawn_blocking(move || -> Result<u16> {
            let (parts, body) = request.into_parts();
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build()
                .into();
            let mut call = agent.post(parts.uri.to_string());
            for (name, value) in &parts.headers {
                call = call.header(name.as_str(), value.to_str().map_err(backend)?);
            }
            // Push services want a TTL; one day is plenty for "they loved your photos".
            let response = call
                .header("TTL", "86400")
                .send(&body[..])
                .map_err(backend)?;
            Ok(response.status().as_u16())
        })
        .await
        .map_err(backend)??;
        match status {
            200..=299 => Ok(PushOutcome::Sent),
            404 | 410 => Ok(PushOutcome::Gone),
            other => Err(StorageError::Backend(format!(
                "push service answered {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use web_push_native::p256::SecretKey;

    fn subscription() -> (SecretKey, [u8; 16], PushSubscription) {
        let ua_secret = SecretKey::from_slice(&[7u8; 32]).unwrap();
        let ua_public = ua_secret.public_key().to_encoded_point(false);
        let auth = [9u8; 16];
        let sub = PushSubscription {
            endpoint: "https://push.example.com/send/abc".into(),
            p256dh: URL_SAFE_NO_PAD.encode(ua_public.as_bytes()),
            auth: URL_SAFE_NO_PAD.encode(auth),
        };
        (ua_secret, auth, sub)
    }

    #[test]
    fn the_vapid_key_is_stable_for_a_secret_and_differs_across_secrets() {
        let a = WebPush::from_secret("secret-one", "https://upload.example.com").unwrap();
        let b = WebPush::from_secret("secret-one", "https://upload.example.com").unwrap();
        let c = WebPush::from_secret("secret-two", "https://upload.example.com").unwrap();
        assert_eq!(a.public_key(), b.public_key());
        assert_ne!(a.public_key(), c.public_key());
        // Uncompressed P-256 point: 65 bytes, first 0x04.
        let raw = URL_SAFE_NO_PAD.decode(a.public_key()).unwrap();
        assert_eq!(raw.len(), 65);
        assert_eq!(raw[0], 4);
    }

    #[test]
    fn a_push_request_is_encrypted_signed_and_decrypts_for_the_browser() {
        let push = WebPush::from_secret("s", "https://upload.example.com").unwrap();
        let (ua_secret, auth, sub) = subscription();
        let message = PushMessage {
            title: "Grandma's TV".into(),
            body: "loved your 14 photos".into(),
            url: "https://upload.example.com/".into(),
        };
        let req = push.request(&sub, &message).unwrap();
        assert_eq!(req.uri().to_string(), sub.endpoint);
        assert_eq!(req.headers()["content-encoding"], "aes128gcm");
        let authz = req.headers()["authorization"].to_str().unwrap();
        assert!(authz.starts_with("vapid t="), "{authz}");
        assert!(authz.contains(&format!("k={}", push.public_key())));
        // What the browser does with the body: RFC 8291 decryption with its own keys.
        let plain = web_push_native::decrypt(
            req.body().clone(),
            &ua_secret,
            &Auth::clone_from_slice(&auth),
        )
        .unwrap();
        let back: serde_json::Value = serde_json::from_slice(&plain).unwrap();
        assert_eq!(back["body"], "loved your 14 photos");
    }

    #[test]
    fn a_bad_subscription_is_refused_before_any_network() {
        let push = WebPush::from_secret("s", "https://upload.example.com").unwrap();
        let (_, _, mut sub) = subscription();
        sub.auth = URL_SAFE_NO_PAD.encode([1u8; 4]);
        let message = PushMessage {
            title: "t".into(),
            body: "b".into(),
            url: "u".into(),
        };
        assert!(push.request(&sub, &message).is_err());
    }
}
