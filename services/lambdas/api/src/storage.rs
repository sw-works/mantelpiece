//! Persistence behind the API. One trait, two implementations: DynamoDB for Lambda and
//! an in-memory map for tests.

use async_trait::async_trait;
use memorylane_core::confirmation::ConfirmationToken;
use memorylane_core::contributor::Contributor;
use memorylane_core::device_code::DeviceCode;
use memorylane_core::events::{Event, EventKind};
use memorylane_core::ids::{ContributorId, HouseholdId, MediaId, TripId};
use memorylane_core::media::MediaItem;
use memorylane_core::trip::Trip;
use memorylane_core::trip_code::TripCode;
use time::OffsetDateTime;

use crate::error::StorageError;

pub type Result<T> = std::result::Result<T, StorageError>;

#[async_trait]
pub trait Storage: Send + Sync {
    async fn put_device_code(&self, code: &DeviceCode) -> Result<()>;
    async fn get_device_code(&self, code: &str) -> Result<Option<DeviceCode>>;

    async fn put_trip_code(&self, code: &TripCode) -> Result<()>;
    async fn get_trip_code(&self, code: &str) -> Result<Option<TripCode>>;
    async fn list_trip_codes(&self, household: &HouseholdId) -> Result<Vec<TripCode>>;

    async fn put_contributor(&self, contributor: &Contributor) -> Result<()>;
    async fn get_contributor(
        &self,
        household: &HouseholdId,
        id: &ContributorId,
    ) -> Result<Option<Contributor>>;
    async fn list_contributors(&self, household: &HouseholdId) -> Result<Vec<Contributor>>;

    async fn put_media(&self, media: &MediaItem) -> Result<()>;
    async fn get_media(&self, household: &HouseholdId, id: &MediaId) -> Result<Option<MediaItem>>;
    async fn find_media_by_sha256(
        &self,
        household: &HouseholdId,
        sha256: &str,
    ) -> Result<Option<MediaItem>>;
    async fn list_media_by_contributor(
        &self,
        household: &HouseholdId,
        contributor: &ContributorId,
    ) -> Result<Vec<MediaItem>>;
    async fn delete_media(&self, household: &HouseholdId, id: &MediaId) -> Result<()>;

    async fn put_trip(&self, trip: &Trip) -> Result<()>;
    async fn get_trip(&self, household: &HouseholdId, id: &TripId) -> Result<Option<Trip>>;
    async fn list_trips(&self, household: &HouseholdId) -> Result<Vec<Trip>>;

    /// Assign the next sequence number for the household and persist the event.
    async fn append_event(
        &self,
        household: &HouseholdId,
        at: OffsetDateTime,
        kind: EventKind,
    ) -> Result<Event>;
    async fn events_since(
        &self,
        household: &HouseholdId,
        since: u64,
        limit: usize,
    ) -> Result<Vec<Event>>;

    async fn put_confirmation(&self, token: &ConfirmationToken) -> Result<()>;
    async fn get_confirmation(&self, token: &str) -> Result<Option<ConfirmationToken>>;
}
