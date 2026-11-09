// SPDX-License-Identifier: Apache-2.0
//! Derive [`FileEvent`]s from a dispatched tool call (agent context
//! continuity, phase 2).
//!
//! Pure function of a [`ToolDispatchOutcome`]: no filesystem or DB access.
//! `ChatDelegate::execute_tool_calls` feeds the result to
//! [`file_working_set::record`](super::file_working_set::record) after each
//! tool batch in transcript mode. Failed or rejected calls produce nothing.

use std::path::Path;

use super::file_working_set::{normalize_path, FileEvent, FileOp};
use super::tool_dispatch::ToolDispatchOutcome;

/// Max characters of one side (`old` / `new`) of an edit summary.
const EDIT_SIDE_CHARS: usize = 80;
/// Max characters of a whole edit detail.
const EDIT_DETAIL_CHARS: usize = 600;

/// Derive the file events one tool call produced. Failed (`is_error` /
/// `Err`) or rejected calls produce none.
pub fn file_events_from_outcome(
    o: &ToolDispatchOutcome,
    workspace_root: Option<&Path>,
) -> Vec<FileEvent> {
    if o.is_error || o.rejected || o.result.is_err() {
        return Vec::new();
    }
    let norm = |raw: &str| normalize_path(raw, workspace_root);
    let args = &o.arguments;
    let arg_path = || args.get("path").and_then(|v| v.as_str()).map(norm).filter(|p| !p.is_empty());

    let events = match o.tool_name.as_str() {
        "read_file" => arg_path()
            .map(|path| vec![FileEvent { path, op: FileOp::Read, detail: read_range(args) }])
            .unwrap_or_default(),
        "write_file" => arg_path()
            .map(|path| {
                let lines = args.get("content").and_then(|v| v.as_str()).map(|c| c.lines().count()).unwrap_or(0);
                vec![FileEvent { path, op: FileOp::Write, detail: format!("写入 {lines} 行") }]
            })
            .unwrap_or_default(),
        "edit" => edit_events(o, workspace_root),
        "plan_write" => plan_write_path(o)
            .map(|p| vec![FileEvent { path: norm(&p), op: FileOp::Plan, detail: "创建计划".into() }])
            .unwrap_or_default(),
        "plan_update" => args
            .get("filename")
            .and_then(|v| v.as_str())
            .and_then(|f| Path::new(f).file_name())
            .map(|f| {
                let rel = Path::new(".uclaw").join("plans").join(f);
                let raw = match workspace_root {
                    Some(root) => root.join(rel),
                    None => rel,
                };
                vec![FileEvent {
                    path: norm(&raw.to_string_lossy()),
                    op: FileOp::Plan,
                    detail: "更新计划".into(),
                }]
            })
            .unwrap_or_default(),
        "bash" => bash_events(args, workspace_root),
        _ if o.was_mutation => o
            .paths_touched
            .iter()
            .map(|p| FileEvent { path: norm(&p.to_string_lossy()), op: FileOp::Edit, detail: o.tool_name.clone() })
            .collect(),
        _ => Vec::new(),
    };
    events.into_iter().filter(|e| !e.path.is_empty()).collect()
}

/// `read_file` takes a 1-based `offset` (default 1) and an optional `limit`.
fn read_range(args: &serde_json::Value) -> String {
    let offset = args.get("offset").and_then(|v| v.as_u64()).map(|v| v.max(1));
    let limit = args.get("limit").and_then(|v| v.as_u64()).filter(|&l| l > 0);
    match (offset.unwrap_or(1), limit) {
        (start, Some(l)) => format!("第 {}–{} 行", start, start + l - 1),
        (1, None) => "全文".to_string(),
        (start, None) => format!("第 {start} 行起"),
    }
}

/// `plan_write` reports `Plan created at <path>`.
fn plan_write_path(o: &ToolDispatchOutcome) -> Option<String> {
    let out = o.result.as_ref().ok()?;
    let text = out.result.get("content").and_then(|v| v.as_str()).unwrap_or(&o.message_content);
    let path = text.lines().find_map(|l| l.trim().strip_prefix("Plan created at "))?;
    Some(path.trim().to_string())
}

fn truncate_chars(s: &str, max: usize) -> (String, bool) {
    match s.char_indices().nth(max) {
        Some((idx, _)) => (s[..idx].to_string(), true),
        None => (s.to_string(), false),
    }
}

fn first_line_capped(s: &str) -> String {
    let first = s.lines().next().unwrap_or("");
    let (head, cut) = truncate_chars(first, EDIT_SIDE_CHARS);
    // A multi-line side whose first line fits is still shown without "…":
    // the arrow summary is a hint, not a diff.
    if cut { format!("{head}…") } else { head }
}

fn edit_summary(edits: Option<&serde_json::Value>) -> String {
    let parts: Vec<String> = edits
        .and_then(|e| e.as_array())
        .map(|edits| {
            edits
                .iter()
                .map(|e| {
                    let old = e.get("old_text").and_then(|v| v.as_str()).unwrap_or("");
                    let new = e.get("new_text").and_then(|v| v.as_str()).unwrap_or("");
                    if old.is_empty() {
                        format!("插入「{}」", first_line_capped(new))
                    } else {
                        format!("「{}」→「{}」", first_line_capped(old), first_line_capped(new))
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    truncate_chars(&parts.join("；"), EDIT_DETAIL_CHARS).0
}

/// One `Edit` per file in `paths_touched` (argument paths when the gate
/// produced none), each with the summary of that file's edits. Supports the
/// single `{path, edits}` and batch `{files: [{path, edits}]}` forms.
fn edit_events(o: &ToolDispatchOutcome, workspace_root: Option<&Path>) -> Vec<FileEvent> {
    let norm = |raw: &str| normalize_path(raw, workspace_root);
    let args = &o.arguments;
    // (normalized path, edits) pairs from the arguments.
    let entries: Vec<(String, Option<&serde_json::Value>)> =
        if let Some(files) = args.get("files").and_then(|f| f.as_array()) {
            files
                .iter()
                .filter_map(|f| f.get("path").and_then(|p| p.as_str()).map(|p| (norm(p), f.get("edits"))))
                .collect()
        } else {
            args.get("path")
                .and_then(|p| p.as_str())
                .map(|p| vec![(norm(p), args.get("edits"))])
                .unwrap_or_default()
        };
    let paths: Vec<String> = if o.paths_touched.is_empty() {
        entries.iter().map(|(p, _)| p.clone()).collect()
    } else {
        o.paths_touched.iter().map(|p| norm(&p.to_string_lossy())).collect()
    };
    let mut out: Vec<FileEvent> = Vec::new();
    for path in paths {
        if out.iter().any(|e| e.path == path) {
            continue;
        }
        let edits = entries.iter().find(|(p, _)| *p == path).and_then(|(_, e)| *e);
        out.push(FileEvent { detail: edit_summary(edits), path, op: FileOp::Edit });
    }
    out
}

/// Split a *simple* shell command into words. Returns `None` for anything
/// with shell syntax we cannot reason about statically (chaining, pipes,
/// redirection, substitution, globs, unbalanced quotes).
fn simple_words(cmd: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for c in cmd.trim().chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    in_word = true;
                }
                // An unquoted newline separates commands like `;`.
                '\n' | '\r' => return None,
                c if c.is_whitespace() => {
                    if in_word {
                        words.push(std::mem::take(&mut cur));
                        in_word = false;
                    }
                }
                ';' | '&' | '|' | '<' | '>' | '`' | '$' | '*' | '?' | '[' | ']' | '{' | '}' | '~' | '(' | ')'
                | '\\' | '#' => return None,
                c => {
                    cur.push(c);
                    in_word = true;
                }
            },
        }
    }
    if quote.is_some() {
        return None;
    }
    if in_word {
        words.push(cur);
    }
    Some(words)
}

/// Recognise only a simple `rm <paths>` (→ `Delete`) or `mv <a> <b>`
/// (→ `Delete a` + `Write b`). Everything else produces no events.
fn bash_events(args: &serde_json::Value, workspace_root: Option<&Path>) -> Vec<FileEvent> {
    let Some(cmd) = args.get("command").and_then(|v| v.as_str()) else { return Vec::new() };
    let Some(words) = simple_words(cmd) else { return Vec::new() };
    let Some((prog, rest)) = words.split_first() else { return Vec::new() };
    // Words before `--` that start with `-` are flags; every word after `--`
    // is an operand (`rm -- -weird` deletes `-weird`).
    let mut flags: Vec<&String> = Vec::new();
    let mut operands: Vec<&String> = Vec::new();
    let mut end_of_flags = false;
    for w in rest {
        if end_of_flags {
            operands.push(w);
        } else if w == "--" {
            end_of_flags = true;
        } else if w.starts_with('-') {
            flags.push(w);
        } else {
            operands.push(w);
        }
    }
    if operands.is_empty() {
        return Vec::new();
    }
    // Relative operands resolve against `working_dir` (itself relative to the root).
    let base: Option<std::path::PathBuf> =
        match args.get("working_dir").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
            Some(wd) if Path::new(wd).is_absolute() => Some(Path::new(wd).to_path_buf()),
            Some(wd) => workspace_root.map(|r| r.join(wd)).or_else(|| Some(Path::new(wd).to_path_buf())),
            None => None,
        };
    let norm = |raw: &str| {
        let p = Path::new(raw);
        match (&base, p.is_absolute()) {
            (Some(b), false) => normalize_path(&b.join(p).to_string_lossy(), workspace_root),
            _ => normalize_path(raw, workspace_root),
        }
    };
    match prog.as_str() {
        "rm" => operands
            .iter()
            .map(|p| FileEvent { path: norm(p), op: FileOp::Delete, detail: "rm 删除".into() })
            .collect(),
        // `-t DIR` / `--target-directory` reverse the operand roles and `-S`
        // takes a suffix argument: too ambiguous, record nothing.
        "mv" if flags.iter().any(|f| {
            f.as_str() == "-t"
                || f.as_str() == "-S"
                || f.starts_with("--target-directory")
                || f.starts_with("--suffix")
        }) => Vec::new(),
        "mv" if operands.len() == 2 => {
            let (from, to) = (norm(operands[0]), norm(operands[1]));
            vec![
                FileEvent { path: from.clone(), op: FileOp::Delete, detail: format!("移动到 {to}") },
                FileEvent { path: to, op: FileOp::Write, detail: format!("由 {from} 移动而来") },
            ]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::tool::{ToolError, ToolOutput};
    use serde_json::json;
    use std::path::PathBuf;

    const ROOT: &str = "/ws";

    fn outcome(tool: &str, args: serde_json::Value) -> ToolDispatchOutcome {
        ToolDispatchOutcome {
            tool_call_id: "c1".into(),
            tool_name: tool.into(),
            arguments: args,
            result: Ok(ToolOutput::success("ok", 1)),
            paths_touched: vec![],
            was_mutation: false,
            soft_error: None,
            rejected: false,
            message_content: "ok".into(),
            is_error: false,
        }
    }

    fn ev(path: &str, op: FileOp, detail: &str) -> FileEvent {
        FileEvent { path: path.into(), op, detail: detail.into() }
    }

    fn derive(o: &ToolDispatchOutcome) -> Vec<FileEvent> {
        file_events_from_outcome(o, Some(Path::new(ROOT)))
    }

    #[test]
    fn read_file_event_records_range() {
        let full = outcome("read_file", json!({"path": "src/a.rs"}));
        assert_eq!(derive(&full), vec![ev("src/a.rs", FileOp::Read, "全文")]);

        let ranged = outcome("read_file", json!({"path": "/ws/src/a.rs", "offset": 11, "limit": 20}));
        assert_eq!(derive(&ranged), vec![ev("src/a.rs", FileOp::Read, "第 11–30 行")]);

        let open_ended = outcome("read_file", json!({"path": "./src/a.rs", "offset": 50}));
        assert_eq!(derive(&open_ended), vec![ev("src/a.rs", FileOp::Read, "第 50 行起")]);

        let head = outcome("read_file", json!({"path": "src/a.rs", "limit": 10}));
        assert_eq!(derive(&head), vec![ev("src/a.rs", FileOp::Read, "第 1–10 行")]);
    }

    #[test]
    fn write_file_event_counts_lines() {
        let o = outcome("write_file", json!({"path": "notes.md", "content": "a\nb\nc\n"}));
        assert_eq!(derive(&o), vec![ev("notes.md", FileOp::Write, "写入 3 行")]);
        let empty = outcome("write_file", json!({"path": "empty.txt", "content": ""}));
        assert_eq!(derive(&empty), vec![ev("empty.txt", FileOp::Write, "写入 0 行")]);
    }

    #[test]
    fn edit_single_and_batch_forms_record_each_file() {
        let mut single = outcome(
            "edit",
            json!({"path": "src/a.rs", "edits": [
                {"old_text": "fn a() {\n  1\n}", "new_text": "fn a() {\n  2\n}"},
                {"old_text": "", "new_text": "use x;"}
            ]}),
        );
        single.was_mutation = true;
        single.paths_touched = vec![PathBuf::from("/ws/src/a.rs")];
        assert_eq!(
            derive(&single),
            vec![ev("src/a.rs", FileOp::Edit, "「fn a() {」→「fn a() {」；插入「use x;」")]
        );

        let mut batch = outcome(
            "edit",
            json!({"files": [
                {"path": "src/a.rs", "edits": [{"old_text": "x", "new_text": "y"}]},
                {"path": "src/b.rs", "edits": [{"old_text": "p", "new_text": "q"}]}
            ]}),
        );
        batch.was_mutation = true;
        batch.paths_touched = vec![PathBuf::from("/ws/src/a.rs"), PathBuf::from("/ws/src/b.rs")];
        assert_eq!(
            derive(&batch),
            vec![
                ev("src/a.rs", FileOp::Edit, "「x」→「y」"),
                ev("src/b.rs", FileOp::Edit, "「p」→「q」"),
            ]
        );

        // No paths_touched (e.g. no workspace gate) → fall back to argument paths.
        let mut bare = outcome(
            "edit",
            json!({"files": [
                {"path": "a.rs", "edits": [{"old_text": "x", "new_text": "y"}]},
                {"path": "b.rs", "edits": [{"old_text": "p", "new_text": "q"}]}
            ]}),
        );
        bare.was_mutation = true;
        assert_eq!(derive(&bare).len(), 2);
    }

    #[test]
    fn edit_detail_is_truncated_to_600_chars() {
        let long = "长".repeat(200);
        let edits: Vec<serde_json::Value> = (0..20)
            .map(|_| json!({"old_text": long, "new_text": long}))
            .collect();
        let mut o = outcome("edit", json!({"path": "src/a.rs", "edits": edits}));
        o.was_mutation = true;
        o.paths_touched = vec![PathBuf::from("/ws/src/a.rs")];
        let events = derive(&o);
        assert_eq!(events.len(), 1);
        let detail = &events[0].detail;
        assert!(detail.chars().count() <= 600, "got {} chars", detail.chars().count());
        // Each side's first line is capped at 80 chars plus an ellipsis.
        let first = detail.split('；').next().unwrap();
        assert_eq!(first, format!("「{}…」→「{}…」", "长".repeat(80), "长".repeat(80)));
    }

    #[test]
    fn plan_tools_record_plan_event() {
        let mut write = outcome("plan_write", json!({"title": "T", "steps": ["a"]}));
        write.result = Ok(ToolOutput::success(
            "Plan created at /ws/.uclaw/plans/2026-11-09_120000-t.md",
            1,
        ));
        assert_eq!(
            derive(&write),
            vec![ev(".uclaw/plans/2026-11-09_120000-t.md", FileOp::Plan, "创建计划")]
        );

        let update = outcome(
            "plan_update",
            json!({"filename": "2026-11-09_120000-t.md", "step_index": 0, "done": true}),
        );
        assert_eq!(
            derive(&update),
            vec![ev(".uclaw/plans/2026-11-09_120000-t.md", FileOp::Plan, "更新计划")]
        );
    }

    #[test]
    fn bash_rm_and_mv_are_recognised_other_commands_ignored() {
        let rm = outcome("bash", json!({"command": "rm -f build/out.o 'tmp/a b.txt'"}));
        assert_eq!(
            derive(&rm),
            vec![
                ev("build/out.o", FileOp::Delete, "rm 删除"),
                ev("tmp/a b.txt", FileOp::Delete, "rm 删除"),
            ]
        );

        let mv = outcome("bash", json!({"command": "mv src/old.rs src/new.rs"}));
        assert_eq!(
            derive(&mv),
            vec![
                ev("src/old.rs", FileOp::Delete, "移动到 src/new.rs"),
                ev("src/new.rs", FileOp::Write, "由 src/old.rs 移动而来"),
            ]
        );

        let dashdash = outcome("bash", json!({"command": "rm -f -- -weird"}));
        assert_eq!(derive(&dashdash), vec![ev("-weird", FileOp::Delete, "rm 删除")]);

        let in_dir = outcome("bash", json!({"command": "rm x.txt", "working_dir": "sub"}));
        assert_eq!(derive(&in_dir), vec![ev("sub/x.txt", FileOp::Delete, "rm 删除")]);

        for cmd in [
            "ls -la",
            "cargo test",
            "rm -rf build && cargo build",
            "rm *.o",
            "mv a b c",
            "rm a\necho done",
            "rm a.txt\r\necho done",
            "mv -t dir a.txt",
            "mv --target-directory=dir a.txt",
            "mv -S .bak a b",
            "echo hi > out.txt",
            "rm $(cat list)",
        ] {
            let o = outcome("bash", json!({"command": cmd}));
            assert!(derive(&o).is_empty(), "`{cmd}` must not produce events");
        }
    }

    #[test]
    fn other_mutating_tools_record_edit_per_path() {
        let mut o = outcome("apply_patch", json!({}));
        o.was_mutation = true;
        o.paths_touched = vec![PathBuf::from("/ws/a.rs"), PathBuf::from("/other/b.rs")];
        assert_eq!(
            derive(&o),
            vec![
                ev("a.rs", FileOp::Edit, "apply_patch"),
                ev("/other/b.rs", FileOp::Edit, "apply_patch"),
            ]
        );
        let mut ro = outcome("grep", json!({}));
        ro.paths_touched = vec![PathBuf::from("/ws/a.rs")];
        assert!(derive(&ro).is_empty());
    }

    #[test]
    fn failed_or_rejected_calls_record_nothing() {
        let mut soft = outcome("write_file", json!({"path": "a.txt", "content": "x"}));
        soft.is_error = true;
        soft.soft_error = Some("boom".into());
        assert!(derive(&soft).is_empty());

        let mut hard = outcome("read_file", json!({"path": "a.txt"}));
        hard.result = Err(ToolError::Execution("nope".into()));
        hard.is_error = true;
        assert!(derive(&hard).is_empty());

        let mut rejected = outcome("bash", json!({"command": "rm a.txt"}));
        rejected.rejected = true;
        assert!(derive(&rejected).is_empty());
    }
}
