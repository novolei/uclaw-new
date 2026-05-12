/**
 * Typed Tauri IPC bindings for the backend `commands::git` module.
 *
 * The wrappers in this file are the **only** sanctioned entry-points
 * for git operations from the front-end (per Staff UX spec
 * `docs/design-docs/git-workbench-ux.md` §9).  Every git-related
 * `invoke` call must go through here so:
 *
 * 1. The argument shape (`{ cwd, ...rest }`) stays consistent across
 *    the entire UI surface.
 * 2. Callers see typed return values without each component
 *    re-declaring the response shape.
 * 3. Future contract changes (e.g. adding `remote` to
 *    `pushBranchSetUpstream`) need to be patched in exactly one file.
 *
 * All commands are async and return promises that reject with the
 * backend's `Display`-rendered error string when the underlying
 * `GitError` is non-recoverable.  Idempotent outcomes (e.g. `commit`
 * on a clean tree) are surfaced as a value, not a rejection.
 */

import { invoke } from "@tauri-apps/api/core";

// ── Result DTOs (mirror commands/git.rs) ─────────────────────────────────────

/** Outcome of a `commit` invocation. */
export interface CommitOutcome {
  /** `"created"` when the commit landed; `"skipped"` when the working
   *  tree had no changes (idempotent). */
  status: "created" | "skipped";
  /** Trimmed commit message that was used, or a human-readable reason
   *  for `"skipped"`. */
  message: string;
}

/** Outcome of a `gh pr create` invocation. */
export interface CreatePrResponse {
  /** URL of the pull request (created or already existing). */
  url: string;
  /** `true` when the PR was already open before the call. */
  wasExisting: boolean;
  /** Base branch the PR was opened against (resolved when caller
   *  passed `base = null`). */
  base: string;
}

// ── Repository discovery ────────────────────────────────────────────────────

/** Cheap probe: does `cwd` sit inside a git working tree?
 *
 *  Never rejects — backend collapses every error (no `.git`, missing
 *  binary, …) into `false`.  UI uses this to flip BranchPicker /
 *  GitActionsPicker into a disabled "无 Git 仓库" state. */
export async function gitIsRepo(cwd: string): Promise<boolean> {
  return invoke<boolean>("git_is_repo", { cwd });
}

/** Run `git init` in `cwd`; idempotent.  Caller should re-probe
 *  `gitIsRepo` (or just optimistically flip local state) afterwards. */
export async function gitInitRepo(cwd: string): Promise<void> {
  await invoke<void>("git_init_repo", { cwd });
}

// ── Status / Diff ────────────────────────────────────────────────────────────

/** `git status --short --branch` snapshot, or `null` if the tree is clean. */
export async function gitStatus(cwd: string): Promise<string | null> {
  return invoke<string | null>("git_status", { cwd });
}

/** Staged + unstaged diff text, or `null` if the tree is clean.
 *
 *  Default is `git diff --stat` (one line per file: `+N -N`) — a few
 *  KB even for huge refactors, safe to keep in chat history / event
 *  log.  Pass `{ full: true }` to opt into the full unified patch
 *  (renderer is expected to virtualise / page large outputs). */
export async function gitDiff(
  cwd: string,
  opts?: { full?: boolean },
): Promise<string | null> {
  return invoke<string | null>("git_diff", {
    cwd,
    full: opts?.full ?? false,
  });
}

// ── Branch ──────────────────────────────────────────────────────────────────

/** Verbose branch listing (`git branch --list --verbose`). */
export async function gitBranches(cwd: string): Promise<string> {
  return invoke<string>("git_branches", { cwd });
}

/** Currently checked-out branch.  Rejects with `"no branch is currently
 *  checked out (detached HEAD)"` when in detached state. */
export async function gitCurrentBranch(cwd: string): Promise<string> {
  return invoke<string>("git_current_branch", { cwd });
}

/** Repository default branch via the `origin/HEAD → main → master →
 *  init.defaultBranch → current` chain. */
export async function gitDefaultBranch(cwd: string): Promise<string> {
  return invoke<string>("git_default_branch", { cwd });
}

/** `git checkout <name>` — switch to an existing branch.  Rejects with
 *  the underlying `git` stderr (dirty tree blocking checkout, branch
 *  missing, …) so the UI can surface it verbatim in a toast. */
export async function gitCheckoutBranch(cwd: string, name: string): Promise<void> {
  await invoke<void>("git_checkout_branch", { cwd, name });
}

/** `git checkout -b <name>` — create a branch at HEAD and check it out.
 *  Caller is responsible for trimming whitespace and warning about
 *  invalid characters before invoking. */
export async function gitCreateBranch(cwd: string, name: string): Promise<void> {
  await invoke<void>("git_create_branch", { cwd, name });
}

/** Parsed entry produced by [`parseBranchList`]. */
export interface BranchListItem {
  /** Branch short name (no `refs/heads/` prefix). */
  name: string;
  /** `true` for the branch currently checked out (the `*` line). */
  isCurrent: boolean;
}

/** Parse the `git branch --list --verbose` text returned by
 *  [`gitBranches`] into a structured list.
 *
 *  Defensive: skips empty lines and the `(HEAD detached at …)` entry
 *  (caller can detect detached state via [`gitCurrentBranch`]).
 *  Worktree-checked-out branches (lines starting with `+`) are
 *  surfaced as `isCurrent: false` — only `*` counts as "this window
 *  is sitting on it". */
export function parseBranchList(raw: string): BranchListItem[] {
  const out: BranchListItem[] = [];
  for (const line of raw.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const isCurrent = trimmed.startsWith("*");
    // Strip leading `*`, `+`, or whitespace marker and split on
    // whitespace.  First token is the branch name.
    const rest = trimmed.replace(/^[*+]\s*/, "");
    const name = rest.split(/\s+/, 1)[0] ?? "";
    if (!name || name.startsWith("(")) continue;
    out.push({ name, isCurrent });
  }
  return out;
}

// ── Commit ──────────────────────────────────────────────────────────────────

/** Stage everything and commit.  `status="skipped"` is the idempotent
 *  no-op outcome (clean tree) — render as a benign info toast, NOT
 *  an error. */
export async function gitCommit(
  cwd: string,
  message: string,
): Promise<CommitOutcome> {
  return invoke<CommitOutcome>("git_commit", { cwd, message });
}

/** Composite "commit + push + open PR".  `gh` is required; rejects
 *  with `MissingBinary("gh")` when not installed.
 *
 *  Argument keys are sent as `branch_hint` (snake_case) to match the
 *  Rust parameter name verbatim; the helper accepts the public-facing
 *  camelCase name on the TypeScript side for ergonomic parity with
 *  the rest of `api.ts`. */
export async function gitCommitPushPr(args: {
  cwd: string;
  title: string;
  body: string;
  branchHint?: string;
}): Promise<string> {
  return invoke<string>("git_commit_push_pr", {
    cwd: args.cwd,
    title: args.title,
    body: args.body,
    branch_hint: args.branchHint ?? null,
  });
}

// ── GitHub: PR / Issue ──────────────────────────────────────────────────────

/** Probe whether the `gh` binary is reachable on `PATH`. */
export async function ghAvailable(): Promise<boolean> {
  return invoke<boolean>("gh_available");
}

/** Open a pull request via `gh pr create`.  When `base` is omitted the
 *  repo's default branch is detected automatically. */
export async function ghCreatePr(args: {
  cwd: string;
  title: string;
  body: string;
  base?: string;
}): Promise<CreatePrResponse> {
  return invoke<CreatePrResponse>("gh_create_pr", {
    cwd: args.cwd,
    title: args.title,
    body: args.body,
    base: args.base ?? null,
  });
}

/** Open a GitHub issue via `gh issue create`.  Returns the issue URL. */
export async function ghCreateIssue(args: {
  cwd: string;
  title: string;
  body: string;
}): Promise<string> {
  return invoke<string>("gh_create_issue", args);
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/** Returned by `gitCreateWorktreeProject` after a successful worktree + project creation. */
export interface CreatedWorktreeProject {
  /** Human-readable project name (derived from target directory basename). */
  name: string
  /** Absolute path to the newly-created worktree directory. */
  path: string
  /** The branch checked out in the new worktree. */
  branch: string
}

/**
 * Create a git worktree at `target` on `branch` (creating the branch if it
 * doesn't exist) and register it as a new uClaw project.
 *
 * NOTE: The backend Tauri command for this is planned in W6 Phase 3 (worktree
 * support).  Until then the call stubs with `invoke('git_create_worktree_project', …)`.
 */
export async function gitCreateWorktreeProject(args: {
  cwd: string
  target: string
  branch: string
  project_name?: string
}): Promise<CreatedWorktreeProject> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<CreatedWorktreeProject>('git_create_worktree_project', { args })
}

/**
 * Count the number of changed files reported in `git status --short --branch`.
 *
 * The first line is the `## branch ...` header — drop it; every remaining
 * non-empty line is one changed/untracked file. Returns `0` when `raw` is
 * null (clean tree) or has no file lines.
 */
export function uncommittedFromStatus(raw: string | null): number {
  if (!raw) return 0;
  return raw.split("\n").slice(1).filter((line) => line.trim().length > 0).length;
}
