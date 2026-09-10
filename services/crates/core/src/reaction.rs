//! Likes and comments from contributors and member households (F-710).

use crate::ids::{HouseholdId, MediaId, ReactionId, TripId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

pub const MAX_COMMENT_CHARS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum ReactionTarget {
    Day { index: u32 },
    Media { id: MediaId },
}

impl ReactionTarget {
    /// Stable key used for one-like-per-actor and sort keys.
    pub fn key(&self) -> String {
        match self {
            Self::Day { index } => format!("day:{index}"),
            Self::Media { id } => format!("media:{id}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReactionKind {
    Like,
    Comment { text: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reaction {
    pub id: ReactionId,
    /// The trip owner's household, where the reaction is stored.
    pub household: HouseholdId,
    pub trip: TripId,
    #[serde(flatten)]
    pub target: ReactionTarget,
    /// Contributor id or member household id.
    pub actor: String,
    pub actor_name: String,
    #[serde(flatten)]
    pub kind: ReactionKind,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReactionError {
    #[error("comment is empty")]
    EmptyComment,
    #[error("comment is longer than {MAX_COMMENT_CHARS} characters")]
    CommentTooLong,
}

impl Reaction {
    pub fn like(
        household: HouseholdId,
        trip: TripId,
        target: ReactionTarget,
        actor: impl Into<String>,
        actor_name: impl Into<String>,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            id: ReactionId::random(),
            household,
            trip,
            target,
            actor: actor.into(),
            actor_name: actor_name.into(),
            kind: ReactionKind::Like,
            created_at: now,
        }
    }

    pub fn comment(
        household: HouseholdId,
        trip: TripId,
        target: ReactionTarget,
        actor: impl Into<String>,
        actor_name: impl Into<String>,
        text: &str,
        now: OffsetDateTime,
    ) -> Result<Self, ReactionError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ReactionError::EmptyComment);
        }
        if text.chars().count() > MAX_COMMENT_CHARS {
            return Err(ReactionError::CommentTooLong);
        }
        let mut r = Self::like(household, trip, target, actor, actor_name, now);
        r.kind = ReactionKind::Comment {
            text: text.to_owned(),
        };
        Ok(r)
    }

    pub fn is_like(&self) -> bool {
        self.kind == ReactionKind::Like
    }

    /// One like per actor per target: two likes with the same key replace each other.
    pub fn like_key(&self) -> Option<String> {
        self.is_like()
            .then(|| format!("{}#{}#{}", self.trip, self.target.key(), self.actor))
    }

    /// Ticker line for the owner's TV.
    pub fn ticker_line(&self) -> String {
        match (&self.kind, &self.target) {
            (ReactionKind::Like, ReactionTarget::Day { index }) => {
                format!("{} loved day {index}", self.actor_name)
            }
            (ReactionKind::Like, ReactionTarget::Media { .. }) => {
                format!("{} loved a photo", self.actor_name)
            }
            (ReactionKind::Comment { text }, _) => format!("{}: {text}", self.actor_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn comments_are_bounded_and_likes_keyed() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let h = HouseholdId::new("h");
        let t = TripId::new("kyoto");
        let day3 = ReactionTarget::Day { index: 3 };
        assert_eq!(
            Reaction::comment(
                h.clone(),
                t.clone(),
                day3.clone(),
                "ruth",
                "Ruth",
                "  ",
                now
            ),
            Err(ReactionError::EmptyComment)
        );
        let long = "x".repeat(MAX_COMMENT_CHARS + 1);
        assert_eq!(
            Reaction::comment(
                h.clone(),
                t.clone(),
                day3.clone(),
                "ruth",
                "Ruth",
                &long,
                now
            ),
            Err(ReactionError::CommentTooLong)
        );
        let c = Reaction::comment(
            h.clone(),
            t.clone(),
            day3.clone(),
            "ruth",
            "Grandma Ruth",
            "That ramen!",
            now,
        )
        .unwrap();
        assert_eq!(c.like_key(), None);
        assert_eq!(c.ticker_line(), "Grandma Ruth: That ramen!");
        let l = Reaction::like(h, t, day3, "ruth", "Grandma Ruth", now);
        assert_eq!(l.like_key().as_deref(), Some("kyoto#day:3#ruth"));
        assert_eq!(l.ticker_line(), "Grandma Ruth loved day 3");
        let json = serde_json::to_value(&l).unwrap();
        assert_eq!(json["target"], "day");
        assert_eq!(json["kind"], "like");
        let back: Reaction = serde_json::from_value(json).unwrap();
        assert_eq!(back, l);
    }
}
