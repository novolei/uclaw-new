//! Bucket-seal memory backend (openhuman port).
//!
//! Standalone storage layer for chunks: atomic file writes under
//! `<content_root>/{chat,email,document}/<slug>/<chunk_id>.md` indexed by a
//! SQLite catalog at `<bucket_seal_dir>/chunks.db`. Build target for the
//! BucketSealAdapter in PR9; no AppState wiring or IPC at this stage.
//!
//! Faithful port of `openhuman::memory::tree` (atomic + paths + chunks-only
//! SQLite). Summaries, scoring, entity index, jobs, and the topic/global
//! trees follow in later PRs.

pub mod atomic;
pub mod paths;
pub mod store;
pub mod types;

pub use store::BucketSealStore;
pub use types::{approx_token_count, chunk_id, Chunk, Metadata, SourceKind, SourceRef};
