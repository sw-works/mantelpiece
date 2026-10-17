//! Trip reels and day films (F-713, F-714).
//!
//! A reel is a plan: the trip's photos in capture order, each held for whole bars of a
//! music track so every cut lands on a bar line, each with a crop window that moves from
//! `from` to `to` while it is on screen, plus a title, place labels grounded in the
//! metadata, and the music credit. The TV plays the plan live over the music (F-713); the
//! film Lambda draws the same plan into frames and encodes a day film (F-714).
//!
//! Everything here is deterministic: the same trip, media and track give the same plan and
//! the same frames. No model is called; the photos' quality scores, subject boxes and
//! landmarks come from ingest (constitution V).

pub mod library;
pub mod plan;
pub mod render;
pub mod schedule;
pub mod text;

pub use library::{library, Track};
pub use plan::{fingerprint, plan, Frame, PlanError, Reel, Request, Slide, Timed, Title, Window};
