//! End-of-day films (F-714): after organize, a one-time EventBridge Scheduler schedule per
//! recent trip day, named so a later organize run moves it rather than adding another. The
//! schedule invokes the film Lambda with `{household, trip, day}` and deletes itself.

use async_trait::async_trait;
use mantelpiece_core::ids::{HouseholdId, TripId};
use mantelpiece_core::media::MediaItem;
use mantelpiece_core::trip::Trip;
use mantelpiece_reels::schedule::{film_time, schedule_name};
use mantelpiece_store::{Result, StorageError};
use std::sync::Mutex;
use time::OffsetDateTime;

#[async_trait]
pub trait DayFilms: Send + Sync {
    /// Create or move the schedule `name` to fire once at `at` with `payload`.
    async fn upsert(&self, name: &str, at: OffsetDateTime, payload: &str) -> Result<()>;
}

/// Records schedules. For tests and for stacks without films.
#[derive(Debug, Default)]
pub struct MemoryDayFilms {
    pub schedules: Mutex<Vec<(String, OffsetDateTime, String)>>,
}

#[async_trait]
impl DayFilms for MemoryDayFilms {
    async fn upsert(&self, name: &str, at: OffsetDateTime, payload: &str) -> Result<()> {
        let mut s = self.schedules.lock().unwrap();
        s.retain(|(n, _, _)| n != name);
        s.push((name.to_owned(), at, payload.to_owned()));
        Ok(())
    }
}

/// EventBridge Scheduler backend. Compile-checked only.
#[derive(Debug, Clone)]
pub struct SchedulerDayFilms {
    client: aws_sdk_scheduler::Client,
    group: String,
    function_arn: String,
    role_arn: String,
}

impl SchedulerDayFilms {
    pub fn from_config(
        config: &aws_config::SdkConfig,
        group: impl Into<String>,
        function_arn: impl Into<String>,
        role_arn: impl Into<String>,
    ) -> Self {
        Self {
            client: aws_sdk_scheduler::Client::new(config),
            group: group.into(),
            function_arn: function_arn.into(),
            role_arn: role_arn.into(),
        }
    }
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

#[async_trait]
impl DayFilms for SchedulerDayFilms {
    async fn upsert(&self, name: &str, at: OffsetDateTime, payload: &str) -> Result<()> {
        use aws_sdk_scheduler::types::{
            ActionAfterCompletion, FlexibleTimeWindow, FlexibleTimeWindowMode, Target,
        };
        let when = at
            .format(time::macros::format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second]"
            ))
            .map_err(backend)?;
        let expression = format!("at({when})");
        let window = FlexibleTimeWindow::builder()
            .mode(FlexibleTimeWindowMode::Off)
            .build()
            .map_err(backend)?;
        let target = Target::builder()
            .arn(&self.function_arn)
            .role_arn(&self.role_arn)
            .input(payload)
            .build()
            .map_err(backend)?;
        let created = self
            .client
            .create_schedule()
            .name(name)
            .group_name(&self.group)
            .schedule_expression(&expression)
            .schedule_expression_timezone("UTC")
            .flexible_time_window(window.clone())
            .target(target.clone())
            .action_after_completion(ActionAfterCompletion::Delete)
            .send()
            .await;
        match created {
            Ok(_) => Ok(()),
            Err(e)
                if e.as_service_error().is_some_and(
                    aws_sdk_scheduler::operation::create_schedule::CreateScheduleError::is_conflict_exception,
                ) =>
            {
                self.client
                    .update_schedule()
                    .name(name)
                    .group_name(&self.group)
                    .schedule_expression(&expression)
                    .schedule_expression_timezone("UTC")
                    .flexible_time_window(window)
                    .target(target)
                    .action_after_completion(ActionAfterCompletion::Delete)
                    .send()
                    .await
                    .map_err(backend)?;
                Ok(())
            }
            Err(e) => Err(backend(e)),
        }
    }
}

/// Schedule (or move) the film of every recent day of the trips organize just built.
/// Failures are logged, not raised: films must never block organize.
pub async fn schedule_days(
    films: &dyn DayFilms,
    household: &HouseholdId,
    trips: &[Trip],
    media: &[MediaItem],
    now: OffsetDateTime,
) -> usize {
    let mut n = 0;
    for trip in trips {
        for day in &trip.days {
            let Some(at) = film_time(day, media, now) else {
                continue;
            };
            let name = schedule_name(household, &trip.id, day.index);
            let payload = payload(household, &trip.id, day.index);
            match films.upsert(&name, at, &payload).await {
                Ok(()) => n += 1,
                Err(e) => {
                    tracing::warn!(%household, trip = %trip.id, day = day.index, error = %e, "film schedule failed");
                }
            }
        }
    }
    n
}

fn payload(household: &HouseholdId, trip: &TripId, day: u32) -> String {
    serde_json::json!({ "household": household, "trip": trip, "day": day }).to_string()
}
