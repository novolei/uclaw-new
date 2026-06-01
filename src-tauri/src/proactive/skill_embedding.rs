//! Skill-body embedding helpers (moved out of the deprecated memU module).

use std::sync::Arc;

use crate::memory_bucket_seal::score::embed::Embedder;

/// Embed the full text body of a skill and return the raw vector.
/// Returns `None` (and logs a warning) on embed failure or empty vector.
pub async fn embed_skill_body(embedder: &Arc<dyn Embedder>, body: &str) -> Option<Vec<f32>> {
    match embedder.embed(body).await {
        Ok(v) if !v.is_empty() => Some(v),
        Ok(_) => {
            tracing::warn!("embed_skill_body: embedder returned empty vector");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "embed_skill_body: embed failed");
            None
        }
    }
}

/// Serialize a `Vec<f32>` to a compact JSON string for `embedding_json` storage.
pub fn serialize_embedding(embedding: &[f32]) -> String {
    serde_json::to_string(embedding).unwrap_or_else(|_| "[]".to_string())
}

/// Deserialize an `embedding_json` string back to `Vec<f32>`.
pub fn parse_embedding(json: Option<&str>) -> Option<Vec<f32>> {
    let s = json?.trim();
    if s.is_empty() || s == "null" {
        return None;
    }
    serde_json::from_str::<Vec<f32>>(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_embedding_round_trip() {
        let original: Vec<f32> = (0..8).map(|i| i as f32 * 0.1).collect();
        let json = serialize_embedding(&original);
        let parsed = parse_embedding(Some(&json)).expect("round-trip");
        assert_eq!(parsed.len(), original.len());
        for (a, b) in original.iter().zip(parsed.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn parse_embedding_rejects_empty_and_null() {
        assert!(parse_embedding(None).is_none());
        assert!(parse_embedding(Some("")).is_none());
        assert!(parse_embedding(Some("null")).is_none());
    }
}
