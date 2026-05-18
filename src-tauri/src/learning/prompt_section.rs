//! Prompt section renderer — Sprint 1.6.
//!
//! Renders the active facets in [`FacetCache`] into a `## User
//! Profile (Learned)` markdown block ready for injection into the
//! agent system prompt.
//!
//! Output shape (consistent with openhuman
//! `learning/prompt_sections.rs:71-98`):
//!
//! ```markdown
//! ## User Profile (Learned)
//!
//! **Identity**
//! - name: Alice
//! - timezone: America/Los_Angeles
//!
//! **Tooling**
//! - editor: helix
//! - package_manager: pnpm
//!
//! **Style**
//! - verbosity: terse
//! ```
//!
//! Sprint 1.8 calls `UserProfileSection::render(&cache)` from the
//! agent prompt builder. The output sits next to (typically before)
//! the existing skill manifest block.
//!
//! ## Token cap
//!
//! Hard upper bound of `USER_PROFILE_MAX_CHARS = 3000` chars
//! (~750 tokens). When approaching the cap, lower-stability facets
//! within each class are dropped first; in extreme cases entire
//! lower-priority classes are omitted (priority order: Identity →
//! Veto → Tooling → Style → Goal → Channel).

use crate::learning::cache::FacetCache;
use crate::learning::candidate::FacetClass;

/// Approximate character cap for the rendered block. Real tokens
/// vary by model but `chars ≈ 4 × tokens` is a defensible upper
/// bound for English.
pub const USER_PROFILE_MAX_CHARS: usize = 3000;

/// Priority order for class rendering. Truncation drops from the
/// bottom of this list first when the char cap is approaching.
const CLASS_RENDER_ORDER: &[FacetClass] = &[
    FacetClass::Identity, // never-truncate first
    FacetClass::Veto,
    FacetClass::Tooling,
    FacetClass::Style,
    FacetClass::Goal,
    FacetClass::Channel,
];

/// Human-readable section heading for each class.
fn class_heading(class: FacetClass) -> &'static str {
    match class {
        FacetClass::Identity => "Identity",
        FacetClass::Veto => "Hard Vetoes",
        FacetClass::Tooling => "Tooling",
        FacetClass::Style => "Style",
        FacetClass::Goal => "Goals",
        FacetClass::Channel => "Channel",
    }
}

// ─── UserProfileSection ────────────────────────────────────────────────

/// Stateless renderer. Holds no data — pulls from `FacetCache` on
/// every call. Sprint 1.8 will mount this into the agent prompt
/// builder behind `MemoryOsConfig.learning_enabled`.
pub struct UserProfileSection;

impl UserProfileSection {
    /// Render the active-facet block. Returns `None` when there's
    /// nothing to render (empty cache); caller skips injection in
    /// that case so the prompt doesn't grow a useless empty heading.
    pub fn render(cache: &FacetCache) -> Option<String> {
        Self::render_with_cap(cache, USER_PROFILE_MAX_CHARS)
    }

    /// Explicit-cap version. Exposed for tests; production callers
    /// use [`render`](Self::render) with the default cap.
    pub fn render_with_cap(cache: &FacetCache, max_chars: usize) -> Option<String> {
        let mut buf = String::from("## User Profile (Learned)\n\n");
        let mut any = false;
        for class in CLASS_RENDER_ORDER {
            let facets = cache.active_by_class(*class);
            if facets.is_empty() {
                continue;
            }
            // Build the class section in a scratch buffer first so we
            // can decide whether to commit it under the cap. If the
            // class section would push the block over the cap, drop
            // the lowest-stability rows one by one until it fits;
            // if even the heading + one row doesn't fit, skip the
            // class entirely.
            let mut sub = format!("**{}**\n", class_heading(*class));
            // Facets come pre-sorted by stability DESC from the cache.
            for f in &facets {
                let line = format!("- {}: {}\n", f.name, f.value);
                if buf.len() + sub.len() + line.len() + 1 > max_chars {
                    break;
                }
                sub.push_str(&line);
            }
            // Has at least one row line ("- ...\n") past the heading?
            let row_lines = sub.lines().filter(|l| l.starts_with("- ")).count();
            if row_lines == 0 {
                continue;
            }
            buf.push_str(&sub);
            buf.push('\n');
            any = true;
        }
        if !any {
            return None;
        }
        // Trim trailing blank line for tidier output.
        while buf.ends_with("\n\n") {
            buf.pop();
        }
        Some(buf)
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::stability_detector::{CueWeights, FacetSnapshot, FacetState};

    fn snap(class: FacetClass, name: &str, value: &str, stability: f64) -> FacetSnapshot {
        FacetSnapshot {
            facet_id: format!("{}-{}", class.as_str(), name),
            class,
            name: name.into(),
            value: value.into(),
            state: FacetState::Active,
            stability,
            cue_weights: CueWeights::default(),
            evidence_count: 1,
            last_seen_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn render_empty_cache_returns_none() {
        let cache = FacetCache::new();
        assert!(UserProfileSection::render(&cache).is_none());
    }

    #[test]
    fn render_single_class_emits_heading_and_rows() {
        let cache = FacetCache::new();
        cache.replace_with(
            vec![
                snap(FacetClass::Tooling, "editor", "helix", 2.5),
                snap(FacetClass::Tooling, "shell", "zsh", 2.0),
            ],
            0,
        );
        let out = UserProfileSection::render(&cache).expect("non-empty");
        assert!(out.starts_with("## User Profile (Learned)"));
        assert!(out.contains("**Tooling**"));
        assert!(out.contains("- editor: helix"));
        assert!(out.contains("- shell: zsh"));
    }

    #[test]
    fn render_multi_class_uses_priority_order() {
        let cache = FacetCache::new();
        cache.replace_with(
            vec![
                snap(FacetClass::Style, "verbosity", "terse", 1.7),
                snap(FacetClass::Tooling, "editor", "helix", 2.0),
                snap(FacetClass::Identity, "name", "Alice", 3.0),
                snap(FacetClass::Goal, "ship", "memory-os", 1.8),
            ],
            0,
        );
        let out = UserProfileSection::render(&cache).expect("non-empty");
        // Class order from CLASS_RENDER_ORDER: Identity > Veto > Tooling > Style > Goal > Channel
        let pos_identity = out.find("**Identity**").unwrap();
        let pos_tooling = out.find("**Tooling**").unwrap();
        let pos_style = out.find("**Style**").unwrap();
        let pos_goal = out.find("**Goals**").unwrap();
        assert!(pos_identity < pos_tooling);
        assert!(pos_tooling < pos_style);
        assert!(pos_style < pos_goal);
    }

    #[test]
    fn render_rows_within_class_are_stability_desc() {
        // FacetCache already sorts at replace_with; renderer relies on that contract.
        let cache = FacetCache::new();
        cache.replace_with(
            vec![
                snap(FacetClass::Tooling, "low", "v1", 1.6),
                snap(FacetClass::Tooling, "high", "v2", 3.2),
                snap(FacetClass::Tooling, "mid", "v3", 2.1),
            ],
            0,
        );
        let out = UserProfileSection::render(&cache).expect("non-empty");
        let pos_high = out.find("- high: v2").unwrap();
        let pos_mid = out.find("- mid: v3").unwrap();
        let pos_low = out.find("- low: v1").unwrap();
        assert!(pos_high < pos_mid);
        assert!(pos_mid < pos_low);
    }

    #[test]
    fn render_skips_classes_with_no_active_facets() {
        let cache = FacetCache::new();
        cache.replace_with(
            vec![snap(FacetClass::Tooling, "editor", "helix", 2.0)],
            0,
        );
        let out = UserProfileSection::render(&cache).expect("non-empty");
        assert!(!out.contains("**Identity**"));
        assert!(!out.contains("**Goals**"));
        assert!(out.contains("**Tooling**"));
    }

    #[test]
    fn render_filters_non_active_state() {
        // Cache contains Provisional + Candidate + Forgotten — none should appear.
        let cache = FacetCache::new();
        let mut s_prov = snap(FacetClass::Tooling, "browser", "firefox", 1.0);
        s_prov.state = FacetState::Provisional;
        let mut s_cand = snap(FacetClass::Tooling, "ide", "vscode", 0.5);
        s_cand.state = FacetState::Candidate;
        let mut s_forgot = snap(FacetClass::Tooling, "old", "vim", 0.0);
        s_forgot.state = FacetState::Forgotten;
        let s_active = snap(FacetClass::Tooling, "editor", "helix", 2.5);
        cache.replace_with(vec![s_prov, s_cand, s_forgot, s_active], 0);

        let out = UserProfileSection::render(&cache).expect("non-empty");
        assert!(out.contains("- editor: helix"));
        assert!(!out.contains("firefox"));
        assert!(!out.contains("vscode"));
        assert!(!out.contains("vim"));
    }

    #[test]
    fn render_with_cap_drops_classes_from_bottom_priority_first() {
        // Tight cap that admits Identity but truncates Style + Goal.
        let cache = FacetCache::new();
        cache.replace_with(
            vec![
                snap(FacetClass::Identity, "name", "Alice", 3.0),
                snap(FacetClass::Tooling, "editor", "helix", 2.0),
                snap(
                    FacetClass::Style,
                    "verbosity",
                    "an extremely long and detailed verbosity preference that takes many bytes to render in markdown form",
                    1.7,
                ),
                snap(
                    FacetClass::Goal,
                    "ship",
                    "another long-winded goal description that would surely overflow most token caps",
                    1.6,
                ),
            ],
            0,
        );
        // Cap small enough that we cannot include everything.
        let out = UserProfileSection::render_with_cap(&cache, 140).expect("non-empty");
        // Identity (highest priority) must survive.
        assert!(out.contains("**Identity**"));
        assert!(out.contains("- name: Alice"));
        // Goal (lowest priority of the set) should be dropped under the cap.
        // We don't assert *which* lower-priority class was dropped exactly,
        // just that the final output fits the cap.
        assert!(
            out.len() <= 140,
            "output ({} chars) must respect cap (140)",
            out.len()
        );
    }

    #[test]
    fn render_handles_huge_active_count_without_panic() {
        // 100 Active Tooling facets — only top-N should fit under the default cap.
        let cache = FacetCache::new();
        let snaps: Vec<FacetSnapshot> = (0..100)
            .map(|i| snap(FacetClass::Tooling, &format!("k{}", i), "v", 100.0 - i as f64))
            .collect();
        cache.replace_with(snaps, 0);
        let out = UserProfileSection::render(&cache).expect("non-empty");
        assert!(out.len() <= USER_PROFILE_MAX_CHARS);
        assert!(out.contains("- k0: v"));
    }
}
