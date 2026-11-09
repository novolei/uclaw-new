// SPDX-License-Identifier: Apache-2.0
//! Context continuity P2 — the compaction "session block": the fixed block
//! that replaces compacted history. It carries the structured summary plus
//! the persisted file working set (`<file_working_set>`), the current plan
//! (`<current_plan>`) and the current contents of the most recently modified
//! files (`<recent_files>`), so after compaction the agent still knows which
//! files it changed and what they look like now.
//!
//! Tag format (the frontend's compaction card parses these):
//! ```text
//! <current_plan path="…">body</current_plan>   |  <current_plan path="…" omitted="…"/>
//! <recent_files>
//! <file path="…">content</file>                 |  <file path="…" omitted="reason"/>
//! </recent_files>
//! ```
//! Attribute values escape `&` and `"` (`&amp;`, `&quot;`).

use std::io::Read;
use std::path::{Path, PathBuf};

use uclaw_message_types::estimate_tokens;

use crate::agent::file_working_set::FileWorkingSet;

/// A reloaded file's (or the plan's) body above this is given as a path only.
pub const RELOAD_FILE_MAX_TOKENS: usize = 5_000;
/// Hard ceiling on the session block (the other bound is `threshold / 4`).
pub const SESSION_BLOCK_MAX_TOKENS: usize = 25_000;

/// `omitted` reasons.
pub const OMIT_DELETED: &str = "已删除";
pub const OMIT_BINARY: &str = "二进制文件";
pub const OMIT_OVERSIZED: &str = "超过 5K token";
/// Not a regular file, unresolvable relative path (no workspace) or an IO error.
pub const OMIT_UNREADABLE: &str = "无法读取";
/// Dropped by `build_session_block` to fit the block budget.
pub const OMIT_OVER_BUDGET: &str = "超出会话块预算";

/// Bytes inspected for a NUL (binary sniff).
const BINARY_SNIFF_BYTES: usize = 8 * 1024;
/// Files larger than this are oversized without being read (any text this
/// large is far above `RELOAD_FILE_MAX_TOKENS`).
const MAX_READ_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadedFile {
    Content { path: String, text: String },
    PathOnly { path: String, reason: &'static str },
}

impl ReloadedFile {
    pub fn path(&self) -> &str {
        match self {
            Self::Content { path, .. } | Self::PathOnly { path, .. } => path,
        }
    }
}

/// What the compaction path needs from the session to assemble the block
/// (`LoopDelegate::session_block_context`).
#[derive(Debug, Clone, Default)]
pub struct SessionBlockContext {
    pub working_set: FileWorkingSet,
    pub workspace_root: Option<PathBuf>,
}

/// `budget_tokens = min(25_000, threshold / 4)`.
pub fn session_block_budget(threshold: usize) -> usize {
    SESSION_BLOCK_MAX_TOKENS.min(threshold / 4)
}

/// Resolve a working-set path: absolute stays absolute; relative joins the
/// workspace root. No globbing / expansion. `None` = relative without a
/// (non-empty) workspace root.
pub(crate) fn resolve_path(workspace_root: Option<&Path>, path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    let root = workspace_root.filter(|r| !r.as_os_str().is_empty())?;
    Some(root.join(p))
}

/// Read a regular UTF-8 text file of at most `RELOAD_FILE_MAX_TOKENS`.
/// `Err(reason)` otherwise; never panics, never reads non-regular files
/// (a FIFO would block).
pub(crate) fn read_text_file(path: &Path) -> Result<String, &'static str> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(OMIT_DELETED),
        Err(_) => return Err(OMIT_UNREADABLE),
    };
    if !meta.is_file() {
        return Err(OMIT_UNREADABLE);
    }
    let mut bytes = Vec::new();
    let file = std::fs::File::open(path).map_err(|_| OMIT_UNREADABLE)?;
    file.take(MAX_READ_BYTES + 1).read_to_end(&mut bytes).map_err(|_| OMIT_UNREADABLE)?;
    let sniff = &bytes[..bytes.len().min(BINARY_SNIFF_BYTES)];
    if sniff.contains(&0) {
        return Err(OMIT_BINARY);
    }
    if bytes.len() as u64 > MAX_READ_BYTES {
        return Err(OMIT_OVERSIZED);
    }
    let text = String::from_utf8(bytes).map_err(|_| OMIT_BINARY)?;
    if estimate_tokens(&text) as usize > RELOAD_FILE_MAX_TOKENS {
        return Err(OMIT_OVERSIZED);
    }
    Ok(text)
}

/// The current contents of the `max_files` most recently modified files
/// (`ws.modified` is already newest-first). Deleted files are not read;
/// binary / non-UTF-8 / oversized / unreadable ones are given as a path only.
pub fn reload_recent_files(
    ws: &FileWorkingSet,
    workspace_root: Option<&Path>,
    max_files: usize,
) -> Vec<ReloadedFile> {
    ws.modified
        .iter()
        .take(max_files)
        .map(|m| {
            let path = m.path.clone();
            if m.deleted {
                return ReloadedFile::PathOnly { path, reason: OMIT_DELETED };
            }
            let Some(abs) = resolve_path(workspace_root, &m.path) else {
                return ReloadedFile::PathOnly { path, reason: OMIT_UNREADABLE };
            };
            match read_text_file(&abs) {
                Ok(text) => ReloadedFile::Content { path, text },
                Err(reason) => ReloadedFile::PathOnly { path, reason },
            }
        })
        .collect()
}

fn attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;")
}

fn push_body(out: &mut String, body: &str) {
    out.push_str(body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
}

/// Working-set markdown keeping only its first `keep` inner lines, marked.
fn truncate_working_set(md: &str, keep: usize) -> String {
    let lines: Vec<&str> = md.lines().collect();
    if lines.len() < 2 {
        return md.to_string();
    }
    let inner = &lines[1..lines.len() - 1];
    if keep >= inner.len() {
        return md.to_string();
    }
    let mut out = String::new();
    out.push_str(lines[0]);
    out.push('\n');
    for l in &inner[..keep] {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(&format!("…（{}，已截断）\n", OMIT_OVER_BUDGET));
    out.push_str(lines[lines.len() - 1]);
    out
}

fn render_block(
    fold_markdown: &str,
    working_set_md: &str,
    plan: Option<(&str, &str)>,
    plan_body: bool,
    files: &[ReloadedFile],
    content_keep: usize,
) -> String {
    let mut out = String::from(fold_markdown.trim_end());
    if !working_set_md.is_empty() {
        out.push_str("\n\n");
        out.push_str(working_set_md);
    }
    if let Some((path, body)) = plan {
        out.push_str("\n\n");
        if !plan_body {
            out.push_str(&format!("<current_plan path=\"{}\" omitted=\"{}\"/>", attr(path), OMIT_OVER_BUDGET));
        } else if body.is_empty() {
            out.push_str(&format!("<current_plan path=\"{}\" omitted=\"正文未载入，需要时请读取\"/>", attr(path)));
        } else {
            out.push_str(&format!("<current_plan path=\"{}\">\n", attr(path)));
            push_body(&mut out, body);
            out.push_str("</current_plan>");
        }
    }
    if !files.is_empty() {
        out.push_str("\n\n<recent_files>\n");
        let mut contents_seen = 0usize;
        for f in files {
            match f {
                ReloadedFile::Content { path, text } => {
                    contents_seen += 1;
                    if contents_seen > content_keep {
                        out.push_str(&format!("<file path=\"{}\" omitted=\"{}\"/>\n", attr(path), OMIT_OVER_BUDGET));
                    } else {
                        out.push_str(&format!("<file path=\"{}\">\n", attr(path)));
                        push_body(&mut out, text);
                        out.push_str("</file>\n");
                    }
                }
                ReloadedFile::PathOnly { path, reason } => {
                    out.push_str(&format!("<file path=\"{}\" omitted=\"{}\"/>\n", attr(path), attr(reason)));
                }
            }
        }
        out.push_str("</recent_files>");
    }
    out
}

/// Assemble the session block: fold markdown + `\n\n` + working set +
/// `<current_plan>` + `<recent_files>`. `plan` = `(path, body)`; an empty
/// body renders path-only. Over `budget_tokens` it degrades in order:
/// recent file contents (least recent first) → plan body → working-set lines
/// (from the tail). The fold itself is never trimmed.
pub fn build_session_block(
    fold_markdown: &str,
    working_set_md: &str,
    plan: Option<(&str, &str)>,
    files: &[ReloadedFile],
    budget_tokens: usize,
) -> String {
    let fits = |s: &str| estimate_tokens(s) as usize <= budget_tokens;
    let n_content = files.iter().filter(|f| matches!(f, ReloadedFile::Content { .. })).count();

    for keep in (0..=n_content).rev() {
        let s = render_block(fold_markdown, working_set_md, plan, true, files, keep);
        if fits(&s) {
            return s;
        }
    }
    let s = render_block(fold_markdown, working_set_md, plan, false, files, 0);
    if fits(&s) || working_set_md.is_empty() {
        return s;
    }
    let inner = working_set_md.lines().count().saturating_sub(2);
    for keep in (0..inner).rev() {
        let ws = truncate_working_set(working_set_md, keep);
        let s = render_block(fold_markdown, &ws, plan, false, files, 0);
        if fits(&s) || keep == 0 {
            return s;
        }
    }
    s
}

/// The whole block for one compaction: `fold_markdown` alone without a
/// context; otherwise the working set, active plan and up to
/// `reload_files` recent files within `budget_tokens`. Returns the block
/// text and the number of files whose content was reloaded.
pub fn assemble(
    fold_markdown: &str,
    ctx: Option<&SessionBlockContext>,
    reload_files: usize,
    budget_tokens: usize,
) -> (String, usize) {
    let Some(ctx) = ctx else { return (fold_markdown.to_string(), 0) };
    let root = ctx.workspace_root.as_deref();
    let ws_budget = budget_tokens.saturating_sub(estimate_tokens(fold_markdown) as usize);
    let ws_md = crate::agent::file_working_set::render(&ctx.working_set, ws_budget);
    let plan_body = ctx
        .working_set
        .active_plan
        .as_deref()
        .map(|p| (p, crate::agent::plan_state::read_plan(root, p).unwrap_or_default()));
    let files = reload_recent_files(&ctx.working_set, root, reload_files);
    let block = build_session_block(
        fold_markdown,
        &ws_md,
        plan_body.as_ref().map(|(p, b)| (*p, b.as_str())),
        &files,
        budget_tokens,
    );
    let reloaded = files
        .iter()
        .filter(|f| match f {
            ReloadedFile::Content { path, .. } => block.contains(&format!("<file path=\"{}\">", attr(path))),
            ReloadedFile::PathOnly { .. } => false,
        })
        .count();
    (block, reloaded)
}

#[cfg(test)]
mod session_block_tests {
    use super::*;
    use crate::agent::file_working_set::ModifiedFile;
    use tempfile::TempDir;

    fn modified(path: &str, deleted: bool, at: i64) -> ModifiedFile {
        ModifiedFile {
            path: path.into(),
            created: false,
            deleted,
            edit_count: 1,
            changes: vec![],
            last_modified_at: at,
            rewound: false,
        }
    }

    fn ws(files: Vec<ModifiedFile>) -> FileWorkingSet {
        FileWorkingSet { modified: files, read: vec![], active_plan: None }
    }

    #[test]
    fn reload_skips_deleted_binary_and_oversized_files() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::write(root.join("ok.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("bin.dat"), b"ab\0cd").unwrap();
        std::fs::write(root.join("latin1.txt"), [0xff, 0xfe, 0x41]).unwrap();
        std::fs::write(root.join("big.txt"), "word ".repeat(30_000)).unwrap();
        std::fs::create_dir(root.join("adir")).unwrap();
        let abs = root.join("ok.rs").to_string_lossy().into_owned();
        let set = ws(vec![
            modified("ok.rs", false, 9),
            modified("gone.rs", true, 8),
            modified("vanished.rs", false, 7), // not marked deleted, but missing on disk
            modified("bin.dat", false, 6),
            modified("latin1.txt", false, 5),
            modified("big.txt", false, 4),
            modified("adir", false, 3),
            modified(&abs, false, 2),
        ]);

        let got = reload_recent_files(&set, Some(root), 10);

        assert_eq!(
            got,
            vec![
                ReloadedFile::Content { path: "ok.rs".into(), text: "fn main() {}\n".into() },
                ReloadedFile::PathOnly { path: "gone.rs".into(), reason: OMIT_DELETED },
                ReloadedFile::PathOnly { path: "vanished.rs".into(), reason: OMIT_DELETED },
                ReloadedFile::PathOnly { path: "bin.dat".into(), reason: OMIT_BINARY },
                ReloadedFile::PathOnly { path: "latin1.txt".into(), reason: OMIT_BINARY },
                ReloadedFile::PathOnly { path: "big.txt".into(), reason: OMIT_OVERSIZED },
                ReloadedFile::PathOnly { path: "adir".into(), reason: OMIT_UNREADABLE },
                ReloadedFile::Content { path: abs.clone(), text: "fn main() {}\n".into() },
            ]
        );
        // A relative path without a workspace root is never resolved against the CWD.
        assert_eq!(
            reload_recent_files(&ws(vec![modified("ok.rs", false, 1)]), None, 5),
            vec![ReloadedFile::PathOnly { path: "ok.rs".into(), reason: OMIT_UNREADABLE }]
        );
    }

    #[test]
    fn reload_takes_most_recent_modified_up_to_max() {
        let dir = TempDir::new().unwrap();
        for n in ["a", "b", "c"] {
            std::fs::write(dir.path().join(n), n).unwrap();
        }
        let set = ws(vec![modified("c", false, 3), modified("b", false, 2), modified("a", false, 1)]);
        let got = reload_recent_files(&set, Some(dir.path()), 2);
        assert_eq!(got.iter().map(|f| f.path()).collect::<Vec<_>>(), vec!["c", "b"]);
        assert!(reload_recent_files(&set, Some(dir.path()), 0).is_empty());
    }

    const WS_MD: &str = "<file_working_set>\n已修改的文件：\n- a.rs（修改 1 次）\n- b.rs（新建）\n</file_working_set>";

    #[test]
    fn session_block_contains_summary_working_set_plan_and_files() {
        let files = vec![
            ReloadedFile::Content { path: "a.rs".into(), text: "AAA".into() },
            ReloadedFile::PathOnly { path: "b \"q\".bin".into(), reason: OMIT_BINARY },
        ];
        let block = build_session_block(
            "## Earlier conversation (compacted)\n- SUMMARY",
            WS_MD,
            Some((".uclaw/plans/p.md", "- [ ] step one")),
            &files,
            25_000,
        );
        let expected = "## Earlier conversation (compacted)\n- SUMMARY\n\n\
            <file_working_set>\n已修改的文件：\n- a.rs（修改 1 次）\n- b.rs（新建）\n</file_working_set>\n\n\
            <current_plan path=\".uclaw/plans/p.md\">\n- [ ] step one\n</current_plan>\n\n\
            <recent_files>\n<file path=\"a.rs\">\nAAA\n</file>\n\
            <file path=\"b &quot;q&quot;.bin\" omitted=\"二进制文件\"/>\n</recent_files>";
        assert_eq!(block, expected);

        // Path-only plan; nothing else.
        let block = build_session_block("S", "", Some(("p.md", "")), &[], 25_000);
        assert_eq!(block, "S\n\n<current_plan path=\"p.md\" omitted=\"正文未载入，需要时请读取\"/>");
        assert_eq!(build_session_block("S", "", None, &[], 25_000), "S");
    }

    #[test]
    fn session_block_respects_budget_trim_order() {
        let big = |c: &str| c.repeat(4_000); // ~1K tokens each
        let files = vec![
            ReloadedFile::Content { path: "new.rs".into(), text: big("n") },
            ReloadedFile::Content { path: "old.rs".into(), text: big("o") },
        ];
        let plan_body = big("p");
        let fold = "SUMMARY";
        let tokens = |s: &str| estimate_tokens(s) as usize;
        let full = build_session_block(fold, WS_MD, Some(("p.md", &plan_body)), &files, 100_000);
        let full_t = tokens(&full);

        // 1) Least recent file content dropped first.
        let b = build_session_block(fold, WS_MD, Some(("p.md", &plan_body)), &files, full_t - 10);
        assert!(b.contains("<file path=\"new.rs\">"));
        assert!(b.contains("<file path=\"old.rs\" omitted=\"超出会话块预算\"/>"));
        assert!(b.contains("<current_plan path=\"p.md\">"));

        // 2) Then all file contents, then the plan body.
        let b = build_session_block(fold, WS_MD, Some(("p.md", &plan_body)), &files, 600);
        assert!(b.contains("<file path=\"new.rs\" omitted=\"超出会话块预算\"/>"));
        assert!(b.contains("<current_plan path=\"p.md\" omitted=\"超出会话块预算\"/>"));
        assert!(b.contains("- b.rs（新建）"), "working set intact");
        assert!(tokens(&b) <= 600);

        // 3) Finally working-set lines from the tail; the summary is never trimmed.
        let small = tokens(&build_session_block(fold, WS_MD, Some(("p.md", &plan_body)), &files, 600)) - 3;
        let b = build_session_block(fold, WS_MD, Some(("p.md", &plan_body)), &files, small);
        assert!(b.starts_with("SUMMARY"));
        assert!(b.contains("<file_working_set>") && b.contains("</file_working_set>"));
        assert!(b.contains("已截断"));
        assert!(!b.contains("- b.rs（新建）"));
    }

    #[test]
    fn budget_is_min_of_cap_and_quarter_threshold() {
        assert_eq!(session_block_budget(128_000), 25_000);
        assert_eq!(session_block_budget(32_000), 8_000);
    }
}
