# GitHub Actions Review Debug

## Intent

Add focused GitHub Actions infrastructure that helps reviewers understand PR
risk quickly and gives maintainers enough CI evidence to debug failures without
rerunning everything locally.

## Scope

- Add core CI for whitespace, shell syntax, GitNexus agent-doc pinning, focused
  UI smoke tests/build, and Rust core crate smoke.
- Add a review digest workflow that writes a job summary, emits PR annotations
  for high-attention paths, and uploads review artifacts.
- Add security workflow coverage for dependency review and JavaScript/TypeScript
  CodeQL scanning.
- Ignore local `review-artifacts/` produced by the digest helper.
- Keep full UI suite, Tauri bundle checks, and browser E2E out of this slice;
  those should be opt-in or path-scoped once the basic review/debug layer is
  stable and CI has the required embedded runtime resources.

## ADR 18 Answers

1. Intent: improve review/debug feedback for PRs.
2. Autonomy: CI runs automatically on PRs and main pushes; no runtime autonomy
   changes.
3. Truth source: GitHub Actions workflows plus local scripts under
   `scripts/ci/`.
4. TaskEvent: none.
5. Context: review digest summarizes changed areas and high-attention files.
6. Capability: reviewers get annotations, summaries, artifacts, and security
   checks.
7. Hooks: GitHub Actions only; existing git hooks are not changed.
8. Projection: CI surfaces PR risk and failure evidence in GitHub UI.
9. Harness: local YAML/script syntax checks, digest script execution, and
   GitNexus staged detect.
10. Rollback: revert this docs/CI commit.
11. Does not own: no app runtime behavior, database schema, or Tauri E2E.

## Verification

- `git diff --check -- .github scripts docs/superpowers/plans/2026-05-25-github-actions-review-debug.md`
- `scripts/ci/shell-syntax-check.sh`
- `BASE_SHA=HEAD~1 GITHUB_SHA=HEAD GITHUB_STEP_SUMMARY=/tmp/uclaw-review-digest.md scripts/ci/pr-review-digest.sh`
- `scripts/verify/gitnexus-agent-docs-pinned.sh`
- `cargo check --locked -p uclaw-message-types -p uclaw-tool-types -p uclaw-runtime-contracts -p uclaw-protocol-types -p uclaw-provider-core`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/github-actions-review-debug`
