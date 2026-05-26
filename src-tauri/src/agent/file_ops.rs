//! 会话文件操作持久记忆 — Pi FileOps 的 Rust 实现。
//!
//! `SessionFileOps` 跨压缩周期累积，是 StructuredFold 的第 10 个字段。
//! Pi 对应：`SessionFileOps` in `compaction/utils.ts`。

use std::collections::HashSet;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// 会话文件操作记录（跨压缩周期累积）。
/// Pi 对应：`SessionFileOps` in `compaction/utils.ts`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionFileOps {
    pub read: HashSet<PathBuf>,
    pub written: HashSet<PathBuf>,
    pub edited: HashSet<PathBuf>,
}

impl SessionFileOps {
    /// 记录一次工具调用，提取路径参数并分类到对应集合。
    pub fn track_tool_call(&mut self, tool_name: &str, args: &serde_json::Value) {
        let path = match args
            .get("path")
            .or_else(|| args.get("file_path"))
            .and_then(|p| p.as_str())
        {
            Some(p) if !p.is_empty() => PathBuf::from(p),
            _ => return,
        };

        match tool_name {
            "read_file" | "read" => {
                self.read.insert(path);
            }
            "write_file" | "write" => {
                self.written.insert(path);
            }
            "edit" | "apply_patch" | "str_replace" | "str_replace_editor" => {
                self.edited.insert(path);
            }
            _ => {}
        }
    }

    /// 将另一个 `SessionFileOps` 的内容合并到 self，实现跨压缩周期累积。
    pub fn merge(&mut self, other: &SessionFileOps) {
        self.read.extend(other.read.iter().cloned());
        self.written.extend(other.written.iter().cloned());
        self.edited.extend(other.edited.iter().cloned());
    }

    /// 生成摘要尾注。空操作时返回空字符串（不追加任何章节）。
    pub fn format_for_summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        if !self.read.is_empty() {
            let mut files: Vec<String> =
                self.read.iter().map(|p| p.display().to_string()).collect();
            files.sort();
            parts.push(format!("**已读取文件**: {}", files.join(", ")));
        }

        let modified: HashSet<&PathBuf> = self.written.union(&self.edited).collect();
        if !modified.is_empty() {
            let mut files: Vec<String> =
                modified.iter().map(|p| p.display().to_string()).collect();
            files.sort();
            parts.push(format!("**已修改文件**: {}", files.join(", ")));
        }

        if parts.is_empty() {
            String::new()
        } else {
            format!(
                "\n\n---\n\n## 文件操作记录（跨压缩持久）\n\n{}",
                parts.join("\n")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_read_file_tool() {
        let mut ops = SessionFileOps::default();
        let args = serde_json::json!({ "path": "/src/auth.rs" });
        ops.track_tool_call("read_file", &args);
        assert!(ops.read.contains(&PathBuf::from("/src/auth.rs")));
        assert!(ops.written.is_empty());
    }

    #[test]
    fn track_edit_tool() {
        let mut ops = SessionFileOps::default();
        let args = serde_json::json!({ "path": "/src/db.rs" });
        ops.track_tool_call("edit", &args);
        assert!(ops.edited.contains(&PathBuf::from("/src/db.rs")));
    }

    #[test]
    fn merge_accumulates_across_compressions() {
        let mut a = SessionFileOps::default();
        a.track_tool_call("read_file", &serde_json::json!({ "path": "/a.rs" }));

        let mut b = SessionFileOps::default();
        b.track_tool_call("write_file", &serde_json::json!({ "path": "/b.rs" }));

        a.merge(&b);
        assert!(a.read.contains(&PathBuf::from("/a.rs")));
        assert!(a.written.contains(&PathBuf::from("/b.rs")));
    }

    #[test]
    fn format_for_summary_empty_when_no_ops() {
        let ops = SessionFileOps::default();
        assert!(ops.format_for_summary().is_empty());
    }

    #[test]
    fn format_for_summary_includes_modified_files() {
        let mut ops = SessionFileOps::default();
        ops.track_tool_call("write_file", &serde_json::json!({ "path": "/x.rs" }));
        let summary = ops.format_for_summary();
        assert!(summary.contains("已修改文件"), "got: {summary}");
        assert!(summary.contains("x.rs"), "got: {summary}");
    }

    #[test]
    fn merge_is_cumulative_across_multiple_compressions() {
        let mut session_ops = SessionFileOps::default();

        let mut round1 = SessionFileOps::default();
        round1.track_tool_call("read_file", &serde_json::json!({ "path": "/src/auth.rs" }));
        round1.track_tool_call("edit", &serde_json::json!({ "path": "/src/db.rs" }));
        session_ops.merge(&round1);

        let mut round2 = SessionFileOps::default();
        round2.track_tool_call("write_file", &serde_json::json!({ "path": "/src/new.rs" }));
        session_ops.merge(&round2);

        assert_eq!(session_ops.read.len(), 1);
        assert!(session_ops.read.contains(&PathBuf::from("/src/auth.rs")));
        assert_eq!(session_ops.edited.len(), 1);
        assert_eq!(session_ops.written.len(), 1);
    }
}
