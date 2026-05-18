//! Learning subsystem — openhuman-style stability-graded user profile facets.
//!
//! Sprint 1 of the post-Phase-7 Memory OS work. Implements the C+D loops
//! from openhuman's "20-30 min smart" mechanism (Composio bulk backfill is
//! piece A — we don't have integrations yet; periodic tick is piece B — we
//! already have ProactiveService).
//!
//! ## Module layout
//!
//! - [`candidate`] — taxonomy types ([`FacetClass`], [`CueFamily`],
//!   [`EvidenceRef`]) + [`LearningCandidate`] unit-of-work + bounded
//!   [`Buffer`] producer queue.
//! - Future modules in this sprint (will be added in subsequent commits):
//!   - `stability_detector` — half-life-decayed evidence accumulation
//!   - `scheduler` — periodic rebuild every 30 min
//!   - `cache` — `FacetCache` typed handle
//!   - `prompt_section` — render active facets into `## User Profile (Learned)`
//!     block for the system prompt
//!   - `extractor` — chat-turn candidate producer (Sprint 1.9)
//!
//! ## Spec references
//!
//! - `docs/memory-os/strategy-2026-05-18-research-synthesis.md` Part 1.4
//!   (Top 5 portable patterns) and Appendix C (Path A++ fallback spec)
//! - openhuman source: `/Users/ryanliu/Documents/openhuman/src/openhuman/learning/`

pub mod candidate;
pub mod stability_detector;
