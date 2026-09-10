//! Share links by email (F-601) through Amazon SES. Optional: without a verified sender
//! the API still returns the link and answers 409 to an email request.

use async_trait::async_trait;
use std::sync::Mutex;

use crate::{Result, StorageError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub text: String,
}

#[async_trait]
pub trait Mailer: Send + Sync {
    /// True when a sender is configured, so callers can refuse early.
    fn configured(&self) -> bool;
    async fn send(&self, mail: Mail) -> Result<()>;
}

/// Records what would have been sent.
#[derive(Debug, Default)]
pub struct MemoryMailer {
    pub sent: Mutex<Vec<Mail>>,
}

#[async_trait]
impl Mailer for MemoryMailer {
    fn configured(&self) -> bool {
        true
    }

    async fn send(&self, mail: Mail) -> Result<()> {
        self.sent.lock().unwrap().push(mail);
        Ok(())
    }
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

/// SES v2 with one verified sender identity (infra `shareSender`). Compile-checked only.
#[derive(Debug, Clone)]
pub struct SesMailer {
    client: aws_sdk_sesv2::Client,
    sender: Option<String>,
}

impl SesMailer {
    pub fn from_config(config: &aws_config::SdkConfig, sender: Option<String>) -> Self {
        Self {
            client: aws_sdk_sesv2::Client::new(config),
            sender: sender.filter(|s| !s.trim().is_empty()),
        }
    }
}

#[async_trait]
impl Mailer for SesMailer {
    fn configured(&self) -> bool {
        self.sender.is_some()
    }

    async fn send(&self, mail: Mail) -> Result<()> {
        use aws_sdk_sesv2::types::{Body, Content, Destination, EmailContent, Message};

        let sender = self
            .sender
            .clone()
            .ok_or_else(|| StorageError::Backend("no share sender configured".into()))?;
        let content = |s: &str| Content::builder().data(s).charset("UTF-8").build();
        let message = Message::builder()
            .subject(content(&mail.subject).map_err(backend)?)
            .body(
                Body::builder()
                    .text(content(&mail.text).map_err(backend)?)
                    .build(),
            )
            .build();
        self.client
            .send_email()
            .from_email_address(sender)
            .destination(Destination::builder().to_addresses(mail.to).build())
            .content(EmailContent::builder().simple(message).build())
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }
}
