//! DynamoDB backend. Single table, `pk`/`sk` keys, one GSI (`gsi1pk`/`gsi1sk`) for
//! per-household lookups of codes and exact-duplicate checks, and a TTL attribute.
//!
//! Sort keys under `H#<household>`: `HH` household, `C#` contributor, `M#` media, `T#`
//! trip, `MB#<owner>#<trip>` membership (member side), `SH#<trip>#<member>` membership
//! (owner side), `TV#<device>` presence, `P#` proposal, `R#<trip>#<target>#<actor>#<id|like>`
//! reaction, `PE#` person, `PL#` playlist. Share links live under `SL#<id>` for the public
//! page with a GSI pointer from the household.
//!
//! Written against the SDK types and compile-checked; not yet exercised against a real
//! table (no AWS account on the development machine).

use async_trait::async_trait;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
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
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::storage::Storage;
use crate::{Result, StorageError};

#[derive(Debug, Clone)]
pub struct DynamoStorage {
    client: Client,
    table: String,
}

#[derive(Serialize, Deserialize)]
struct Row<T> {
    pk: String,
    sk: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    gsi1pk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gsi1sk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl: Option<i64>,
    data: T,
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

impl DynamoStorage {
    pub fn from_config(config: &aws_config::SdkConfig, table: impl Into<String>) -> Self {
        Self::new(Client::new(config), table)
    }

    pub fn new(client: Client, table: impl Into<String>) -> Self {
        Self {
            client,
            table: table.into(),
        }
    }

    async fn put<T: Serialize + Send + Sync>(&self, row: Row<T>) -> Result<()> {
        let item = serde_dynamo::to_item(row).map_err(|e| StorageError::Serde(e.to_string()))?;
        self.client
            .put_item()
            .table_name(&self.table)
            .set_item(Some(item))
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn get<T: DeserializeOwned>(&self, pk: String, sk: String) -> Result<Option<T>> {
        let out = self
            .client
            .get_item()
            .table_name(&self.table)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(sk))
            .send()
            .await
            .map_err(backend)?;
        match out.item {
            None => Ok(None),
            Some(item) => {
                let row: Row<T> = serde_dynamo::from_item(item)
                    .map_err(|e| StorageError::Serde(e.to_string()))?;
                Ok(Some(row.data))
            }
        }
    }

    async fn query<T: DeserializeOwned>(
        &self,
        pk: String,
        sk_prefix: &str,
        index: Option<&str>,
        limit: Option<i32>,
    ) -> Result<Vec<T>> {
        let (pk_name, sk_name) = if index.is_some() {
            ("gsi1pk", "gsi1sk")
        } else {
            ("pk", "sk")
        };
        let mut q = self
            .client
            .query()
            .table_name(&self.table)
            .key_condition_expression("#pk = :pk AND begins_with(#sk, :prefix)")
            .expression_attribute_names("#pk", pk_name)
            .expression_attribute_names("#sk", sk_name)
            .expression_attribute_values(":pk", AttributeValue::S(pk))
            .expression_attribute_values(":prefix", AttributeValue::S(sk_prefix.to_owned()));
        if let Some(i) = index {
            q = q.index_name(i);
        }
        if let Some(l) = limit {
            q = q.limit(l);
        }
        let out = q.send().await.map_err(backend)?;
        out.items
            .unwrap_or_default()
            .into_iter()
            .map(|item| {
                serde_dynamo::from_item::<_, Row<T>>(item)
                    .map(|r| r.data)
                    .map_err(|e| StorageError::Serde(e.to_string()))
            })
            .collect()
    }

    /// Whole rows (keys included) under a partition, for deletions.
    async fn query_rows<T: DeserializeOwned>(
        &self,
        pk: String,
        sk_prefix: &str,
    ) -> Result<Vec<Row<T>>> {
        let out = self
            .client
            .query()
            .table_name(&self.table)
            .key_condition_expression("#pk = :pk AND begins_with(#sk, :prefix)")
            .expression_attribute_names("#pk", "pk")
            .expression_attribute_names("#sk", "sk")
            .expression_attribute_values(":pk", AttributeValue::S(pk))
            .expression_attribute_values(":prefix", AttributeValue::S(sk_prefix.to_owned()))
            .send()
            .await
            .map_err(backend)?;
        out.items
            .unwrap_or_default()
            .into_iter()
            .map(|item| {
                serde_dynamo::from_item::<_, Row<T>>(item)
                    .map_err(|e| StorageError::Serde(e.to_string()))
            })
            .collect()
    }

    async fn delete(&self, pk: String, sk: String) -> Result<()> {
        self.client
            .delete_item()
            .table_name(&self.table)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(sk))
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn next_seq(&self, household: &HouseholdId) -> Result<u64> {
        let out = self
            .client
            .update_item()
            .table_name(&self.table)
            .key("pk", AttributeValue::S(format!("H#{household}")))
            .key("sk", AttributeValue::S("SEQ".into()))
            .update_expression("ADD seq :one")
            .expression_attribute_values(":one", AttributeValue::N("1".into()))
            .return_values(aws_sdk_dynamodb::types::ReturnValue::UpdatedNew)
            .send()
            .await
            .map_err(backend)?;
        out.attributes
            .and_then(|a| {
                a.get("seq")
                    .and_then(|v| v.as_n().ok().and_then(|n| n.parse().ok()))
            })
            .ok_or_else(|| StorageError::Backend("seq counter missing".into()))
    }
}

fn household_pk(h: &HouseholdId) -> String {
    format!("H#{h}")
}

#[async_trait]
impl Storage for DynamoStorage {
    async fn put_household(&self, h: &Household) -> Result<()> {
        self.put(Row {
            pk: household_pk(&h.id),
            sk: "HH".into(),
            gsi1pk: None,
            gsi1sk: None,
            ttl: None,
            data: h,
        })
        .await
    }
    async fn get_household(&self, id: &HouseholdId) -> Result<Option<Household>> {
        self.get(household_pk(id), "HH".into()).await
    }
    async fn purge_household(&self, id: &HouseholdId) -> Result<()> {
        // Every row under the household partition except the household row itself.
        let rows: Vec<Row<serde_json::Value>> = self.query_rows(household_pk(id), "").await?;
        for row in rows {
            if row.sk != "HH" {
                self.delete(row.pk, row.sk).await?;
            }
        }
        // Rows keyed elsewhere that point at the household.
        for code in self.list_trip_codes(id).await? {
            self.delete(format!("TC#{}", code.code), "TC".into())
                .await?;
        }
        for link in self.list_share_links(id).await? {
            self.delete(format!("SL#{}", link.id), "SL".into()).await?;
        }
        let events: Vec<Row<serde_json::Value>> = self.query_rows(format!("E#{id}"), "").await?;
        for row in events {
            self.delete(row.pk, row.sk).await?;
        }
        Ok(())
    }

    async fn put_device_code(&self, code: &DeviceCode) -> Result<()> {
        self.put(Row {
            pk: format!("DC#{}", code.code),
            sk: "DC".into(),
            gsi1pk: None,
            gsi1sk: None,
            ttl: Some(code.expires_at.unix_timestamp() + 3600),
            data: code,
        })
        .await
    }
    async fn get_device_code(&self, code: &str) -> Result<Option<DeviceCode>> {
        self.get(format!("DC#{code}"), "DC".into()).await
    }

    async fn put_landmark_facts(&self, facts: &LandmarkFacts) -> Result<()> {
        self.put(Row {
            pk: format!("LM#{}", facts.key),
            sk: "LM".into(),
            gsi1pk: None,
            gsi1sk: None,
            ttl: None,
            data: facts,
        })
        .await
    }
    async fn get_landmark_facts(&self, key: &str) -> Result<Option<LandmarkFacts>> {
        self.get(format!("LM#{key}"), "LM".into()).await
    }

    async fn put_trip_code(&self, code: &TripCode) -> Result<()> {
        self.put(Row {
            pk: format!("TC#{}", code.code),
            sk: "TC".into(),
            gsi1pk: Some(household_pk(&code.household)),
            gsi1sk: Some(format!("TC#{}", code.code)),
            ttl: None,
            data: code,
        })
        .await
    }
    async fn get_trip_code(&self, code: &str) -> Result<Option<TripCode>> {
        self.get(format!("TC#{code}"), "TC".into()).await
    }
    async fn list_trip_codes(&self, household: &HouseholdId) -> Result<Vec<TripCode>> {
        self.query(household_pk(household), "TC#", Some("gsi1"), None)
            .await
    }

    async fn put_contributor(&self, c: &Contributor) -> Result<()> {
        self.put(Row {
            pk: household_pk(&c.household),
            sk: format!("C#{}", c.id),
            gsi1pk: None,
            gsi1sk: None,
            ttl: None,
            data: c,
        })
        .await
    }
    async fn get_contributor(
        &self,
        household: &HouseholdId,
        id: &ContributorId,
    ) -> Result<Option<Contributor>> {
        self.get(household_pk(household), format!("C#{id}")).await
    }
    async fn list_contributors(&self, household: &HouseholdId) -> Result<Vec<Contributor>> {
        self.query(household_pk(household), "C#", None, None).await
    }

    async fn put_media(&self, m: &MediaItem) -> Result<()> {
        self.put(Row {
            pk: household_pk(&m.household),
            sk: format!("M#{}", m.id),
            gsi1pk: Some(format!("SHA#{}", m.household)),
            gsi1sk: Some(m.sha256.clone()),
            ttl: None,
            data: m,
        })
        .await
    }
    async fn get_media(&self, household: &HouseholdId, id: &MediaId) -> Result<Option<MediaItem>> {
        self.get(household_pk(household), format!("M#{id}")).await
    }
    async fn list_media(&self, household: &HouseholdId) -> Result<Vec<MediaItem>> {
        self.query(household_pk(household), "M#", None, None).await
    }
    async fn find_media_by_sha256(
        &self,
        household: &HouseholdId,
        sha256: &str,
    ) -> Result<Option<MediaItem>> {
        let mut v: Vec<MediaItem> = self
            .query(format!("SHA#{household}"), sha256, Some("gsi1"), Some(1))
            .await?;
        Ok(v.pop())
    }
    async fn list_media_by_contributor(
        &self,
        household: &HouseholdId,
        contributor: &ContributorId,
    ) -> Result<Vec<MediaItem>> {
        let all: Vec<MediaItem> = self
            .query(household_pk(household), "M#", None, None)
            .await?;
        Ok(all
            .into_iter()
            .filter(|m| &m.contributor == contributor)
            .collect())
    }
    async fn delete_media(&self, household: &HouseholdId, id: &MediaId) -> Result<()> {
        self.delete(household_pk(household), format!("M#{id}"))
            .await
    }

    async fn put_trip(&self, t: &Trip) -> Result<()> {
        self.put(Row {
            pk: household_pk(&t.household),
            sk: format!("T#{}", t.id),
            gsi1pk: None,
            gsi1sk: None,
            ttl: None,
            data: t,
        })
        .await
    }
    async fn get_trip(&self, household: &HouseholdId, id: &TripId) -> Result<Option<Trip>> {
        self.get(household_pk(household), format!("T#{id}")).await
    }
    async fn list_trips(&self, household: &HouseholdId) -> Result<Vec<Trip>> {
        let mut v: Vec<Trip> = self
            .query(household_pk(household), "T#", None, None)
            .await?;
        v.sort_by_key(|t| std::cmp::Reverse(t.start));
        Ok(v)
    }

    async fn append_event(
        &self,
        household: &HouseholdId,
        at: OffsetDateTime,
        kind: EventKind,
    ) -> Result<Event> {
        let seq = self.next_seq(household).await?;
        let event = Event {
            household: household.clone(),
            seq,
            at,
            kind,
        };
        self.put(Row {
            pk: format!("E#{household}"),
            sk: format!("{seq:012}"),
            gsi1pk: None,
            gsi1sk: None,
            ttl: Some(at.unix_timestamp() + 30 * 24 * 3600),
            data: &event,
        })
        .await?;
        Ok(event)
    }
    async fn events_since(
        &self,
        household: &HouseholdId,
        since: u64,
        limit: usize,
    ) -> Result<Vec<Event>> {
        let out = self
            .client
            .query()
            .table_name(&self.table)
            .key_condition_expression("pk = :pk AND sk > :since")
            .expression_attribute_values(":pk", AttributeValue::S(format!("E#{household}")))
            .expression_attribute_values(":since", AttributeValue::S(format!("{since:012}")))
            .limit(i32::try_from(limit).unwrap_or(i32::MAX))
            .send()
            .await
            .map_err(backend)?;
        out.items
            .unwrap_or_default()
            .into_iter()
            .map(|item| {
                serde_dynamo::from_item::<_, Row<Event>>(item)
                    .map(|r| r.data)
                    .map_err(|e| StorageError::Serde(e.to_string()))
            })
            .collect()
    }

    async fn put_confirmation(&self, token: &ConfirmationToken) -> Result<()> {
        self.put(Row {
            pk: format!("CF#{}", token.token),
            sk: "CF".into(),
            gsi1pk: None,
            gsi1sk: None,
            ttl: Some(token.expires_at.unix_timestamp() + 3600),
            data: token,
        })
        .await
    }
    async fn get_confirmation(&self, token: &str) -> Result<Option<ConfirmationToken>> {
        self.get(format!("CF#{token}"), "CF".into()).await
    }

    async fn put_membership(&self, m: &Membership) -> Result<()> {
        self.put(Row {
            pk: household_pk(&m.member_household),
            sk: format!("MB#{}#{}", m.owner_household, m.trip),
            gsi1pk: None,
            gsi1sk: None,
            ttl: None,
            data: m,
        })
        .await?;
        self.put(Row {
            pk: household_pk(&m.owner_household),
            sk: format!("SH#{}#{}", m.trip, m.member_household),
            gsi1pk: None,
            gsi1sk: None,
            ttl: None,
            data: m,
        })
        .await
    }
    async fn get_membership(
        &self,
        member: &HouseholdId,
        owner: &HouseholdId,
        trip: &TripId,
    ) -> Result<Option<Membership>> {
        self.get(household_pk(member), format!("MB#{owner}#{trip}"))
            .await
    }
    async fn list_memberships(&self, member: &HouseholdId) -> Result<Vec<Membership>> {
        self.query(household_pk(member), "MB#", None, None).await
    }
    async fn list_trip_members(
        &self,
        owner: &HouseholdId,
        trip: &TripId,
    ) -> Result<Vec<Membership>> {
        self.query(household_pk(owner), &format!("SH#{trip}#"), None, None)
            .await
    }

    async fn put_presence(&self, p: &Presence) -> Result<()> {
        self.put(Row {
            pk: household_pk(&p.household),
            sk: format!("TV#{}", p.device_id),
            gsi1pk: None,
            gsi1sk: None,
            ttl: Some(p.at.unix_timestamp() + 24 * 3600),
            data: p,
        })
        .await
    }
    async fn list_presence(&self, household: &HouseholdId) -> Result<Vec<Presence>> {
        self.query(household_pk(household), "TV#", None, None).await
    }

    async fn put_audit(&self, a: &AuditEntry) -> Result<()> {
        self.put(Row {
            pk: household_pk(&a.household),
            sk: format!("AU#{}#{}", a.at.unix_timestamp(), a.id),
            gsi1pk: None,
            gsi1sk: None,
            ttl: Some(a.expires_at().unix_timestamp()),
            data: a,
        })
        .await
    }
    async fn list_audit(&self, household: &HouseholdId) -> Result<Vec<AuditEntry>> {
        let mut v: Vec<AuditEntry> = self
            .query(household_pk(household), "AU#", None, None)
            .await?;
        v.sort_by(|a, b| b.at.cmp(&a.at));
        Ok(v)
    }

    async fn put_proposal(&self, p: &Proposal) -> Result<()> {
        self.put(Row {
            pk: household_pk(&p.household),
            sk: format!("P#{}", p.id),
            gsi1pk: None,
            gsi1sk: None,
            ttl: Some(p.opened_at.unix_timestamp() + 90 * 24 * 3600),
            data: p,
        })
        .await
    }
    async fn get_proposal(
        &self,
        household: &HouseholdId,
        id: &ProposalId,
    ) -> Result<Option<Proposal>> {
        self.get(household_pk(household), format!("P#{id}")).await
    }
    async fn list_proposals(&self, household: &HouseholdId) -> Result<Vec<Proposal>> {
        self.query(household_pk(household), "P#", None, None).await
    }

    async fn put_reaction(&self, r: &Reaction) -> Result<()> {
        let tail = if r.is_like() {
            "like".to_owned()
        } else {
            r.id.to_string()
        };
        self.put(Row {
            pk: household_pk(&r.household),
            sk: format!("R#{}#{}#{}#{tail}", r.trip, r.target.key(), r.actor),
            gsi1pk: None,
            gsi1sk: None,
            ttl: None,
            data: r,
        })
        .await
    }
    async fn list_reactions(
        &self,
        household: &HouseholdId,
        trip: &TripId,
    ) -> Result<Vec<Reaction>> {
        let mut v: Vec<Reaction> = self
            .query(household_pk(household), &format!("R#{trip}#"), None, None)
            .await?;
        v.sort_by_key(|r| (r.created_at, r.id.clone()));
        Ok(v)
    }

    async fn put_person(&self, p: &Person) -> Result<()> {
        self.put(Row {
            pk: household_pk(&p.household),
            sk: format!("PE#{}", p.id),
            gsi1pk: None,
            gsi1sk: None,
            ttl: None,
            data: p,
        })
        .await
    }
    async fn get_person(&self, household: &HouseholdId, id: &PersonId) -> Result<Option<Person>> {
        self.get(household_pk(household), format!("PE#{id}")).await
    }
    async fn list_people(&self, household: &HouseholdId) -> Result<Vec<Person>> {
        let mut v: Vec<Person> = self
            .query(household_pk(household), "PE#", None, None)
            .await?;
        v.sort_by_key(|p| (std::cmp::Reverse(p.media_count), p.id.clone()));
        Ok(v)
    }
    async fn delete_people(&self, household: &HouseholdId) -> Result<()> {
        let people: Vec<Person> = self
            .query(household_pk(household), "PE#", None, None)
            .await?;
        for p in people {
            self.delete(household_pk(household), format!("PE#{}", p.id))
                .await?;
        }
        Ok(())
    }

    async fn put_share_link(&self, l: &ShareLink) -> Result<()> {
        self.put(Row {
            pk: format!("SL#{}", l.id),
            sk: "SL".into(),
            gsi1pk: Some(household_pk(&l.household)),
            gsi1sk: Some(format!("SL#{}", l.id)),
            ttl: Some(l.expires_at.unix_timestamp() + 30 * 24 * 3600),
            data: l,
        })
        .await
    }
    async fn get_share_link(&self, id: &ShareLinkId) -> Result<Option<ShareLink>> {
        self.get(format!("SL#{id}"), "SL".into()).await
    }
    async fn list_share_links(&self, household: &HouseholdId) -> Result<Vec<ShareLink>> {
        self.query(household_pk(household), "SL#", Some("gsi1"), None)
            .await
    }

    async fn put_playlist(&self, p: &Playlist) -> Result<()> {
        self.put(Row {
            pk: household_pk(&p.household),
            sk: format!("PL#{}", p.id),
            gsi1pk: None,
            gsi1sk: None,
            ttl: None,
            data: p,
        })
        .await
    }
    async fn get_playlist(
        &self,
        household: &HouseholdId,
        id: &PlaylistId,
    ) -> Result<Option<Playlist>> {
        self.get(household_pk(household), format!("PL#{id}")).await
    }
    async fn list_playlists(&self, household: &HouseholdId) -> Result<Vec<Playlist>> {
        let mut v: Vec<Playlist> = self
            .query(household_pk(household), "PL#", None, None)
            .await?;
        v.sort_by_key(|p| (std::cmp::Reverse(p.created_at), p.id.clone()));
        Ok(v)
    }
}
