//! In-memory backends for tests and local runs.

use async_trait::async_trait;
use mantelpiece_core::audit::AuditEntry;
use mantelpiece_core::confirmation::ConfirmationToken;
use mantelpiece_core::contributor::Contributor;
use mantelpiece_core::device_code::DeviceCode;
use mantelpiece_core::events::{Event, EventKind};
use mantelpiece_core::household::Household;
use mantelpiece_core::ids::{
    ContributorId, HouseholdId, MediaId, PersonId, PlaylistId, ProposalId, ShareLinkId, TripId,
};
use mantelpiece_core::landmark::LandmarkFacts;
use mantelpiece_core::media::MediaItem;
use mantelpiece_core::membership::Membership;
use mantelpiece_core::person::Person;
use mantelpiece_core::presence::Presence;
use mantelpiece_core::proposal::Proposal;
use mantelpiece_core::reaction::Reaction;
use mantelpiece_core::share_link::ShareLink;
use mantelpiece_core::trip::{Playlist, Trip};
use mantelpiece_core::trip_code::TripCode;
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use time::OffsetDateTime;

use crate::blobs::{Blobs, CompletedPart};
use crate::storage::Storage;
use crate::{Result, StorageError};

#[derive(Debug, Default)]
struct Tables {
    device_codes: HashMap<String, DeviceCode>,
    landmarks: HashMap<String, LandmarkFacts>,
    trip_codes: HashMap<String, TripCode>,
    contributors: HashMap<(HouseholdId, ContributorId), Contributor>,
    media: HashMap<(HouseholdId, MediaId), MediaItem>,
    trips: HashMap<(HouseholdId, TripId), Trip>,
    events: BTreeMap<(HouseholdId, u64), Event>,
    seq: HashMap<HouseholdId, u64>,
    confirmations: HashMap<String, ConfirmationToken>,
    households: HashMap<HouseholdId, Household>,
    memberships: HashMap<(HouseholdId, HouseholdId, TripId), Membership>,
    presence: HashMap<(HouseholdId, String), Presence>,
    audit: Vec<AuditEntry>,
    proposals: HashMap<(HouseholdId, ProposalId), Proposal>,
    /// Keyed by like key for likes, by id for comments.
    reactions: HashMap<(HouseholdId, String), Reaction>,
    people: HashMap<(HouseholdId, PersonId), Person>,
    share_links: HashMap<ShareLinkId, ShareLink>,
    playlists: HashMap<(HouseholdId, PlaylistId), Playlist>,
}

#[derive(Debug, Default)]
pub struct MemoryStorage {
    tables: Mutex<Tables>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn put_household(&self, h: &Household) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .households
            .insert(h.id.clone(), h.clone());
        Ok(())
    }
    async fn get_household(&self, id: &HouseholdId) -> Result<Option<Household>> {
        Ok(self.tables.lock().unwrap().households.get(id).cloned())
    }
    async fn purge_household(&self, id: &HouseholdId) -> Result<()> {
        let mut t = self.tables.lock().unwrap();
        t.trip_codes.retain(|_, c| &c.household != id);
        t.contributors.retain(|(h, _), _| h != id);
        t.media.retain(|(h, _), _| h != id);
        t.trips.retain(|(h, _), _| h != id);
        t.events.retain(|(h, _), _| h != id);
        t.seq.remove(id);
        t.confirmations.retain(|_, c| &c.household != id);
        t.memberships.retain(|(m, o, _), _| m != id && o != id);
        t.presence.retain(|(h, _), _| h != id);
        t.audit.retain(|a| &a.household != id);
        t.proposals.retain(|(h, _), _| h != id);
        t.reactions.retain(|(h, _), _| h != id);
        t.people.retain(|(h, _), _| h != id);
        t.share_links.retain(|_, l| &l.household != id);
        t.playlists.retain(|(h, _), _| h != id);
        Ok(())
    }

    async fn put_device_code(&self, code: &DeviceCode) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .device_codes
            .insert(code.code.clone(), code.clone());
        Ok(())
    }
    async fn get_device_code(&self, code: &str) -> Result<Option<DeviceCode>> {
        Ok(self.tables.lock().unwrap().device_codes.get(code).cloned())
    }

    async fn put_landmark_facts(&self, facts: &LandmarkFacts) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .landmarks
            .insert(facts.key.clone(), facts.clone());
        Ok(())
    }
    async fn get_landmark_facts(&self, key: &str) -> Result<Option<LandmarkFacts>> {
        Ok(self.tables.lock().unwrap().landmarks.get(key).cloned())
    }

    async fn put_trip_code(&self, code: &TripCode) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .trip_codes
            .insert(code.code.clone(), code.clone());
        Ok(())
    }
    async fn get_trip_code(&self, code: &str) -> Result<Option<TripCode>> {
        Ok(self.tables.lock().unwrap().trip_codes.get(code).cloned())
    }
    async fn list_trip_codes(&self, household: &HouseholdId) -> Result<Vec<TripCode>> {
        let t = self.tables.lock().unwrap();
        Ok(t.trip_codes
            .values()
            .filter(|c| &c.household == household)
            .cloned()
            .collect())
    }

    async fn put_contributor(&self, c: &Contributor) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .contributors
            .insert((c.household.clone(), c.id.clone()), c.clone());
        Ok(())
    }
    async fn get_contributor(
        &self,
        household: &HouseholdId,
        id: &ContributorId,
    ) -> Result<Option<Contributor>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .contributors
            .get(&(household.clone(), id.clone()))
            .cloned())
    }
    async fn list_contributors(&self, household: &HouseholdId) -> Result<Vec<Contributor>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<Contributor> = t
            .contributors
            .values()
            .filter(|c| &c.household == household)
            .cloned()
            .collect();
        v.sort_by_key(|c| c.joined_at);
        Ok(v)
    }

    async fn put_media(&self, m: &MediaItem) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .media
            .insert((m.household.clone(), m.id.clone()), m.clone());
        Ok(())
    }
    async fn get_media(&self, household: &HouseholdId, id: &MediaId) -> Result<Option<MediaItem>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .media
            .get(&(household.clone(), id.clone()))
            .cloned())
    }
    async fn list_media(&self, household: &HouseholdId) -> Result<Vec<MediaItem>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<MediaItem> = t
            .media
            .values()
            .filter(|m| &m.household == household)
            .cloned()
            .collect();
        v.sort_by_key(|m| (m.uploaded_at, m.id.clone()));
        Ok(v)
    }
    async fn find_media_by_sha256(
        &self,
        household: &HouseholdId,
        sha256: &str,
    ) -> Result<Option<MediaItem>> {
        let t = self.tables.lock().unwrap();
        Ok(t.media
            .values()
            .find(|m| &m.household == household && m.sha256 == sha256)
            .cloned())
    }
    async fn list_media_by_contributor(
        &self,
        household: &HouseholdId,
        contributor: &ContributorId,
    ) -> Result<Vec<MediaItem>> {
        let t = self.tables.lock().unwrap();
        Ok(t.media
            .values()
            .filter(|m| &m.household == household && &m.contributor == contributor)
            .cloned()
            .collect())
    }
    async fn delete_media(&self, household: &HouseholdId, id: &MediaId) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .media
            .remove(&(household.clone(), id.clone()));
        Ok(())
    }

    async fn put_trip(&self, trip: &Trip) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .trips
            .insert((trip.household.clone(), trip.id.clone()), trip.clone());
        Ok(())
    }
    async fn get_trip(&self, household: &HouseholdId, id: &TripId) -> Result<Option<Trip>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .trips
            .get(&(household.clone(), id.clone()))
            .cloned())
    }
    async fn list_trips(&self, household: &HouseholdId) -> Result<Vec<Trip>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<Trip> = t
            .trips
            .values()
            .filter(|t| &t.household == household)
            .cloned()
            .collect();
        v.sort_by_key(|t| std::cmp::Reverse(t.start));
        Ok(v)
    }

    async fn append_event(
        &self,
        household: &HouseholdId,
        at: OffsetDateTime,
        kind: EventKind,
    ) -> Result<Event> {
        let mut t = self.tables.lock().unwrap();
        let seq = t.seq.entry(household.clone()).or_default();
        *seq += 1;
        let event = Event {
            household: household.clone(),
            seq: *seq,
            at,
            kind,
        };
        t.events
            .insert((household.clone(), event.seq), event.clone());
        Ok(event)
    }
    async fn events_since(
        &self,
        household: &HouseholdId,
        since: u64,
        limit: usize,
    ) -> Result<Vec<Event>> {
        let t = self.tables.lock().unwrap();
        Ok(t.events
            .range((household.clone(), since + 1)..(household.clone(), u64::MAX))
            .map(|(_, e)| e.clone())
            .take(limit)
            .collect())
    }

    async fn put_confirmation(&self, token: &ConfirmationToken) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .confirmations
            .insert(token.token.clone(), token.clone());
        Ok(())
    }
    async fn get_confirmation(&self, token: &str) -> Result<Option<ConfirmationToken>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .confirmations
            .get(token)
            .cloned())
    }

    async fn put_membership(&self, m: &Membership) -> Result<()> {
        self.tables.lock().unwrap().memberships.insert(
            (
                m.member_household.clone(),
                m.owner_household.clone(),
                m.trip.clone(),
            ),
            m.clone(),
        );
        Ok(())
    }
    async fn get_membership(
        &self,
        member: &HouseholdId,
        owner: &HouseholdId,
        trip: &TripId,
    ) -> Result<Option<Membership>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .memberships
            .get(&(member.clone(), owner.clone(), trip.clone()))
            .cloned())
    }
    async fn list_memberships(&self, member: &HouseholdId) -> Result<Vec<Membership>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<Membership> = t
            .memberships
            .values()
            .filter(|m| &m.member_household == member)
            .cloned()
            .collect();
        v.sort_by_key(|m| m.joined_at);
        Ok(v)
    }
    async fn list_trip_members(
        &self,
        owner: &HouseholdId,
        trip: &TripId,
    ) -> Result<Vec<Membership>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<Membership> = t
            .memberships
            .values()
            .filter(|m| &m.owner_household == owner && &m.trip == trip)
            .cloned()
            .collect();
        v.sort_by_key(|m| m.joined_at);
        Ok(v)
    }

    async fn put_presence(&self, p: &Presence) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .presence
            .insert((p.household.clone(), p.device_id.to_string()), p.clone());
        Ok(())
    }
    async fn list_presence(&self, household: &HouseholdId) -> Result<Vec<Presence>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<Presence> = t
            .presence
            .values()
            .filter(|p| &p.household == household)
            .cloned()
            .collect();
        v.sort_by_key(|p| p.device_id.to_string());
        Ok(v)
    }

    async fn put_proposal(&self, p: &Proposal) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .proposals
            .insert((p.household.clone(), p.id.clone()), p.clone());
        Ok(())
    }
    async fn get_proposal(
        &self,
        household: &HouseholdId,
        id: &ProposalId,
    ) -> Result<Option<Proposal>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .proposals
            .get(&(household.clone(), id.clone()))
            .cloned())
    }
    async fn list_proposals(&self, household: &HouseholdId) -> Result<Vec<Proposal>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<Proposal> = t
            .proposals
            .values()
            .filter(|p| &p.household == household)
            .cloned()
            .collect();
        v.sort_by_key(|p| (p.opened_at, p.id.clone()));
        Ok(v)
    }

    async fn put_reaction(&self, r: &Reaction) -> Result<()> {
        let key = r.like_key().unwrap_or_else(|| r.id.to_string());
        self.tables
            .lock()
            .unwrap()
            .reactions
            .insert((r.household.clone(), key), r.clone());
        Ok(())
    }
    async fn list_reactions(
        &self,
        household: &HouseholdId,
        trip: &TripId,
    ) -> Result<Vec<Reaction>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<Reaction> = t
            .reactions
            .values()
            .filter(|r| &r.household == household && &r.trip == trip)
            .cloned()
            .collect();
        v.sort_by_key(|r| (r.created_at, r.id.clone()));
        Ok(v)
    }

    async fn put_person(&self, p: &Person) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .people
            .insert((p.household.clone(), p.id.clone()), p.clone());
        Ok(())
    }
    async fn get_person(&self, household: &HouseholdId, id: &PersonId) -> Result<Option<Person>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .people
            .get(&(household.clone(), id.clone()))
            .cloned())
    }
    async fn list_people(&self, household: &HouseholdId) -> Result<Vec<Person>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<Person> = t
            .people
            .values()
            .filter(|p| &p.household == household)
            .cloned()
            .collect();
        v.sort_by_key(|p| (std::cmp::Reverse(p.media_count), p.id.clone()));
        Ok(v)
    }
    async fn delete_people(&self, household: &HouseholdId) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .people
            .retain(|(h, _), _| h != household);
        Ok(())
    }

    async fn put_audit(&self, entry: &AuditEntry) -> Result<()> {
        self.tables.lock().unwrap().audit.push(entry.clone());
        Ok(())
    }
    async fn list_audit(&self, household: &HouseholdId) -> Result<Vec<AuditEntry>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<AuditEntry> = t
            .audit
            .iter()
            .filter(|a| &a.household == household)
            .cloned()
            .collect();
        // Newest first; a stable sort keeps insertion order among equal timestamps once
        // the list is reversed, which matters under a frozen test clock.
        v.reverse();
        v.sort_by(|a, b| b.at.cmp(&a.at));
        Ok(v)
    }

    async fn put_share_link(&self, l: &ShareLink) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .share_links
            .insert(l.id.clone(), l.clone());
        Ok(())
    }
    async fn get_share_link(&self, id: &ShareLinkId) -> Result<Option<ShareLink>> {
        Ok(self.tables.lock().unwrap().share_links.get(id).cloned())
    }
    async fn list_share_links(&self, household: &HouseholdId) -> Result<Vec<ShareLink>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<ShareLink> = t
            .share_links
            .values()
            .filter(|l| &l.household == household)
            .cloned()
            .collect();
        v.sort_by_key(|l| (l.created_at, l.id.clone()));
        Ok(v)
    }

    async fn put_playlist(&self, p: &Playlist) -> Result<()> {
        self.tables
            .lock()
            .unwrap()
            .playlists
            .insert((p.household.clone(), p.id.clone()), p.clone());
        Ok(())
    }
    async fn get_playlist(
        &self,
        household: &HouseholdId,
        id: &PlaylistId,
    ) -> Result<Option<Playlist>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .playlists
            .get(&(household.clone(), id.clone()))
            .cloned())
    }
    async fn list_playlists(&self, household: &HouseholdId) -> Result<Vec<Playlist>> {
        let t = self.tables.lock().unwrap();
        let mut v: Vec<Playlist> = t
            .playlists
            .values()
            .filter(|p| &p.household == household)
            .cloned()
            .collect();
        v.sort_by_key(|p| (std::cmp::Reverse(p.created_at), p.id.clone()));
        Ok(v)
    }
}

/// Records every call so tests can assert on S3 interactions.
#[derive(Debug, Default)]
pub struct MemoryBlobs {
    pub log: Mutex<Vec<String>>,
    pub objects: Mutex<HashMap<String, Vec<u8>>>,
}

#[async_trait]
impl Blobs for MemoryBlobs {
    async fn create_multipart(&self, key: &str, content_type: &str) -> Result<String> {
        self.log
            .lock()
            .unwrap()
            .push(format!("create {key} {content_type}"));
        Ok(format!("upload-{}", key.len()))
    }
    async fn presign_part(&self, key: &str, upload_id: &str, part_number: i32) -> Result<String> {
        Ok(format!(
            "https://blobs.test/{key}?uploadId={upload_id}&partNumber={part_number}"
        ))
    }
    async fn complete_multipart(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[CompletedPart],
    ) -> Result<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("complete {key} {upload_id} {}", parts.len()));
        Ok(())
    }
    async fn abort_multipart(&self, key: &str, upload_id: &str) -> Result<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("abort {key} {upload_id}"));
        Ok(())
    }
    async fn presign_get(&self, key: &str) -> Result<String> {
        Ok(format!("https://blobs.test/{key}"))
    }
    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.objects
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| StorageError::NotFound(key.to_owned()))
    }
    async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> Result<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("put {key} {content_type} {}", bytes.len()));
        self.objects.lock().unwrap().insert(key.to_owned(), bytes);
        Ok(())
    }
    async fn delete(&self, keys: &[String]) -> Result<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("delete {}", keys.join(",")));
        let mut objects = self.objects.lock().unwrap();
        for k in keys {
            objects.remove(k);
        }
        Ok(())
    }
}
