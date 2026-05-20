//! M3-T5 — Intent classifier (pilot).
//!
//! Takes a raw user message and produces a tentative `IntentSpec` —
//! specifically `risk_class`, `autonomy_target`, and a list of
//! `requested_capabilities` derived from the message text. The
//! capability mesh (M3-T2) consumes those queries to resolve concrete
//! tools.
//!
//! This pilot ships a **rule-based** classifier. An LLM-augmented
//! version (M3-T5 commit 2) refines the rule output, but the rule
//! version stays as the fast path and the offline fallback.
//!
//! Classification signals:
//!
//! - **Destructive keywords** (delete, rm -rf, drop table, format, ...)
//!   → `RiskClass::Restricted`, `autonomy_target = AssistedAction`
//! - **Sensitive keywords** (password, secret, billing, ...)
//!   → `RiskClass::High`, `autonomy_target = SupervisedTask`
//! - **External-write keywords** (send, post, deploy, push, publish)
//!   → at least `RiskClass::Medium`, `autonomy_target = SupervisedTask`
//! - **Tool keywords** map to capability queries (\"browse\", \"search the
//!   web\" → web capability; \"check my email\" → email capability; ...).
//!
//! Layout:
//!
//! - [`classify`] — `IntentClassifier` + `classify` function +
//!   `Classification` result

pub mod classify;

pub use classify::{classify, Classification, ClassifierConfig, RiskSignal};
