//! Safety mode resolution for ChatDelegate.
//!
//! Owns the effective-mode decision: per-session override beats the
//! global mode held by SafetyManager. In P3-5b this file becomes the
//! single chokepoint for safety queries (no direct SafetyManager reads
//! from turn_runner / model_io).

use super::ChatDelegate;
use crate::safety::SafetyMode;

impl ChatDelegate {
    pub(super) async fn resolve_effective_mode(&self) -> SafetyMode {
        if let Some(m) = self.safety_mode.as_ref() {
            return m.clone();
        }
        self.safety_manager.read().await.policy().global_mode.clone()
    }
}
