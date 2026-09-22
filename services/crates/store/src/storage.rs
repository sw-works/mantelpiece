//! Persistence behind the API. One trait, two implementations: DynamoDB for Lambda and
//! an in-memory map for tests.

use async_trait::async_trait;
use memorylane_core::audit::AuditEntry;
use memorylane_core::confirmation::ConfirmationToken;
use memorylane_core::contributor::Contributor;
use memorylane_core::device_code::DeviceCode;
use memorylane_core::events::{Event, EventKind};
use memorylane_core::household::Household;
use memorylane_core::ids::{
    ContributorId, HouseholdId, MediaId, PersonId, PlaylistId, ProposalId, ShareLinkId, TripId,
};
use memorylane_core::landmark::LandmarkFacts;
use memorylane_core::media::MediaItem;
use memorylane_core::membership::Membership;
use memorylane_core::person::Person;
use memorylane_core::presence::Presence;
use memorylane_core::proposal::Proposal;
use memorylane_core::reaction::Reaction;
use memorylane_core::share_link::ShareLink;
use memorylane_core::trip::{Playlist, Trip};
use memorylane_core::trip_code::TripCode;
use time::OffsetDateTime;

use crate::Result;

#[async_trait]
pub trait Storage: Send + Sync {
    async fn put_household(&self, household: &Household) -> Result<()>;
    async fn get_household(&self, id: &HouseholdId) -> Result<Option<Household>>;
    /// Account deletion (F-604): every record under the household except the household
    /// row itself, which keeps `deleting_since` so late uploads answer 410.
    async fn purge_household(&self, id: &HouseholdId) -> Result<()>;

    async fn put_device_code(&self, code: &DeviceCode) -> Result<()>;
    async fn get_device_code(&self, code: &str) -> Result<Option<DeviceCode>>;

    /// Landmark facts are public knowledge, shared by every household (F-210).
    async fn put_landmark_facts(&self, facts: &LandmarkFacts) -> Result<()>;
    async fn get_landmark_facts(&self, key: &str) -> Result<Option<LandmarkFacts>>;

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
    async fn list_media(&self, household: &HouseholdId) -> Result<Vec<MediaItem>>;
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

    // Memberships (F-162): written under the member's partition with a reverse pointer
    // under the owner's, so both TVs can list them without a scan.
    async fn put_membership(&self, membership: &Membership) -> Result<()>;
    async fn get_membership(
        &self,
        member: &HouseholdId,
        owner: &HouseholdId,
        trip: &TripId,
    ) -> Result<Option<Membership>>;
    /// Trips this household joined, active or not.
    async fn list_memberships(&self, member: &HouseholdId) -> Result<Vec<Membership>>;
    /// Households that joined this trip of `owner`.
    async fn list_trip_members(
        &self,
        owner: &HouseholdId,
        trip: &TripId,
    ) -> Result<Vec<Membership>>;

    async fn put_presence(&self, presence: &Presence) -> Result<()>;
    async fn list_presence(&self, household: &HouseholdId) -> Result<Vec<Presence>>;

    async fn put_proposal(&self, proposal: &Proposal) -> Result<()>;
    async fn get_proposal(
        &self,
        household: &HouseholdId,
        id: &ProposalId,
    ) -> Result<Option<Proposal>>;
    async fn list_proposals(&self, household: &HouseholdId) -> Result<Vec<Proposal>>;

    /// A like replaces an earlier like by the same actor on the same target; comments
    /// accumulate.
    async fn put_reaction(&self, reaction: &Reaction) -> Result<()>;
    async fn list_reactions(&self, household: &HouseholdId, trip: &TripId)
        -> Result<Vec<Reaction>>;

    async fn put_person(&self, person: &Person) -> Result<()>;
    async fn get_person(&self, household: &HouseholdId, id: &PersonId) -> Result<Option<Person>>;
    async fn list_people(&self, household: &HouseholdId) -> Result<Vec<Person>>;
    async fn delete_people(&self, household: &HouseholdId) -> Result<()>;

    async fn put_share_link(&self, link: &ShareLink) -> Result<()>;
    /// Global lookup for the public share page.
    async fn get_share_link(&self, id: &ShareLinkId) -> Result<Option<ShareLink>>;
    async fn list_share_links(&self, household: &HouseholdId) -> Result<Vec<ShareLink>>;

    async fn put_audit(&self, entry: &AuditEntry) -> Result<()>;
    /// Newest first.
    async fn list_audit(&self, household: &HouseholdId) -> Result<Vec<AuditEntry>>;

    async fn put_playlist(&self, playlist: &Playlist) -> Result<()>;
    async fn get_playlist(
        &self,
        household: &HouseholdId,
        id: &PlaylistId,
    ) -> Result<Option<Playlist>>;
    async fn list_playlists(&self, household: &HouseholdId) -> Result<Vec<Playlist>>;
}
