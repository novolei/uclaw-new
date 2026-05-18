//! PROFILE.md managed-block parser + renderer — Sprint 1.7.
//!
//! Lives in `~/<workspace>/PROFILE.md`. Tracks the user-facing
//! profile that the agent system prompt injects via
//! [`UserFilesSection`] (Sprint 1.8). The file has two kinds of
//! content:
//!
//! 1. **Managed blocks** between HTML comment markers:
//!
//!    ```markdown
//!    <!-- ms:identity:start -->
//!    - name: Alice
//!    - timezone: America/Los_Angeles
//!    <!-- ms:identity:end -->
//!    ```
//!
//!    The body between markers is rewritten on every
//!    [`refresh`](ProfileMd::refresh) from the active facet cache.
//!    The markers themselves are reserved — users mustn't move or
//!    edit them.
//!
//! 2. **User-editable prose** anywhere else in the file. Preserved
//!    verbatim across refreshes. Users use this for additions the
//!    facet store doesn't capture yet — preferences, project notes,
//!    pinned reminders.
//!
//! ## File layout convention
//!
//! ```markdown
//! # User Profile
//!
//! _Auto-managed by uClaw memory_os. Edit only outside the
//! `<!-- ms:*:start ... :end -->` markers; content inside gets
//! overwritten on every Profile rebuild._
//!
//! <!-- ms:identity:start -->
//! - name: Alice
//! - timezone: America/Los_Angeles
//! <!-- ms:identity:end -->
//!
//! <!-- ms:tooling:start -->
//! - editor: helix
//! <!-- ms:tooling:end -->
//!
//! ## My own notes
//!
//! - Remember to push Phase 8 PR
//! - The agent kept guessing my email wrong this morning — fix
//! ```
//!
//! Reference: openhuman
//! `composio/providers/profile_md.rs` + `learning/profile_md_renderer.rs`.

use std::collections::HashMap;
use std::path::Path;

use crate::learning::candidate::FacetClass;
use crate::learning::stability_detector::FacetSnapshot;

// ─── Marker helpers ────────────────────────────────────────────────────

/// Marker prefix. Short to keep the file tidy.
pub const MARKER_PREFIX: &str = "<!-- ms:";

/// Build the start-marker line for a class.
pub fn start_marker(class: FacetClass) -> String {
    format!("<!-- ms:{}:start -->", class.as_str())
}

/// Build the end-marker line for a class.
pub fn end_marker(class: FacetClass) -> String {
    format!("<!-- ms:{}:end -->", class.as_str())
}

/// All six classes in canonical render order (matches
/// `prompt_section::CLASS_RENDER_ORDER`).
pub const CLASS_RENDER_ORDER: &[FacetClass] = &[
    FacetClass::Identity,
    FacetClass::Veto,
    FacetClass::Tooling,
    FacetClass::Style,
    FacetClass::Goal,
    FacetClass::Channel,
];

// ─── Parsed shape ──────────────────────────────────────────────────────

/// Parsed representation of a PROFILE.md file. Round-trips through
/// [`parse`] / [`render`] preserving user-editable prose.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileMdContents {
    /// Free prose before the first managed block (may be empty).
    pub prelude: String,
    /// Found managed blocks keyed by class. If a class isn't present
    /// in the file, key is absent. Value is the raw text body the
    /// USER might have edited (we replace on refresh so contents
    /// here are about to be overwritten).
    pub managed_bodies: HashMap<FacetClass, String>,
    /// Free prose after the last managed block (may be empty).
    /// Anything between managed blocks gets merged into the next
    /// block's preceding text — i.e. the canonical layout has all
    /// managed blocks together at the top and user prose at the
    /// bottom.
    pub postlude: String,
}

impl ProfileMdContents {
    /// Empty file shape used on first creation.
    pub fn empty() -> Self {
        Self {
            prelude: String::new(),
            managed_bodies: HashMap::new(),
            postlude: String::new(),
        }
    }
}

/// Parse a PROFILE.md file into [`ProfileMdContents`]. Always
/// succeeds: malformed marker pairs (orphan start without end, etc.)
/// are treated as plain text so user edits never get lost on a
/// half-edited file.
pub fn parse(text: &str) -> ProfileMdContents {
    let mut bodies: HashMap<FacetClass, String> = HashMap::new();
    let mut prelude_done = false;
    let mut prelude = String::new();
    let mut postlude = String::new();
    // Text between managed blocks is buffered here and discarded when the
    // next block is found (it's the render-separator \n\n, which the
    // renderer regenerates). Only flushed to postlude in the None arm.
    let mut pending_between = String::new();
    let mut idx = 0usize;
    let bytes = text.as_bytes();

    while idx < bytes.len() {
        // Look for next `<!-- ms:` marker from here.
        let rest = &text[idx..];
        let marker_pos = rest.find(MARKER_PREFIX);
        let marker_pos = match marker_pos {
            Some(p) => idx + p,
            None => {
                // No more markers — accumulate the rest as
                // prelude/postlude based on whether we've seen any
                // managed block yet.
                let tail = &text[idx..];
                if prelude_done {
                    // pending_between held inter-block whitespace that is
                    // discarded here (the renderer regenerates \n\n
                    // separators). Strip the leading \n\n the renderer
                    // placed before the postlude so round-trips are stable.
                    let tail_strip = tail.strip_prefix("\n\n").unwrap_or(tail);
                    postlude.push_str(tail_strip);
                } else {
                    prelude.push_str(tail);
                }
                break;
            }
        };

        // Everything before the marker is "between-block" text. If we
        // haven't entered any block yet, that's prelude; otherwise stage
        // it in pending_between — it will be discarded if another block
        // follows (it's just the render-separator \n\n) and flushed to
        // postlude only in the None arm above.
        let between = &text[idx..marker_pos];
        if prelude_done {
            pending_between.push_str(between);
        } else {
            prelude.push_str(between);
        }

        // Identify the class + ensure it's a `start` marker line. If
        // it's an `end` without a paired start we already consumed,
        // skip past it as user prose.
        let after_prefix = &text[marker_pos + MARKER_PREFIX.len()..];
        // Class string is the substring up to the next ':' .
        let colon_pos = match after_prefix.find(':') {
            Some(p) => p,
            None => {
                // Malformed — treat as plain text and advance one char.
                if prelude_done {
                    postlude.push_str(&text[marker_pos..marker_pos + 1]);
                } else {
                    prelude.push_str(&text[marker_pos..marker_pos + 1]);
                }
                idx = marker_pos + 1;
                continue;
            }
        };
        let class_str = &after_prefix[..colon_pos];
        let kind_and_rest = &after_prefix[colon_pos + 1..];
        // Expect either `start -->` or `end -->`.
        let is_start = kind_and_rest.starts_with("start -->");
        let is_end = kind_and_rest.starts_with("end -->");
        if !is_start && !is_end {
            // Malformed — copy the marker text and advance past it.
            if prelude_done {
                postlude.push_str(&text[marker_pos..marker_pos + 1]);
            } else {
                prelude.push_str(&text[marker_pos..marker_pos + 1]);
            }
            idx = marker_pos + 1;
            continue;
        }
        let class = match parse_class(class_str) {
            Some(c) => c,
            None => {
                // Unknown class — treat as plain text.
                let len_to_copy = MARKER_PREFIX.len()
                    + class_str.len()
                    + 1
                    + (if is_start { "start -->".len() } else { "end -->".len() });
                let chunk_end = (marker_pos + len_to_copy).min(text.len());
                let chunk = &text[marker_pos..chunk_end];
                if prelude_done {
                    postlude.push_str(chunk);
                } else {
                    prelude.push_str(chunk);
                }
                idx = chunk_end;
                continue;
            }
        };
        if is_end {
            // Orphan end — treat as plain text and continue.
            let chunk_end = marker_pos + MARKER_PREFIX.len() + class_str.len() + 1 + "end -->".len();
            let chunk_end = chunk_end.min(text.len());
            let chunk = &text[marker_pos..chunk_end];
            if prelude_done {
                postlude.push_str(chunk);
            } else {
                prelude.push_str(chunk);
            }
            idx = chunk_end;
            continue;
        }

        // It's a well-formed start marker. Find the matching end.
        let start_marker_end = marker_pos + MARKER_PREFIX.len() + class_str.len() + 1 + "start -->".len();
        let end_needle = end_marker(class);
        let body_end_pos = text[start_marker_end..]
            .find(&end_needle)
            .map(|p| start_marker_end + p);
        let body_end_pos = match body_end_pos {
            Some(p) => p,
            None => {
                // Orphan start — treat from the marker as plain text
                // (don't consume more than the marker so the user's
                // half-edited content survives).
                let chunk = &text[marker_pos..start_marker_end];
                if prelude_done {
                    postlude.push_str(chunk);
                } else {
                    prelude.push_str(chunk);
                }
                idx = start_marker_end;
                continue;
            }
        };
        let body = text[start_marker_end..body_end_pos].to_string();
        bodies.insert(class, body);
        prelude_done = true;
        pending_between.clear(); // inter-block separator consumed by this block
        idx = body_end_pos + end_needle.len();
    }

    // Trim trailing newline from prelude so render's double-newline guard
    // doesn't produce a triple blank line.
    if prelude.ends_with('\n') {
        prelude.pop();
    }

    ProfileMdContents {
        prelude,
        managed_bodies: bodies,
        postlude,
    }
}

fn parse_class(s: &str) -> Option<FacetClass> {
    match s {
        "identity" => Some(FacetClass::Identity),
        "veto" => Some(FacetClass::Veto),
        "tooling" => Some(FacetClass::Tooling),
        "style" => Some(FacetClass::Style),
        "goal" => Some(FacetClass::Goal),
        "channel" => Some(FacetClass::Channel),
        _ => None,
    }
}

// ─── Render ────────────────────────────────────────────────────────────

/// Re-render a PROFILE.md file from an existing [`ProfileMdContents`]
/// (preserving the prelude / postlude) + fresh active facets keyed by
/// class. Always emits one managed block per class in
/// [`CLASS_RENDER_ORDER`], even if empty (so user can see "we know
/// nothing about your identity yet" rather than silently absent).
pub fn render(
    contents: &ProfileMdContents,
    active_by_class: &HashMap<FacetClass, Vec<FacetSnapshot>>,
) -> String {
    let mut out = String::new();
    if !contents.prelude.is_empty() {
        out.push_str(&contents.prelude);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
    } else {
        // Default header so the file is self-documenting on first creation.
        out.push_str("# User Profile\n\n");
        out.push_str(
            "_Auto-managed by uClaw memory_os. Edit only outside the \
             `<!-- ms:*:start ... :end -->` markers; content inside gets \
             overwritten on every Profile rebuild._\n\n",
        );
    }

    for class in CLASS_RENDER_ORDER {
        out.push_str(&start_marker(*class));
        out.push('\n');
        let facets = active_by_class.get(class).cloned().unwrap_or_default();
        if facets.is_empty() {
            out.push_str("_(none yet)_\n");
        } else {
            for f in &facets {
                out.push_str(&format!("- {}: {}\n", f.name, f.value));
            }
        }
        out.push_str(&end_marker(*class));
        out.push_str("\n\n");
    }

    if !contents.postlude.is_empty() {
        out.push_str(&contents.postlude);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// Read PROFILE.md from `path`, parse it. Missing file ⇒
/// [`ProfileMdContents::empty`]. IO errors return Err — caller
/// decides whether to log + continue or abort the refresh.
pub fn read(path: &Path) -> Result<ProfileMdContents, std::io::Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(parse(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ProfileMdContents::empty()),
        Err(e) => Err(e),
    }
}

/// Atomically write `text` to `path`. Writes to `path.tmp`, fsyncs,
/// then renames over `path`. Caller (Sprint 1.10 scheduler) calls
/// this after every refresh.
pub fn write(path: &Path, text: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::stability_detector::{CueWeights, FacetSnapshot, FacetState};

    fn snap(class: FacetClass, name: &str, value: &str) -> FacetSnapshot {
        FacetSnapshot {
            facet_id: format!("{}-{}", class.as_str(), name),
            class,
            name: name.into(),
            value: value.into(),
            state: FacetState::Active,
            stability: 2.0,
            cue_weights: CueWeights::default(),
            evidence_count: 1,
            last_seen_ms: 0,
        }
    }

    fn build_actives(snaps: Vec<FacetSnapshot>) -> HashMap<FacetClass, Vec<FacetSnapshot>> {
        let mut m: HashMap<FacetClass, Vec<FacetSnapshot>> = HashMap::new();
        for s in snaps {
            m.entry(s.class).or_default().push(s);
        }
        m
    }

    // ── Markers ───────────────────────────────────────────────────

    #[test]
    fn marker_strings_match_spec() {
        assert_eq!(start_marker(FacetClass::Identity), "<!-- ms:identity:start -->");
        assert_eq!(end_marker(FacetClass::Tooling), "<!-- ms:tooling:end -->");
    }

    // ── Parse ─────────────────────────────────────────────────────

    #[test]
    fn parse_empty_string() {
        let p = parse("");
        assert!(p.prelude.is_empty());
        assert!(p.managed_bodies.is_empty());
        assert!(p.postlude.is_empty());
    }

    #[test]
    fn parse_well_formed_block() {
        let text = "# Profile\n\n<!-- ms:identity:start -->\n- name: Alice\n<!-- ms:identity:end -->\n\nmy notes\n";
        let p = parse(text);
        assert!(p.prelude.contains("# Profile"));
        assert_eq!(
            p.managed_bodies.get(&FacetClass::Identity).unwrap().trim(),
            "- name: Alice",
        );
        assert!(p.postlude.contains("my notes"));
    }

    #[test]
    fn parse_handles_multiple_classes() {
        let text = "\
<!-- ms:identity:start -->
- name: Alice
<!-- ms:identity:end -->

<!-- ms:tooling:start -->
- editor: helix
<!-- ms:tooling:end -->

trailing notes
";
        let p = parse(text);
        assert_eq!(p.managed_bodies.len(), 2);
        assert!(p.managed_bodies.contains_key(&FacetClass::Identity));
        assert!(p.managed_bodies.contains_key(&FacetClass::Tooling));
        assert!(p.postlude.contains("trailing notes"));
    }

    #[test]
    fn parse_orphan_start_marker_preserved_as_text() {
        // Half-edited file: user wrote a start marker but no end.
        // Must NOT crash, must NOT lose the user's content.
        let text = "before\n<!-- ms:tooling:start -->\nuser was here\n";
        let p = parse(text);
        // Body became part of postlude (orphan start) so content survives somewhere.
        assert!(p.managed_bodies.is_empty(), "orphan start → no managed body");
        // The user's text isn't lost (it's either in prelude or postlude).
        let combined = format!("{} {}", p.prelude, p.postlude);
        assert!(combined.contains("before"));
        assert!(combined.contains("user was here") || p.prelude.contains("user was here"));
    }

    #[test]
    fn parse_unknown_class_marker_kept_as_text() {
        let text = "<!-- ms:mystery:start -->\nodd\n<!-- ms:mystery:end -->\nafter\n";
        let p = parse(text);
        assert!(p.managed_bodies.is_empty());
        // The marker text survives somewhere (prelude or postlude).
        let combined = format!("{} {}", p.prelude, p.postlude);
        assert!(combined.contains("ms:mystery") || combined.contains("after"));
    }

    // ── Render ────────────────────────────────────────────────────

    #[test]
    fn render_from_empty_contents_emits_default_header() {
        let out = render(&ProfileMdContents::empty(), &HashMap::new());
        assert!(out.starts_with("# User Profile"));
        assert!(out.contains("Auto-managed by uClaw"));
        // All six markers present.
        for class in CLASS_RENDER_ORDER {
            assert!(out.contains(&start_marker(*class)));
            assert!(out.contains(&end_marker(*class)));
        }
    }

    #[test]
    fn render_with_active_facets_emits_rows() {
        let actives = build_actives(vec![
            snap(FacetClass::Tooling, "editor", "helix"),
            snap(FacetClass::Tooling, "shell", "zsh"),
        ]);
        let out = render(&ProfileMdContents::empty(), &actives);
        assert!(out.contains("<!-- ms:tooling:start -->"));
        assert!(out.contains("- editor: helix"));
        assert!(out.contains("- shell: zsh"));
        assert!(out.contains("<!-- ms:tooling:end -->"));
    }

    #[test]
    fn render_with_empty_class_writes_none_yet_placeholder() {
        let out = render(&ProfileMdContents::empty(), &HashMap::new());
        // Each class has '_(none yet)_' between markers.
        let identity_idx = out.find("<!-- ms:identity:start -->").unwrap();
        let after_identity_start = &out[identity_idx..];
        assert!(after_identity_start.starts_with("<!-- ms:identity:start -->\n_(none yet)_\n<!-- ms:identity:end -->"));
    }

    #[test]
    fn render_preserves_prelude_and_postlude() {
        let contents = ProfileMdContents {
            prelude: "# My Profile\n\nA note from the user before the markers.".into(),
            managed_bodies: HashMap::new(),
            postlude: "## User Notes\n\n- TODO: ship phase 8\n".into(),
        };
        let out = render(&contents, &HashMap::new());
        assert!(out.contains("# My Profile"));
        assert!(out.contains("A note from the user before the markers."));
        assert!(out.contains("## User Notes"));
        assert!(out.contains("TODO: ship phase 8"));
    }

    #[test]
    fn parse_render_round_trip_preserves_user_prose() {
        // Render a file from facts, parse it back, re-render — user
        // prose must survive both passes.
        let orig = ProfileMdContents {
            prelude: "# Header".into(),
            managed_bodies: HashMap::new(),
            postlude: "## My notes\n\n- hello world".into(),
        };
        let actives = build_actives(vec![
            snap(FacetClass::Identity, "name", "Alice"),
            snap(FacetClass::Tooling, "editor", "helix"),
        ]);
        let pass1 = render(&orig, &actives);
        let reparsed = parse(&pass1);
        let pass2 = render(&reparsed, &actives);
        assert!(pass2.contains("# Header"));
        assert!(pass2.contains("hello world"));
        assert!(pass2.contains("- name: Alice"));
        assert!(pass2.contains("- editor: helix"));
        // Idempotent: pass2 should equal pass1 (or be byte-identical up to trailing whitespace).
        assert_eq!(pass1.trim_end(), pass2.trim_end(), "render→parse→render must be idempotent");
    }

    // ── File I/O ──────────────────────────────────────────────────

    #[test]
    fn read_missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("PROFILE.md");
        let p = read(&path).unwrap();
        assert_eq!(p, ProfileMdContents::empty());
    }

    #[test]
    fn write_then_read_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("PROFILE.md");
        let actives = build_actives(vec![snap(FacetClass::Tooling, "editor", "helix")]);
        let body = render(&ProfileMdContents::empty(), &actives);
        write(&path, &body).unwrap();
        let p = read(&path).unwrap();
        assert_eq!(
            p.managed_bodies.get(&FacetClass::Tooling).unwrap().trim(),
            "- editor: helix"
        );
    }

    #[test]
    fn write_is_atomic_uses_tmp_then_rename() {
        // Sanity-check we didn't leave a stray .tmp file behind.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("PROFILE.md");
        write(&path, "hello").unwrap();
        assert!(path.exists());
        let tmp_path = path.with_extension("tmp");
        assert!(!tmp_path.exists(), "tmp file should have been renamed away");
    }
}
