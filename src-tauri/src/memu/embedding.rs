//! Thin helpers for embedding skill bodies via the memU bridge.
//!
//! Three public symbols:
//! - `embed_skill_body` — calls the bridge, returns the raw vector
//! - `parse_embedding`  — JSON-string ↔ `Vec<f32>` round-trip
//! - `cosine_sim`       — cosine similarity (returns 0.0 on length mismatch)

use std::sync::Arc;

use crate::memu::client::MemUClient;

/// Embed the full text body of a skill and return the raw vector.
///
/// Returns `None` (and logs a warning) if:
/// - `memu_client` is `None` (fastembed unavailable / bridge absent)
/// - the bridge call fails
/// - the response vector is empty
pub async fn embed_skill_body(
    memu_client: &Option<Arc<MemUClient>>,
    body: &str,
) -> Option<Vec<f32>> {
    let client = memu_client.as_ref()?;
    match client.embed_text(&[body]).await {
        Ok(mut vecs) if !vecs.is_empty() => Some(vecs.remove(0)),
        Ok(_) => {
            tracing::warn!("embed_skill_body: bridge returned empty vector list");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "embed_skill_body: bridge call failed");
            None
        }
    }
}

/// Serialize a `Vec<f32>` to a compact JSON string for `embedding_json` storage.
pub fn serialize_embedding(embedding: &[f32]) -> String {
    // serde_json serialises f32 slices cleanly; unwrap is safe for finite floats.
    serde_json::to_string(embedding).unwrap_or_else(|_| "[]".to_string())
}

/// Deserialize an `embedding_json` string back to `Vec<f32>`.
///
/// Returns `None` if the string is `None`, empty, or not a valid JSON array of
/// numbers. Callers should treat `None` as "no embedding available" and skip the
/// cosine channel gracefully.
pub fn parse_embedding(json: Option<&str>) -> Option<Vec<f32>> {
    let s = json?.trim();
    if s.is_empty() || s == "null" {
        return None;
    }
    serde_json::from_str::<Vec<f32>>(s).ok()
}

/// Cosine similarity between two vectors.
///
/// Returns `0.0` for zero-length inputs or mismatched dimensions rather than
/// panicking — callers treat a low score as "no semantic match".
pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identity_is_one() {
        let v = vec![1.0f32, 0.0, 0.0];
        let sim = cosine_sim(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6, "identical vectors should have cosine sim 1.0, got {}", sim);
    }

    #[test]
    fn cosine_orthogonal_is_zero() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        let sim = cosine_sim(&a, &b);
        assert!(sim.abs() < 1e-6, "orthogonal vectors should have cosine sim 0.0, got {}", sim);
    }

    #[test]
    fn cosine_handles_mismatched_lengths() {
        let a = vec![1.0f32, 0.0];
        let b = vec![1.0f32, 0.0, 0.0];
        let sim = cosine_sim(&a, &b);
        assert_eq!(sim, 0.0, "mismatched lengths should return 0.0");
    }

    #[test]
    fn parse_embedding_round_trip() {
        let original: Vec<f32> = (0..8).map(|i| i as f32 * 0.1).collect();
        let json = serialize_embedding(&original);
        let parsed = parse_embedding(Some(&json)).expect("should round-trip");
        assert_eq!(parsed.len(), original.len());
        for (a, b) in original.iter().zip(parsed.iter()) {
            assert!((a - b).abs() < 1e-6, "value mismatch: {} vs {}", a, b);
        }
    }
}
