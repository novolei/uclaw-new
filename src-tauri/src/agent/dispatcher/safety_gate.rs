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
        // Clone the Arc out of AppState so the State borrow is released
        // before the async .read().await — avoids holding State across an await.
        // Then bind the guard and mode separately so `safety_manager` outlives
        // the read guard (E0597 fix).
        let safety_manager = self.app_state().safety_manager.clone();
        let guard = safety_manager.read().await;
        guard.policy().global_mode.clone()
    }
}
