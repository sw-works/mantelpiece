//! Embedding index for Ask (F-303, research R7). Within a trip the hot path is a small
//! DynamoDB cache scanned in memory; across the household it is S3 Vectors.

use async_trait::async_trait;
use aws_sdk_dynamodb::types::AttributeValue;
use mantelpiece_core::ids::{HouseholdId, MediaId, TripId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::{Result, StorageError};

#[derive(Clone, Debug, PartialEq)]
pub struct VectorHit {
    pub media: MediaId,
    /// Cosine similarity, higher is closer.
    pub score: f32,
}

#[async_trait]
pub trait Vectors: Send + Sync {
    async fn upsert(
        &self,
        household: &HouseholdId,
        trip: Option<&TripId>,
        media: &MediaId,
        vector: &[f32],
    ) -> Result<()>;
    /// Nearest media inside one trip. The 300 ms warm budget lives here.
    async fn query_trip(
        &self,
        household: &HouseholdId,
        trip: &TripId,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorHit>>;
    /// Nearest media across the whole household.
    async fn query_household(
        &self,
        household: &HouseholdId,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorHit>>;
    async fn delete(&self, household: &HouseholdId, media: &MediaId) -> Result<()>;
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

fn rank(
    candidates: impl Iterator<Item = (MediaId, Vec<f32>)>,
    vector: &[f32],
    top_k: usize,
) -> Vec<VectorHit> {
    let mut hits: Vec<VectorHit> = candidates
        .map(|(media, v)| VectorHit {
            media,
            score: cosine(&v, vector),
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.media.cmp(&b.media))
    });
    hits.truncate(top_k);
    hits
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Stored {
    trip: Option<TripId>,
    media: MediaId,
    vector: Vec<f32>,
}

/// Brute-force cosine over a map; the shape every backend must match.
#[derive(Debug, Default)]
pub struct MemoryVectors {
    rows: Mutex<HashMap<(HouseholdId, MediaId), Stored>>,
}

#[async_trait]
impl Vectors for MemoryVectors {
    async fn upsert(
        &self,
        household: &HouseholdId,
        trip: Option<&TripId>,
        media: &MediaId,
        vector: &[f32],
    ) -> Result<()> {
        self.rows.lock().unwrap().insert(
            (household.clone(), media.clone()),
            Stored {
                trip: trip.cloned(),
                media: media.clone(),
                vector: vector.to_vec(),
            },
        );
        Ok(())
    }
    async fn query_trip(
        &self,
        household: &HouseholdId,
        trip: &TripId,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorHit>> {
        let rows = self.rows.lock().unwrap();
        Ok(rank(
            rows.iter()
                .filter(|((h, _), s)| h == household && s.trip.as_ref() == Some(trip))
                .map(|(_, s)| (s.media.clone(), s.vector.clone())),
            vector,
            top_k,
        ))
    }
    async fn query_household(
        &self,
        household: &HouseholdId,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorHit>> {
        let rows = self.rows.lock().unwrap();
        Ok(rank(
            rows.iter()
                .filter(|((h, _), _)| h == household)
                .map(|(_, s)| (s.media.clone(), s.vector.clone())),
            vector,
            top_k,
        ))
    }
    async fn delete(&self, household: &HouseholdId, media: &MediaId) -> Result<()> {
        self.rows
            .lock()
            .unwrap()
            .remove(&(household.clone(), media.clone()));
        Ok(())
    }
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

/// The key condition for one household's vectors. DynamoDB rejects an empty string in
/// `begins_with`, so a household-wide read (search across trips, delete) is the partition
/// alone.
fn key_condition(prefix: &str) -> &'static str {
    if prefix.is_empty() {
        "pk = :pk"
    } else {
        "pk = :pk AND begins_with(sk, :prefix)"
    }
}

/// Per-trip cache in the main table: `pk = V#<household>`, `sk = <trip>#<media>`. Every
/// query reads one partition prefix and ranks in memory, which is fine for a trip of a few
/// hundred keepers. Compile-checked only.
#[derive(Debug, Clone)]
pub struct DynamoVectors {
    client: aws_sdk_dynamodb::Client,
    table: String,
}

#[derive(Serialize, Deserialize)]
struct VectorRow {
    pk: String,
    sk: String,
    data: Stored,
}

impl DynamoVectors {
    pub fn from_config(config: &aws_config::SdkConfig, table: impl Into<String>) -> Self {
        Self::new(aws_sdk_dynamodb::Client::new(config), table)
    }

    pub fn new(client: aws_sdk_dynamodb::Client, table: impl Into<String>) -> Self {
        Self {
            client,
            table: table.into(),
        }
    }

    fn pk(household: &HouseholdId) -> String {
        format!("V#{household}")
    }

    fn sk(trip: Option<&TripId>, media: &MediaId) -> String {
        format!(
            "{}#{media}",
            trip.map_or("_".to_owned(), ToString::to_string)
        )
    }

    async fn scan_prefix(&self, household: &HouseholdId, prefix: &str) -> Result<Vec<Stored>> {
        let mut query = self
            .client
            .query()
            .table_name(&self.table)
            .key_condition_expression(key_condition(prefix))
            .expression_attribute_values(":pk", AttributeValue::S(Self::pk(household)));
        if !prefix.is_empty() {
            query =
                query.expression_attribute_values(":prefix", AttributeValue::S(prefix.to_owned()));
        }
        let out = query.send().await.map_err(backend)?;
        out.items
            .unwrap_or_default()
            .into_iter()
            .map(|item| {
                serde_dynamo::from_item::<_, VectorRow>(item)
                    .map(|r| r.data)
                    .map_err(|e| StorageError::Serde(e.to_string()))
            })
            .collect()
    }
}

#[async_trait]
impl Vectors for DynamoVectors {
    async fn upsert(
        &self,
        household: &HouseholdId,
        trip: Option<&TripId>,
        media: &MediaId,
        vector: &[f32],
    ) -> Result<()> {
        let row = VectorRow {
            pk: Self::pk(household),
            sk: Self::sk(trip, media),
            data: Stored {
                trip: trip.cloned(),
                media: media.clone(),
                vector: vector.to_vec(),
            },
        };
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
    async fn query_trip(
        &self,
        household: &HouseholdId,
        trip: &TripId,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorHit>> {
        let rows = self.scan_prefix(household, &format!("{trip}#")).await?;
        Ok(rank(
            rows.into_iter().map(|s| (s.media, s.vector)),
            vector,
            top_k,
        ))
    }
    async fn query_household(
        &self,
        household: &HouseholdId,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorHit>> {
        let rows = self.scan_prefix(household, "").await?;
        Ok(rank(
            rows.into_iter().map(|s| (s.media, s.vector)),
            vector,
            top_k,
        ))
    }
    async fn delete(&self, household: &HouseholdId, media: &MediaId) -> Result<()> {
        // The trip is not known here; find the row by suffix and delete it.
        let rows = self.scan_prefix(household, "").await?;
        for s in rows.into_iter().filter(|s| &s.media == media) {
            self.client
                .delete_item()
                .table_name(&self.table)
                .key("pk", AttributeValue::S(Self::pk(household)))
                .key("sk", AttributeValue::S(Self::sk(s.trip.as_ref(), media)))
                .send()
                .await
                .map_err(backend)?;
        }
        Ok(())
    }
}

/// S3 Vectors for cross-trip search (research R7). One index per deployment; every
/// vector carries household and trip metadata used as a filter. Compile-checked only.
#[derive(Debug, Clone)]
pub struct S3Vectors {
    client: aws_sdk_s3vectors::Client,
    bucket: String,
    index: String,
}

impl S3Vectors {
    pub fn new(
        client: aws_sdk_s3vectors::Client,
        bucket: impl Into<String>,
        index: impl Into<String>,
    ) -> Self {
        Self {
            client,
            bucket: bucket.into(),
            index: index.into(),
        }
    }

    fn key(household: &HouseholdId, media: &MediaId) -> String {
        format!("{household}/{media}")
    }

    fn filter(fields: &[(&str, &str)]) -> aws_smithy_types::Document {
        let map: HashMap<String, aws_smithy_types::Document> = fields
            .iter()
            .map(|(k, v)| {
                (
                    (*k).to_owned(),
                    aws_smithy_types::Document::String((*v).to_owned()),
                )
            })
            .collect();
        aws_smithy_types::Document::Object(map)
    }

    async fn query(
        &self,
        filter: aws_smithy_types::Document,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorHit>> {
        let out = self
            .client
            .query_vectors()
            .vector_bucket_name(&self.bucket)
            .index_name(&self.index)
            .query_vector(aws_sdk_s3vectors::types::VectorData::Float32(
                vector.to_vec(),
            ))
            .top_k(i32::try_from(top_k).unwrap_or(i32::MAX))
            .filter(filter)
            .return_distance(true)
            .send()
            .await
            .map_err(backend)?;
        Ok(out
            .vectors()
            .iter()
            .filter_map(|v| {
                let media = v.key().rsplit('/').next()?;
                Some(VectorHit {
                    media: MediaId::new(media),
                    // Cosine distance to similarity.
                    score: 1.0 - v.distance().unwrap_or(1.0),
                })
            })
            .collect())
    }
}

#[async_trait]
impl Vectors for S3Vectors {
    async fn upsert(
        &self,
        household: &HouseholdId,
        trip: Option<&TripId>,
        media: &MediaId,
        vector: &[f32],
    ) -> Result<()> {
        let metadata = Self::filter(&[
            ("household", household.as_str()),
            ("trip", trip.map_or("", TripId::as_str)),
        ]);
        let input = aws_sdk_s3vectors::types::PutInputVector::builder()
            .key(Self::key(household, media))
            .data(aws_sdk_s3vectors::types::VectorData::Float32(
                vector.to_vec(),
            ))
            .metadata(metadata)
            .build()
            .map_err(backend)?;
        self.client
            .put_vectors()
            .vector_bucket_name(&self.bucket)
            .index_name(&self.index)
            .vectors(input)
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }
    async fn query_trip(
        &self,
        household: &HouseholdId,
        trip: &TripId,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorHit>> {
        self.query(
            Self::filter(&[("household", household.as_str()), ("trip", trip.as_str())]),
            vector,
            top_k,
        )
        .await
    }
    async fn query_household(
        &self,
        household: &HouseholdId,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorHit>> {
        self.query(
            Self::filter(&[("household", household.as_str())]),
            vector,
            top_k,
        )
        .await
    }
    async fn delete(&self, household: &HouseholdId, media: &MediaId) -> Result<()> {
        self.client
            .delete_vectors()
            .vector_bucket_name(&self.bucket)
            .index_name(&self.index)
            .keys(Self::key(household, media))
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_vectors_rank_by_cosine_within_scope() {
        let v = MemoryVectors::default();
        let h = HouseholdId::new("h");
        let kyoto = TripId::new("kyoto");
        let lisbon = TripId::new("lisbon");
        v.upsert(&h, Some(&kyoto), &MediaId::new("ramen"), &[1.0, 0.0])
            .await
            .unwrap();
        v.upsert(&h, Some(&kyoto), &MediaId::new("temple"), &[0.0, 1.0])
            .await
            .unwrap();
        v.upsert(&h, Some(&lisbon), &MediaId::new("tram"), &[0.9, 0.1])
            .await
            .unwrap();
        v.upsert(
            &HouseholdId::new("other"),
            None,
            &MediaId::new("x"),
            &[1.0, 0.0],
        )
        .await
        .unwrap();
        let hits = v.query_trip(&h, &kyoto, &[0.8, 0.2], 5).await.unwrap();
        assert_eq!(
            hits.iter().map(|h| h.media.as_str()).collect::<Vec<_>>(),
            ["ramen", "temple"]
        );
        let all = v.query_household(&h, &[0.8, 0.2], 2).await.unwrap();
        assert_eq!(
            all.iter().map(|h| h.media.as_str()).collect::<Vec<_>>(),
            ["tram", "ramen"]
        );
        v.delete(&h, &MediaId::new("ramen")).await.unwrap();
        assert_eq!(
            v.query_trip(&h, &kyoto, &[1.0, 0.0], 5)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(cosine(&[0.0, 0.0], &[1.0, 0.0]).abs() < f32::EPSILON);
    }

    #[test]
    fn dynamo_household_read_has_no_empty_prefix() {
        assert_eq!(key_condition(""), "pk = :pk");
        assert_eq!(
            key_condition("kyoto#"),
            "pk = :pk AND begins_with(sk, :prefix)"
        );
    }
}
