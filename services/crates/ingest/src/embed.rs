//! Deterministic embeddings for Ask (F-303, research R7). Labels, detected text, the
//! landmark and the description are tokenised and feature-hashed into a fixed vector, so
//! a query embedded the same way lands near the media that share its words. No model
//! call, no network, stable across runs; a learned embedding can replace it behind the
//! `Vectors` trait later.

use memorylane_core::media::Tag;

pub const DIM: usize = 128;

const STOP: [&str; 24] = [
    "a", "an", "and", "at", "for", "from", "in", "is", "it", "of", "on", "or", "our", "the",
    "this", "to", "we", "with", "show", "me", "photos", "pictures", "day", "days",
];

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Lowercase alphanumeric tokens with stop words and single characters dropped.
pub fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|t| t.len() > 1 && !STOP.contains(&t.as_str()))
        .collect()
}

fn add(vector: &mut [f32], token: &str, weight: f32) {
    let h = fnv1a(token.as_bytes());
    let idx = usize::try_from(h % DIM as u64).unwrap_or(0);
    let sign = if (h >> 32) & 1 == 0 { 1.0 } else { -1.0 };
    vector[idx] += sign * weight;
}

fn normalise(mut vector: Vec<f32>) -> Vec<f32> {
    let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut vector {
            *x /= norm;
        }
    }
    vector
}

/// Vector for a media item from what the pipeline learned about it.
pub fn embed_media(
    tags: &[Tag],
    landmark: Option<&str>,
    text: Option<&str>,
    description: Option<&str>,
) -> Vec<f32> {
    let mut v = vec![0.0; DIM];
    for t in tags {
        for tok in tokens(&t.name) {
            add(&mut v, &tok, t.confidence.clamp(0.1, 1.0));
        }
    }
    if let Some(l) = landmark {
        for tok in tokens(l) {
            add(&mut v, &tok, 1.5);
        }
    }
    for source in [text, description].into_iter().flatten() {
        for tok in tokens(source) {
            add(&mut v, &tok, 0.5);
        }
    }
    normalise(v)
}

/// Vector for a natural-language query.
pub fn embed_query(query: &str) -> Vec<f32> {
    let mut v = vec![0.0; DIM];
    for tok in tokens(query) {
        add(&mut v, &tok, 1.0);
    }
    normalise(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    #[test]
    fn queries_land_near_media_that_share_their_words() {
        let ramen = embed_media(
            &[
                Tag {
                    name: "Ramen".into(),
                    confidence: 0.9,
                },
                Tag {
                    name: "Restaurant".into(),
                    confidence: 0.8,
                },
            ],
            None,
            Some("Ichiran"),
            None,
        );
        let temple = embed_media(
            &[Tag {
                name: "Temple".into(),
                confidence: 0.9,
            }],
            Some("Kinkaku-ji"),
            None,
            None,
        );
        let q = embed_query("the ramen place");
        assert!(cosine(&q, &ramen) > cosine(&q, &temple));
        assert!(cosine(&embed_query("Kinkaku-ji temple"), &temple) > 0.5);
        assert_eq!(tokens("Show me the Photos of a Day"), Vec::<String>::new());
        assert!(embed_query("").iter().all(|x| *x == 0.0));
    }
}
