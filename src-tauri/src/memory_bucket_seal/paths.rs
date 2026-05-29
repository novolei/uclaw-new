//! Content-file path generation.
//!
//! Each chunk body is stored as a `.md` file under `<content_root>/`. The path
//! structure depends on the source kind:
//!
//! ```text
//! Email:    <content_root>/email/<participants_slug>/<chunk_id>.md
//! Chat:     <content_root>/chat/<source_slug>/<chunk_id>.md
//! Document: <content_root>/document/<source_slug>/<chunk_id>.md
//! ```
//!
//! Faithful port of `openhuman::memory::tree::content_store::paths` minus
//! summary-tree path helpers (deferred to PR8).

use std::path::{Path, PathBuf};

use crate::memory_bucket_seal::util::redact;

/// Build the relative content path for a chunk, using forward slashes.
///
/// Path layout depends on source_kind:
/// - Email:    `"email/<participants_slug>/<chunk_id>.md"`
///   Parses `source_id` as `gmail:{participants}` (two colon-separated parts)
///   where `participants` is `addr1|addr2|...` (sorted, deduped, lowercased).
///   The entire participants string is slugified as a single unit to produce
///   one folder level per conversation set (no nested thread subfolder).
///   If the source_id lacks a `gmail:` prefix or has no participants segment,
///   falls through to the chat/document layout using `slugify_source_id(source_id)`.
/// - Chat:     `"chat/<source_slug>/<chunk_id>.md"`
/// - Document: `"document/<source_slug>/<chunk_id>.md"`
///
/// `chunk_id` — the deterministic content hash produced by `types::chunk_id`.
pub fn chunk_rel_path(source_kind: &str, source_id: &str, chunk_id: &str) -> String {
    // Sanitize chunk_id into a cross-platform filename. Chunk IDs may contain
    // characters that are illegal on Windows NTFS; replace them with `-`.
    let filename = sanitize_filename(chunk_id);
    match source_kind {
        "email" => {
            // Expected format: "gmail:{participants}"
            // Split on ':' — exactly 2 parts required; part[0] == "gmail".
            let parts: Vec<&str> = source_id.splitn(2, ':').collect();
            if parts.len() == 2 && parts[0] == "gmail" && !parts[1].is_empty() {
                let participants_slug = slugify_source_id(parts[1]);
                format!("email/{}/{}.md", participants_slug, filename)
            } else {
                // Malformed / legacy source_id — fall back to flat layout.
                // Redact the source_id before logging since it may embed email
                // addresses.
                tracing::debug!(
                    source_id_hash = %redact(source_id),
                    "memory_bucket_seal::paths: email source_id has unexpected format, falling back to flat layout"
                );
                let slug = slugify_source_id(source_id);
                format!("email/{}/{}.md", slug, filename)
            }
        }
        _ => {
            // Chat, Document, and any future kinds use a 3-level layout.
            let slug = slugify_source_id(source_id);
            format!("{}/{}/{}.md", source_kind, slug, filename)
        }
    }
}

/// Build the absolute on-disk path for a chunk given the content root.
pub fn chunk_abs_path(
    content_root: &Path,
    source_kind: &str,
    source_id: &str,
    chunk_id: &str,
) -> PathBuf {
    let rel = chunk_rel_path(source_kind, source_id, chunk_id);
    // Convert forward-slash relative path to OS-native path.
    let mut abs = content_root.to_path_buf();
    for component in rel.split('/') {
        abs.push(component);
    }
    abs
}

/// Convert a raw `source_id` (e.g. `"slack:#general"`, `"gmail:thread/abc"`)
/// into a filesystem-safe slug using only `[a-z0-9_-]` characters.
///
/// Rules:
/// - lowercase the whole string
/// - replace any character outside `[a-z0-9_-]` with `-`
/// - collapse consecutive `-` to one
/// - trim leading/trailing `-`
/// - `_` is preserved anywhere in the string (interior underscores are kept)
/// - truncate to 120 characters
pub fn slugify_source_id(source_id: &str) -> String {
    let lower = source_id.to_lowercase();
    let mut out = String::with_capacity(lower.len().min(120));
    let mut last_dash = true; // avoids leading dash; also suppresses leading underscore runs
    let mut pending_underscore = false; // deferred `_` to avoid leading underscore

    for ch in lower.chars() {
        if ch == '_' {
            // Defer underscores — emit only if we have already emitted a
            // non-separator character (so `_solo_` becomes `_solo_` once the
            // `s` is emitted, but a leading `_` is dropped).
            if !last_dash {
                // We have real content before this, so emit the underscore now.
                pending_underscore = true;
            }
            // If last_dash is true (nothing emitted yet), silently skip.
        } else if ch.is_ascii_alphanumeric() {
            if pending_underscore {
                out.push('_');
                pending_underscore = false;
            }
            out.push(ch);
            last_dash = false;
        } else {
            // Non-alphanumeric, non-underscore → convert to `-`.
            pending_underscore = false; // drop any pending underscore before a dash
            if !last_dash {
                out.push('-');
                last_dash = true;
            }
        }
    }
    // trailing underscore: drop it (trim trailing separators).
    // trim trailing dash
    let trimmed = out.trim_end_matches('-');
    // also trim any trailing underscore
    let trimmed = trimmed.trim_end_matches('_');
    let truncated = truncate_at_char(trimmed, 120);
    if truncated.is_empty() {
        "unknown".to_string()
    } else {
        truncated.to_string()
    }
}

/// Replace characters that are illegal in filenames on Windows NTFS with `-`.
///
/// Illegal characters: `\`, `/`, `:`, `*`, `?`, `"`, `<`, `>`, `|`.
pub(crate) fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c => c,
        })
        .collect()
}

/// Truncate `s` to at most `max_chars` Unicode code points.
fn truncate_at_char(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── slugify tests ────────────────────────────────────────────────────────

    #[test]
    fn slugify_slack_channel() {
        assert_eq!(slugify_source_id("slack:#general"), "slack-general");
    }

    #[test]
    fn slugify_gmail_thread() {
        assert_eq!(
            slugify_source_id("gmail:thread/abc-123"),
            "gmail-thread-abc-123"
        );
    }

    #[test]
    fn slugify_collapses_consecutive_separators() {
        assert_eq!(slugify_source_id("foo::bar"), "foo-bar");
    }

    #[test]
    fn slugify_uppercase_lowercased() {
        assert_eq!(slugify_source_id("Slack:ABC"), "slack-abc");
    }

    #[test]
    fn slugify_empty_falls_back_to_unknown() {
        assert_eq!(slugify_source_id(""), "unknown");
        assert_eq!(slugify_source_id(":::"), "unknown");
    }

    // ─── chunk_rel_path tests ─────────────────────────────────────────────────

    #[test]
    fn chunk_rel_path_chat() {
        let p = chunk_rel_path("chat", "slack:#eng", "xyz789");
        assert_eq!(p, "chat/slack-eng/xyz789.md");
    }

    #[test]
    fn chunk_rel_path_email_well_formed() {
        let p = chunk_rel_path("email", "gmail:alice@x.com|bob@y.com", "abc");
        assert_eq!(p, "email/alice-x-com-bob-y-com/abc.md");
    }

    #[test]
    fn chunk_rel_path_email_malformed_fallback() {
        // Malformed: no `gmail:` prefix → flat fallback.
        let p = chunk_rel_path("email", "legacyid", "xyz");
        assert!(p.starts_with("email/"), "must remain under email/");
        assert!(p.ends_with("/xyz.md"), "chunk_id must be the filename");
    }

    #[test]
    fn chunk_rel_path_document() {
        let p = chunk_rel_path("document", "doc:notes.md", "uvw");
        assert_eq!(p, "document/doc-notes-md/uvw.md");
    }

    #[test]
    fn chunk_abs_path_resolves_under_root() {
        let root = Path::new("/workspace/content");
        let abs = chunk_abs_path(root, "email", "gmail:alice@x.com|bob@y.com", "abc");
        assert!(abs.starts_with(root));
        assert!(abs.ends_with("abc.md"));
    }
}
