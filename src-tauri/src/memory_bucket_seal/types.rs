//! Core types for the bucket-seal ingestion layer (openhuman port — Phase 1
//! equivalent of issue #707). Defines [`Chunk`] + provenance [`Metadata`] +
//! deterministic chunk-id hashing.
//!
//! Faithful port of `openhuman/src/openhuman/memory/tree/types.rs` with the
//! `DataSource` enum (provider-level discriminator) dropped — that lands
//! with the canonicalize port in PR6.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Which kind of upstream source produced a chunk.
///
/// Used both as a metadata discriminator and as the routing key for the
/// canonicaliser dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Chat transcript scoped by channel or group (Slack, Discord, Telegram, WhatsApp…).
    Chat,
    /// Email thread (Gmail and generic IMAP).
    Email,
    /// Standalone document (Notion page, Drive doc, meeting note, uploaded file…).
    Document,
}

impl SourceKind {
    /// Stable string representation for DB storage and RPC surfaces.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Chat => "chat",
            SourceKind::Email => "email",
            SourceKind::Document => "document",
        }
    }

    /// Parse back from the on-wire / on-disk string form.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "chat" => Ok(SourceKind::Chat),
            "email" => Ok(SourceKind::Email),
            "document" => Ok(SourceKind::Document),
            other => Err(format!("unknown source kind: {other}")),
        }
    }
}

/// A concrete pointer back to where a chunk originated — used for citation,
/// drill-down, and deduplication at re-ingest time.
///
/// Consumers should treat this as an opaque, source-specific reference. The
/// shape depends on [`SourceKind`]:
/// - **Chat**: `{platform}://{channel}/{message_id}` or `{permalink}`
/// - **Email**: message-id header (`<abc@example.com>`) or provider URL
/// - **Document**: file path, Notion page URL, Drive file id
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    /// Opaque provider-specific identifier for the exact source record.
    pub value: String,
}

impl SourceRef {
    /// Wrap an opaque provider-specific identifier as a [`SourceRef`].
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }
}

/// Provenance metadata captured per chunk at ingest time.
///
/// Acceptance criteria require at minimum: source type, source identifier,
/// owner/account, timestamps, and tags/labels when available.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    /// Which upstream source kind produced this chunk.
    pub source_kind: SourceKind,
    /// Stable logical id for the ingestion group (channel id, thread id, doc id).
    ///
    /// Chat: channel/group id. Email: thread id. Document: doc id.
    pub source_id: String,
    /// Account or user the content belongs to. Empty string for anonymous / system sources.
    pub owner: String,
    /// Point-in-time timestamp for ordering within a source.
    ///
    /// For chats = message time; for emails = message sent time;
    /// for documents = last-modified or ingest time.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
    /// Covering time range the chunk spans. For a single leaf it usually equals
    /// `(timestamp, timestamp)`; for later summary nodes it widens to
    /// cover all children.
    #[serde(with = "time_range_serde")]
    pub time_range: (DateTime<Utc>, DateTime<Utc>),
    /// Arbitrary labels / tags carried through from the source (e.g. Gmail labels,
    /// Slack reactions, Notion tags). Ingest does not interpret these.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Opaque pointer back to the raw source record for drill-down / citation.
    pub source_ref: Option<SourceRef>,
}

impl Metadata {
    /// Convenience constructor used by canonicalisers: point timestamp,
    /// `time_range = (timestamp, timestamp)`.
    pub fn point_in_time(
        source_kind: SourceKind,
        source_id: impl Into<String>,
        owner: impl Into<String>,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            source_kind,
            source_id: source_id.into(),
            owner: owner.into(),
            timestamp,
            time_range: (timestamp, timestamp),
            tags: Vec::new(),
            source_ref: None,
        }
    }
}

/// A single ingested chunk — the atomic persistence unit.
///
/// In the LLD this is the leaf of a source tree. Later phases will build
/// summary nodes on top of these leaves; at PR5 they live standalone.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    /// Deterministic id derived from (source_kind, source_id, seq_in_source, content).
    pub id: String,
    /// Canonical Markdown content.
    pub content: String,
    /// Provenance metadata.
    pub metadata: Metadata,
    /// Token count (rough heuristic — 1 token ≈ 4 chars).
    pub token_count: u32,
    /// Sequence number of this chunk inside its logical source. Stable and
    /// starts at 0 for the first chunk of a source.
    pub seq_in_source: u32,
    /// When this chunk was persisted to the local store.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,
    /// True when this chunk is a sub-split of a single logical unit (e.g. a
    /// chat message or email body that exceeded `max_tokens`). The full logical
    /// unit was split into multiple pieces; each piece carries this flag so
    /// downstream scorers can lower its weight relative to whole-unit chunks.
    #[serde(default)]
    pub partial_message: bool,
}

/// Deterministic chunk id.
///
/// `sha256(source_kind | "\0" | source_id | "\0" | seq | "\0" | content)`
/// hex-encoded, first 32 chars (128 bits of collision resistance). Short
/// enough for human inspection, long enough for global uniqueness in a
/// single-user workspace.
///
/// Content is included so multiple ingest calls that share a `source_id`
/// (e.g. successive Slack 6-hour buckets all flowing into one
/// per-connection source tree) don't collide on `seq=0,1,2,…`. Re-ingesting
/// the same canonical content under the same `(source_id, seq)` still
/// produces the same id, so upserts stay idempotent.
pub fn chunk_id(
    source_kind: SourceKind,
    source_id: &str,
    seq_in_source: u32,
    content: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_kind.as_str().as_bytes());
    hasher.update([0u8]);
    hasher.update(source_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(seq_in_source.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    let hex = digest.iter().fold(String::with_capacity(64), |mut acc, b| {
        use std::fmt::Write;
        let _ = write!(acc, "{b:02x}");
        acc
    });
    hex[..32].to_string()
}

/// Approximate token count (GPT-family heuristic: 1 token ≈ 4 chars).
///
/// PR5 does not need a real tokenizer — downstream phases will enforce the
/// 10k summariser budget with a precise tokenizer.
pub fn approx_token_count(text: &str) -> u32 {
    // saturating_add guards against absurdly long inputs
    let chars = text.chars().count() as u32;
    chars.saturating_add(3) / 4
}

mod time_range_serde {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct Wire {
        start_ms: i64,
        end_ms: i64,
    }

    pub fn serialize<S: Serializer>(
        value: &(DateTime<Utc>, DateTime<Utc>),
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Wire {
            start_ms: value.0.timestamp_millis(),
            end_ms: value.1.timestamp_millis(),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<(DateTime<Utc>, DateTime<Utc>), D::Error> {
        let wire = Wire::deserialize(deserializer)?;
        let start = Utc
            .timestamp_millis_opt(wire.start_ms)
            .single()
            .ok_or_else(|| serde::de::Error::custom("invalid start_ms"))?;
        let end = Utc
            .timestamp_millis_opt(wire.end_ms)
            .single()
            .ok_or_else(|| serde::de::Error::custom("invalid end_ms"))?;
        Ok((start, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_id_is_deterministic() {
        let a = chunk_id(SourceKind::Chat, "slack:#eng", 0, "hello");
        let b = chunk_id(SourceKind::Chat, "slack:#eng", 0, "hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn chunk_id_varies_with_seq() {
        let a = chunk_id(SourceKind::Chat, "slack:#eng", 0, "hello");
        let b = chunk_id(SourceKind::Chat, "slack:#eng", 1, "hello");
        assert_ne!(a, b);
    }

    #[test]
    fn chunk_id_varies_with_content() {
        // Critical for the per-connection source_id design: two ingests
        // sharing source_id but different content (e.g. different 6-hour
        // Slack buckets) must produce distinct ids at seq=0,1,2,…
        let a = chunk_id(SourceKind::Chat, "slack:c1", 0, "bucket A content");
        let b = chunk_id(SourceKind::Chat, "slack:c1", 0, "bucket B content");
        assert_ne!(a, b);
    }

    #[test]
    fn source_kind_round_trip() {
        for kind in [SourceKind::Chat, SourceKind::Email, SourceKind::Document] {
            assert_eq!(SourceKind::parse(kind.as_str()).unwrap(), kind);
        }
    }

    #[test]
    fn approx_token_count_scales_linearly() {
        assert_eq!(approx_token_count(""), 0);
        assert_eq!(approx_token_count("a"), 1); // 1→1
        assert_eq!(approx_token_count("abcd"), 1); // 4→1
        assert_eq!(approx_token_count("abcde"), 2); // 5→2
        assert_eq!(approx_token_count(&"x".repeat(400)), 100);
    }

    #[test]
    fn time_range_serde_round_trip() {
        use chrono::TimeZone;
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        let meta = Metadata::point_in_time(SourceKind::Chat, "src", "owner", ts);
        let json = serde_json::to_string(&meta).unwrap();
        let back: Metadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.time_range, (ts, ts));
    }
}
