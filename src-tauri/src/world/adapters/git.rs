//! Git projection adapter — turn a local git repo into `WorldEntity`s.
//!
//! M4-T4 commit 2 will spawn `git` via a subprocess pool. This pilot
//! ships the **parsers** for the most useful subset:
//!
//! - `parse_branch_listing` — `git branch --list` output → `GitBranch`s
//! - `parse_log_one_line` — `git log --oneline` output → `GitCommit`s
//! - `parse_status_porcelain` — `git status --porcelain` → `GitWorkTreeChange`s
//!
//! Pure parsers, no I/O. The wrapper `GitAdapter` is a thin shell
//! that runs commands + funnels the parsed output into
//! `ProjectionStore::upsert`.
//!
//! Entity ids:
//! - Commit:  `git:commit:{repo}:{sha}`
//! - Branch:  `git:branch:{repo}:{name}`
//! - Working-tree change: `git:wtchange:{repo}:{path}`
//!
//! `repo` is a stable identifier the caller picks (e.g. the absolute
//! path to the repo root); we don't dictate format.

use serde_json::json;

use crate::world::entity::{EntityRef, WorldEntity, WorldEntityKind, WorldEntityState};

/// One commit observation. `sha` is the full SHA when known, short
/// SHA when the output only had the short form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    pub sha: String,
    pub subject: String,
}

/// One branch observation. `head` is `true` when the branch is the
/// current HEAD (parsed from the leading `*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitBranch {
    pub name: String,
    pub head: bool,
}

/// Working-tree change row from `git status --porcelain` v1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorkTreeChange {
    pub path: String,
    /// Two-character porcelain status code (e.g. `" M"`, `"A "`, `"??"`).
    pub status: String,
}

// ── parsers ─────────────────────────────────────────────────────────

/// Parse `git log --oneline`-style output.
///
/// Format expected (one per line):
/// ```text
/// abcdef0 fix(agent): something
/// 1234567 feat: another
/// ```
/// Hash is the leading token; subject is the rest. Empty lines are
/// skipped. Lines without a space are skipped (defensive).
pub fn parse_log_one_line(input: &str) -> Vec<GitCommit> {
    input
        .lines()
        .filter_map(|l| {
            let line = l.trim_end();
            if line.is_empty() {
                return None;
            }
            let mut parts = line.splitn(2, ' ');
            let sha = parts.next()?.to_string();
            let subject = parts.next()?.to_string();
            if sha.is_empty() {
                return None;
            }
            Some(GitCommit { sha, subject })
        })
        .collect()
}

/// Parse `git branch --list` output. Format:
/// ```text
///   main
/// * feature/x
///   release/1.0
/// ```
/// Leading `* ` marks the current HEAD.
pub fn parse_branch_listing(input: &str) -> Vec<GitBranch> {
    input
        .lines()
        .filter_map(|l| {
            let line = l;
            let (head, name_part) = if let Some(stripped) = line.strip_prefix("* ") {
                (true, stripped)
            } else {
                (false, line.trim_start_matches(' '))
            };
            let name = name_part.trim().to_string();
            if name.is_empty() {
                return None;
            }
            // Ignore detached-HEAD lines like "(HEAD detached at abc)" —
            // they're informative, not actual branch refs.
            if name.starts_with('(') {
                return None;
            }
            Some(GitBranch { name, head })
        })
        .collect()
}

/// Parse `git status --porcelain` v1 output. Format:
/// ```text
///  M agent/dispatcher.rs
/// A  new_file.txt
/// ?? untracked.log
/// ```
/// First two chars are the status code; rest of the line (after one
/// space) is the path.
pub fn parse_status_porcelain(input: &str) -> Vec<GitWorkTreeChange> {
    input
        .lines()
        .filter_map(|l| {
            if l.len() < 4 {
                return None;
            }
            let status = l[..2].to_string();
            let path = l[3..].to_string();
            if path.is_empty() {
                return None;
            }
            Some(GitWorkTreeChange { path, status })
        })
        .collect()
}

// ── projection emit ────────────────────────────────────────────────

/// Turn a parsed commit into a `WorldEntity` ready for upsert.
pub fn commit_to_entity(
    repo: &str,
    commit: &GitCommit,
    observed_at: &str,
) -> WorldEntity {
    let id = format!("git:commit:{repo}:{}", commit.sha);
    let state = WorldEntityState::fresh(observed_at)
        .with_property("sha", json!(commit.sha))
        .with_property("subject", json!(commit.subject))
        .with_property("repo", json!(repo));
    WorldEntity::new(EntityRef::new(id), WorldEntityKind::GitObject, state)
}

pub fn branch_to_entity(
    repo: &str,
    branch: &GitBranch,
    observed_at: &str,
) -> WorldEntity {
    let id = format!("git:branch:{repo}:{}", branch.name);
    let state = WorldEntityState::fresh(observed_at)
        .with_property("name", json!(branch.name))
        .with_property("head", json!(branch.head))
        .with_property("repo", json!(repo));
    WorldEntity::new(EntityRef::new(id), WorldEntityKind::GitObject, state)
}

pub fn wtchange_to_entity(
    repo: &str,
    change: &GitWorkTreeChange,
    observed_at: &str,
) -> WorldEntity {
    let id = format!("git:wtchange:{repo}:{}", change.path);
    let state = WorldEntityState::fresh(observed_at)
        .with_property("path", json!(change.path))
        .with_property("status", json!(change.status))
        .with_property("repo", json!(repo));
    WorldEntity::new(EntityRef::new(id), WorldEntityKind::GitObject, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_log_one_line ─────────────────────────────────────────

    #[test]
    fn parse_log_two_commits() {
        let input = "abcdef0 fix(agent): one\n1234567 feat: two\n";
        let out = parse_log_one_line(input);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].sha, "abcdef0");
        assert_eq!(out[0].subject, "fix(agent): one");
        assert_eq!(out[1].sha, "1234567");
        assert_eq!(out[1].subject, "feat: two");
    }

    #[test]
    fn parse_log_skips_empty_and_malformed() {
        let input = "\nabcdef0 ok\n\nnobody\n1234567 fine\n";
        // "nobody" has no space → malformed → skipped.
        let out = parse_log_one_line(input);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].sha, "abcdef0");
        assert_eq!(out[1].sha, "1234567");
    }

    #[test]
    fn parse_log_empty_input_returns_empty() {
        assert!(parse_log_one_line("").is_empty());
        assert!(parse_log_one_line("\n\n").is_empty());
    }

    // ── parse_branch_listing ──────────────────────────────────────

    #[test]
    fn parse_branches_marks_head_with_star() {
        let input = "  main\n* feature/x\n  release/1.0\n";
        let out = parse_branch_listing(input);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].name, "main");
        assert!(!out[0].head);
        assert_eq!(out[1].name, "feature/x");
        assert!(out[1].head);
        assert_eq!(out[2].name, "release/1.0");
        assert!(!out[2].head);
    }

    #[test]
    fn parse_branches_skips_detached_head_line() {
        let input = "  main\n* (HEAD detached at abc123)\n  develop\n";
        let out = parse_branch_listing(input);
        // Detached HEAD informational line is skipped.
        assert_eq!(out.len(), 2);
        assert!(!out.iter().any(|b| b.name.starts_with("(HEAD")));
    }

    #[test]
    fn parse_branches_skips_blank_lines() {
        let input = "\n\n  main\n\n* dev\n";
        let out = parse_branch_listing(input);
        assert_eq!(out.len(), 2);
    }

    // ── parse_status_porcelain ────────────────────────────────────

    #[test]
    fn parse_porcelain_three_changes() {
        let input = " M agent/dispatcher.rs\nA  new_file.txt\n?? untracked.log\n";
        let out = parse_status_porcelain(input);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].status, " M");
        assert_eq!(out[0].path, "agent/dispatcher.rs");
        assert_eq!(out[1].status, "A ");
        assert_eq!(out[1].path, "new_file.txt");
        assert_eq!(out[2].status, "??");
        assert_eq!(out[2].path, "untracked.log");
    }

    #[test]
    fn parse_porcelain_too_short_line_skipped() {
        // Less than 4 chars → no path possible.
        let input = "M\n A\n";
        let out = parse_status_porcelain(input);
        assert!(out.is_empty());
    }

    // ── entity emit ───────────────────────────────────────────────

    #[test]
    fn commit_entity_id_includes_repo_and_sha() {
        let c = GitCommit {
            sha: "abc".into(),
            subject: "s".into(),
        };
        let e = commit_to_entity("/repo", &c, "t0");
        assert_eq!(e.r#ref.id, "git:commit:/repo:abc");
        assert_eq!(e.state.properties.get("sha"), Some(&json!("abc")));
        assert_eq!(e.state.properties.get("subject"), Some(&json!("s")));
    }

    #[test]
    fn branch_entity_records_head_flag() {
        let b = GitBranch {
            name: "main".into(),
            head: true,
        };
        let e = branch_to_entity("/repo", &b, "t0");
        assert_eq!(e.r#ref.id, "git:branch:/repo:main");
        assert_eq!(e.state.properties.get("head"), Some(&json!(true)));
    }

    #[test]
    fn wtchange_entity_records_status() {
        let c = GitWorkTreeChange {
            path: "x.rs".into(),
            status: " M".into(),
        };
        let e = wtchange_to_entity("/repo", &c, "t0");
        assert_eq!(e.r#ref.id, "git:wtchange:/repo:x.rs");
        assert_eq!(e.state.properties.get("status"), Some(&json!(" M")));
    }

    // ── kind is GitObject for all three ──────────────────────────

    #[test]
    fn all_emitters_use_git_object_kind() {
        let c = commit_to_entity(
            "/r",
            &GitCommit {
                sha: "a".into(),
                subject: "s".into(),
            },
            "t",
        );
        let b = branch_to_entity(
            "/r",
            &GitBranch {
                name: "n".into(),
                head: false,
            },
            "t",
        );
        let w = wtchange_to_entity(
            "/r",
            &GitWorkTreeChange {
                path: "p".into(),
                status: "AA".into(),
            },
            "t",
        );
        for e in [c, b, w] {
            assert_eq!(e.kind, WorldEntityKind::GitObject);
        }
    }
}
