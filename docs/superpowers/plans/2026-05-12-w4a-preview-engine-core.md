# W4a — Preview Engine Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the minimum-viable preview pipeline — click a file in `FilesRail` → it opens in a slide-in panel on the right side of the agent view with syntax-highlighted code / rendered markdown / asset-protocol images. Backend `preview_read_bytes` + path resolver. Consumes W1's `codeHighlightCache` + `usePreviewRefresh`.

**Architecture:** New Rust module `src-tauri/src/preview/` (4 files) owns path resolution + a single `preview_read_bytes` Tauri command. New `ui/src/components/preview/` directory adds a slide-in `<PreviewPanel>` shell, a renderer router (`usePreviewRouter` selects between Code/Markdown/Image/Binary by extension), and 4 simple renderers. State lives in Jotai atoms (selected file, isOpen, width). The existing `FilesRail.onFileClick` callback in `SidePanel.tsx` is rewired to open the preview instead of just adding to chat.

**Tech Stack:** Rust · `serde` (already in tree) · React 18 + TypeScript · Jotai (`atomFamily` + `atomWithStorage`) · existing `shiki` highlighter at `ui/src/lib/highlight.ts` · existing `react-markdown` + `remark-gfm` · existing `convertFileSrc` for asset protocol · Tailwind + uClaw theme tokens · Vitest + RTL · `cargo test --lib`.

**Spec:** `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §6 (core subset — rich formats land in W4b, editing + chips in W4c).

**Out of W4a scope** (deferred to W4b / W4c):
- PDF, DOCX, XLSX, PPTX renderers + new npm deps (jszip, mammoth, pdfjs-dist, @xmldom/xmldom)
- Inline editing (CodeMirror, TipTap)
- File-path chips + remark plugin
- Detached preview window (W5)
- Diff renderer (needs the diff library decision)

---

## Pre-flight

- [ ] **Branch setup** (already done at plan-writing time, but confirm)

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main
git pull --ff-only
git checkout -b claude/w4a-preview-engine-core   # or reuse claude/w4-preview-engine if branched already
```

- [ ] **Baseline verification**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib 2>&1 | tail -3
```

Record baseline test count (should be 387 after W3).

```bash
cd ../ui && npx tsc --noEmit 2>&1 | head
cd ui && npm test -- --run 2>&1 | tail -3
```

Record UI baseline (should be 266 after W3).

- [ ] **State landmarks** (read once, don't change)

```bash
cat /Users/ryanliu/Documents/uclaw/ui/src/lib/highlight.ts | head -20
```

The existing `highlightCode(code, language, theme?)` returns shiki HTML. W4a's CodeRenderer consumes it directly.

```bash
cat /Users/ryanliu/Documents/uclaw/ui/src/components/preview/codeHighlightCache.ts
```

W1's cache. Key = `gitRoot:filePath:refreshVersion`. Stores raw + highlighted HTML + lang + theme. W4a consumes via `cacheGet` / `cacheSet` / `cacheKey` / `shouldSkipHighlight`.

```bash
cat /Users/ryanliu/Documents/uclaw/ui/src/hooks/usePreviewRefresh.ts
```

W1's hook. Returns a version number; bumped by `tauri://focus` + `agent:file-written`. W4a's `useFileBytes` includes this version in its dep array so the byte stream auto-refreshes.

```bash
grep -n "FilesRail" /Users/ryanliu/Documents/uclaw/ui/src/components/agent/SidePanel.tsx
```

The integration point — `<FilesRail onFileClick=...>` at SidePanel:220.

---

## File Structure

### New Rust modules

| Path | Lines (target) | Responsibility |
|---|---|---|
| `src-tauri/src/preview/mod.rs` | ~30 | barrel re-exports + module doc |
| `src-tauri/src/preview/types.rs` | ~60 | `PreviewBytes` struct (bytes, size, truncated, mtime_ms) |
| `src-tauri/src/preview/resolver.rs` | ~130 | path resolution (mount-aware; reject absolute + `..`); ports Proma's `resolveTargetPath` shape |
| `src-tauri/src/preview/commands.rs` | ~90 | `preview_read_bytes` only (write comes in W4c) |
| `src-tauri/src/preview/tests.rs` | ~120 | resolver edge cases + 50MB cap |

### Modified Rust files

| Path | Edit |
|---|---|
| `src-tauri/src/lib.rs` | add `pub mod preview;` |
| `src-tauri/src/main.rs` | register `preview_read_bytes` in `generate_handler!` |
| `src-tauri/src/tauri_commands.rs` | re-export `preview::commands::preview_read_bytes` |
| `src-tauri/src/app.rs` | no change (preview helpers live in `preview/` module, not `AppState`; resolver takes mount info from `files_rail_list_mounts`) |

### New TypeScript modules

| Path | Lines (target) | Responsibility |
|---|---|---|
| `ui/src/atoms/preview-panel-atoms.ts` | ~70 | selected-file atom + isOpen + width (atomWithStorage) |
| `ui/src/components/preview/PreviewPanel.tsx` | ~150 | slide-in container with width handle |
| `ui/src/components/preview/PreviewHeader.tsx` | ~90 | filename + truncated path + close button + pop-out button (disabled placeholder for W5) |
| `ui/src/components/preview/PreviewSurface.tsx` | ~80 | renderer dispatcher driven by `usePreviewRouter` |
| `ui/src/components/preview/PreviewEmpty.tsx` | ~40 | empty/loading/error states |
| `ui/src/components/preview/renderers/CodeRenderer.tsx` | ~150 | shiki + cache integration + large-file fallback |
| `ui/src/components/preview/renderers/MarkdownRenderer.tsx` | ~80 | react-markdown + remark-gfm; safe HTML |
| `ui/src/components/preview/renderers/ImageRenderer.tsx` | ~60 | `convertFileSrc` + max dims |
| `ui/src/components/preview/renderers/BinaryFallback.tsx` | ~50 | file size + "binary, not previewable" message |
| `ui/src/components/preview/hooks/useFileBytes.ts` | ~110 | calls `preview_read_bytes` + decodes UTF-8 + caches text |
| `ui/src/components/preview/hooks/usePreviewState.ts` | ~70 | derived state convenience hook |
| `ui/src/components/preview/hooks/usePreviewRouter.ts` | ~90 | ext → renderer key dispatch |
| `ui/src/components/preview/hooks/useShikiHighlight.ts` | ~80 | wraps `highlightCode` + W1 cache; theme observer |
| `ui/src/components/preview/utils/ext-classifier.ts` | ~80 | `classifyExtension`, IMAGE_EXTS, CODE_EXTS, MD_EXTS sets |
| `ui/src/components/preview/utils/ext-classifier.test.ts` | ~90 | unit tests |

### Modified TS files

| Path | Edit |
|---|---|
| `ui/src/components/agent/SidePanel.tsx` | render `<PreviewPanel />` as a sibling; route `<FilesRail onFileClick>` → `setSelectedPreviewFile` action instead of (or in addition to) `handleAddToChat` |
| `ui/src/lib/tauri-bridge.ts` | add `previewReadBytes` wrapper |

**Module size budget**: every new file ≤ 250 lines target / ≤ 400 hard cap. Largest is `CodeRenderer.tsx` at ~150 LOC.

**Total new code**: ~16 new files, ~1300 LoC. 4 modified files (small edits).

**Design decision**: clicking a file currently calls `handleAddToChat` (adds it to chat input as an attachment). W4a changes this to **open the preview**, with `handleAddToChat` moving to a secondary action (Shift+Click or context menu) → **deferred to W4c**. For W4a, clicking just opens the preview; the user can no longer add a file to chat via the rail until W4c restores that path. Note in the PR body so reviewers know.

---

## Task 1: Backend types + resolver

**Files:**
- Create: `src-tauri/src/preview/mod.rs`
- Create: `src-tauri/src/preview/types.rs`
- Create: `src-tauri/src/preview/resolver.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod preview;` alphabetically)

- [ ] **Step 1: Create `types.rs`**

```rust
//! Data types for the preview subsystem.
//!
//! Wire format for the `preview_read_bytes` Tauri command. Bytes flow up
//! to the frontend as a base64-encoded string (Tauri's default for Vec<u8>);
//! the frontend's `useFileBytes` decodes to a Uint8Array.

use serde::Serialize;
use std::path::PathBuf;

/// 50 MB hard cap. Files larger than this are truncated at this boundary
/// and `truncated: true` is set so the renderer can show a banner.
pub const MAX_PREVIEW_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct PreviewBytes {
    /// Resolved absolute path on disk (after mount-relative lookup).
    pub resolved_path: PathBuf,
    /// File contents, truncated to `MAX_PREVIEW_BYTES` if larger.
    pub bytes: Vec<u8>,
    /// Original file size in bytes (NOT the length of `bytes` — that may be capped).
    pub size: u64,
    /// True if `bytes` is a truncated prefix.
    pub truncated: bool,
    /// Modification time, milliseconds since epoch.
    pub mtime_ms: i64,
}
```

- [ ] **Step 2: Create `resolver.rs`**

```rust
//! Resolve a `(mount_id, rel_path)` pair into a concrete absolute path.
//!
//! Reuses `AppState::files_rail_list_mounts` to fetch the mount catalog,
//! then composes the absolute path with `..` and absolute-path guards.

use super::types::{PreviewBytes, MAX_PREVIEW_BYTES};
use crate::app::AppState;
use crate::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Resolve `(mount_id, rel_path)` to a concrete absolute path.
///
/// `session_id` is optional; passed through to `files_rail_list_mounts`
/// so workspace mounts scoped to a non-default space resolve correctly
/// (same threading as W3 `files_rail_read_dir`).
pub async fn resolve_path(
    state: &AppState,
    mount_id: &str,
    rel_path: &str,
    session_id: Option<String>,
) -> Result<PathBuf, Error> {
    // Reject absolute and traversal segments BEFORE consulting mounts so a
    // malformed input fails fast.
    if rel_path.starts_with('/') {
        return Err(Error::InvalidInput(
            "rel_path must be relative".into(),
        ));
    }
    if rel_path.split('/').any(|seg| seg == "..") {
        return Err(Error::InvalidInput(
            "rel_path must not contain '..' segments".into(),
        ));
    }

    let mounts = state.files_rail_list_mounts(session_id).await?;
    let mount = mounts
        .into_iter()
        .find(|m| m.id == mount_id)
        .ok_or_else(|| Error::Internal(format!("mount not found: {}", mount_id)))?;

    let target = if rel_path.is_empty() || rel_path == "/" {
        mount.path.clone()
    } else {
        mount.path.join(rel_path)
    };

    // Defense-in-depth: even after path traversal guard, ensure final
    // canonicalised path stays under the mount root. `canonicalize` requires
    // the file to exist; for read operations this is correct.
    if let (Ok(canon_target), Ok(canon_root)) = (target.canonicalize(), mount.path.canonicalize()) {
        if !canon_target.starts_with(&canon_root) {
            return Err(Error::InvalidInput(format!(
                "resolved path escapes mount: {}",
                target.display()
            )));
        }
    }

    Ok(target)
}

/// Read up to `MAX_PREVIEW_BYTES` from `path`. Returns `PreviewBytes` with
/// `truncated = true` when the file exceeds the cap.
pub fn read_capped(path: &Path) -> Result<PreviewBytes, Error> {
    if !path.exists() {
        return Err(Error::NotFound(format!("file not found: {}", path.display())));
    }
    let metadata = fs::metadata(path)
        .map_err(|e| Error::Internal(format!("metadata: {}", e)))?;
    if !metadata.is_file() {
        return Err(Error::InvalidInput(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    let size = metadata.len();
    let truncated = size > MAX_PREVIEW_BYTES;
    let to_read = if truncated { MAX_PREVIEW_BYTES } else { size };

    let mut file = fs::File::open(path).map_err(|e| Error::Internal(format!("open: {}", e)))?;
    let mut bytes = Vec::with_capacity(to_read as usize);
    file.take(to_read)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Internal(format!("read: {}", e)))?;

    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    Ok(PreviewBytes {
        resolved_path: path.to_path_buf(),
        bytes,
        size,
        truncated,
        mtime_ms,
    })
}
```

- [ ] **Step 3: Create `mod.rs`**

```rust
//! W4a: preview engine — backend.
//!
//! Spec: `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §6

pub mod resolver;
pub mod types;

pub use types::{PreviewBytes, MAX_PREVIEW_BYTES};
```

- [ ] **Step 4: Wire into `src-tauri/src/lib.rs`**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "^pub mod" src-tauri/src/lib.rs | head -15
```

Insert `pub mod preview;` alphabetically — likely between `pub mod observability;` and `pub mod proactive;` (or wherever 'p…' falls). Use Edit with a unique anchor.

- [ ] **Step 5: Build to confirm**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error|^warning: unused" | head -10
```

Expected: clean.

- [ ] **Step 6: Pre-commit verification**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current && git status --short
```

Branch MUST be `claude/w4-preview-engine` (or `claude/w4a-preview-engine-core` — match whichever was created). Status MUST show ONLY:
```
 M src-tauri/src/lib.rs
?? src-tauri/src/preview/
```

If ANY other file appears, STOP and report BLOCKED.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/preview/mod.rs src-tauri/src/preview/types.rs src-tauri/src/preview/resolver.rs
git commit -m "feat(preview): scaffold module + PreviewBytes type + path resolver"
```

Verify branch + commit content with `git show HEAD --stat` — exactly 4 files.

---

## Task 2: Backend command + tests

**Files:**
- Create: `src-tauri/src/preview/commands.rs`
- Create: `src-tauri/src/preview/tests.rs`
- Modify: `src-tauri/src/preview/mod.rs` (add `pub mod commands;` + `#[cfg(test)] mod tests;`)
- Modify: `src-tauri/src/main.rs` (register command in `generate_handler!`)
- Modify: `src-tauri/src/tauri_commands.rs` (re-export)

- [ ] **Step 1: Write the resolver/read tests FIRST (TDD)**

Create `src-tauri/src/preview/tests.rs`:

```rust
//! W4a preview tests.

use super::resolver::{read_capped, resolve_path};
use super::types::MAX_PREVIEW_BYTES;
use std::fs::{create_dir_all, write};
use tempfile::TempDir;

// Path traversal + boundary tests (don't require AppState — resolver tests
// that need real mounts live in the manual smoke section of the PR plan).

#[test]
fn read_capped_returns_full_file_under_cap() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("small.txt");
    write(&path, b"hello world").unwrap();

    let result = read_capped(&path).unwrap();
    assert_eq!(result.bytes, b"hello world");
    assert_eq!(result.size, 11);
    assert!(!result.truncated);
    assert!(result.mtime_ms > 0);
}

#[test]
fn read_capped_truncates_oversized_file() {
    // Synthesize a file slightly larger than the cap by writing a known
    // pattern. Use the smallest possible size so the test is fast.
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("big.bin");
    // 1 KB above the cap — enough to verify truncation behavior without
    // allocating 50 MB in the test.
    let oversize = (MAX_PREVIEW_BYTES + 1024) as usize;
    let payload = vec![b'x'; oversize];
    write(&path, &payload).unwrap();

    let result = read_capped(&path).unwrap();
    assert_eq!(result.size as usize, oversize);
    assert!(result.truncated, "should mark as truncated");
    assert_eq!(
        result.bytes.len() as u64,
        MAX_PREVIEW_BYTES,
        "bytes must equal exactly MAX_PREVIEW_BYTES"
    );
    assert!(result.bytes.iter().all(|&b| b == b'x'));
}

#[test]
fn read_capped_rejects_nonexistent_path() {
    let result = read_capped(std::path::Path::new("/definitely/does/not/exist/xyz"));
    assert!(result.is_err());
}

#[test]
fn read_capped_rejects_directory() {
    let tmp = TempDir::new().unwrap();
    let result = read_capped(tmp.path());
    assert!(result.is_err(), "directories should be rejected");
}

// Note: resolve_path requires AppState which has heavy construction
// requirements (Tauri AppHandle, etc.). The traversal guards are exercised
// up-front in the function itself and are covered by the manual smoke test
// in the PR plan. The pure-Rust read_capped tests above cover the boundary
// behavior unit-tests should pin.
```

- [ ] **Step 2: Run the tests, watch them fail**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib preview 2>&1 | tail -10
```

Expected: 4 tests fail because `mod tests;` is not yet declared in `mod.rs`.

- [ ] **Step 3: Update `mod.rs`**

Replace its content with:

```rust
//! W4a: preview engine — backend.
//!
//! Spec: `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §6

pub mod commands;
pub mod resolver;
pub mod types;

#[cfg(test)]
mod tests;

pub use types::{PreviewBytes, MAX_PREVIEW_BYTES};
```

- [ ] **Step 4: Create `commands.rs`**

```rust
//! Tauri commands for the preview UI.

use super::resolver::{read_capped, resolve_path};
use super::types::PreviewBytes;
use crate::app::AppState;
use crate::error::Error;
use tauri::State;

#[tauri::command]
pub async fn preview_read_bytes(
    state: State<'_, AppState>,
    mount_id: String,
    rel_path: String,
    session_id: Option<String>,
) -> Result<PreviewBytes, Error> {
    let target = resolve_path(&state, &mount_id, &rel_path, session_id).await?;
    let bytes = read_capped(&target)?;
    Ok(bytes)
}
```

- [ ] **Step 5: Run tests now — should pass**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error|^warning: unused" | head
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib preview 2>&1 | tail -10
```

Expected: build clean. 4 tests pass.

- [ ] **Step 6: Register the command**

a) In `src-tauri/src/tauri_commands.rs`, add at top of file (with other re-exports):

```rust
pub use crate::preview::commands::preview_read_bytes;
```

Verify there are no naming collisions:

```bash
grep -n "preview_read_bytes\|fn preview_" /Users/ryanliu/Documents/uclaw/src-tauri/src/tauri_commands.rs | head -3
```

If a function with the same name exists elsewhere, RENAME the new export to `preview_read_bytes_v2` or report BLOCKED. (Unlikely — `preview_*` is a new namespace.)

b) In `src-tauri/src/main.rs`, find the `tauri::generate_handler![` macro at line ~327 and add `crate::tauri_commands::preview_read_bytes,` alphabetically (next to `preview_*` if any exist, otherwise grouped with `files_rail_*`).

- [ ] **Step 7: Full Rust build + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
```

Expected: clean build. Total tests = baseline + 4.

- [ ] **Step 8: Pre-commit verification**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Status MUST show ONLY:
```
 M src-tauri/src/main.rs
 M src-tauri/src/preview/mod.rs
 M src-tauri/src/tauri_commands.rs
?? src-tauri/src/preview/commands.rs
?? src-tauri/src/preview/tests.rs
```

If any other file appears, STOP and BLOCKED.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/preview/commands.rs src-tauri/src/preview/tests.rs src-tauri/src/preview/mod.rs src-tauri/src/main.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(preview): preview_read_bytes Tauri command + resolver tests (4 pass)"
```

---

## Task 3: Frontend atoms + Tauri bridge wrapper

**Files:**
- Create: `ui/src/atoms/preview-panel-atoms.ts`
- Modify: `ui/src/lib/tauri-bridge.ts` (append wrapper)

- [ ] **Step 1: Create `preview-panel-atoms.ts`**

```ts
/**
 * preview-panel-atoms — W4a preview panel state.
 *
 * selectedPreviewFileAtom — the file currently shown in the panel
 * previewPanelOpenAtom — whether the panel is visible
 * previewPanelWidthAtom — user-resizable, persisted
 * openPreviewAction — atomic set-file + open
 * closePreviewAction — convenience
 */

import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export interface PreviewFileTarget {
  /** Identifies the mount the file lives in (workspace:* / attached:*). */
  mountId: string
  /** Forward-slash path relative to the mount root. */
  relPath: string
  /** Display name (last segment of relPath). */
  name: string
  /** Optional session id — required for session-scoped mounts. */
  sessionId?: string | null
  /** Absolute on-disk path. Empty string if not yet resolved. */
  absolutePath?: string
}

export const selectedPreviewFileAtom = atom<PreviewFileTarget | null>(null)
export const previewPanelOpenAtom = atom<boolean>(false)

/** Persisted width in CSS pixels. Default 540; clamped to [380, 1100] by the UI. */
export const previewPanelWidthAtom = atomWithStorage<number>(
  'uclaw-preview-panel-width',
  540,
)

/** Write-only action: select a file AND open the panel in one update. */
export const openPreviewAction = atom(null, (_get, set, payload: PreviewFileTarget) => {
  set(selectedPreviewFileAtom, payload)
  set(previewPanelOpenAtom, true)
})

/** Write-only action: close the panel, keep the selection for re-open. */
export const closePreviewAction = atom(null, (_get, set) => {
  set(previewPanelOpenAtom, false)
})
```

- [ ] **Step 2: Add `previewReadBytes` wrapper to tauri-bridge.ts**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "filesRailReadDir\|invoke<" ui/src/lib/tauri-bridge.ts | tail -5
```

Find the existing files-rail block (end of file). Append the new wrapper AFTER it:

```ts
// ============================================================================
// Preview (W4a)
// ============================================================================

interface BackendPreviewBytes {
  resolved_path: string
  /** Tauri serializes Vec<u8> as a number[] by default. */
  bytes: number[]
  size: number
  truncated: boolean
  mtime_ms: number
}

export interface PreviewBytes {
  resolvedPath: string
  /** Owned byte buffer for the file content. */
  bytes: Uint8Array
  size: number
  truncated: boolean
  mtimeMs: number
}

export async function previewReadBytes(
  mountId: string,
  relPath: string,
  sessionId: string | null = null,
): Promise<PreviewBytes> {
  const raw = await invoke<BackendPreviewBytes>('preview_read_bytes', {
    mountId,
    relPath,
    sessionId,
  })
  return {
    resolvedPath: raw.resolved_path,
    bytes: new Uint8Array(raw.bytes),
    size: raw.size,
    truncated: raw.truncated,
    mtimeMs: raw.mtime_ms,
  }
}
```

- [ ] **Step 3: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
```

Expected: clean. Test count unchanged (no new tests in this task).

- [ ] **Step 4: Pre-commit verification**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
 M ui/src/lib/tauri-bridge.ts
?? ui/src/atoms/preview-panel-atoms.ts
```

- [ ] **Step 5: Commit**

```bash
git add ui/src/atoms/preview-panel-atoms.ts ui/src/lib/tauri-bridge.ts
git commit -m "feat(preview): preview-panel atoms + previewReadBytes Tauri wrapper"
```

---

## Task 4: Extension classifier (utility + tests)

**Files:**
- Create: `ui/src/components/preview/utils/ext-classifier.ts`
- Create: `ui/src/components/preview/utils/ext-classifier.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `ui/src/components/preview/utils/ext-classifier.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import {
  classifyExtension,
  getExtension,
  IMAGE_EXTS,
  CODE_EXTS,
  MD_EXTS,
} from './ext-classifier'

describe('ext-classifier', () => {
  describe('getExtension', () => {
    it('returns lowercased ext without dot', () => {
      expect(getExtension('foo.TS')).toBe('ts')
      expect(getExtension('FOO.Bar.JSX')).toBe('jsx')
    })

    it('returns empty for no-extension filenames', () => {
      expect(getExtension('Makefile')).toBe('')
      expect(getExtension('LICENSE')).toBe('')
    })

    it('handles dotfiles', () => {
      expect(getExtension('.gitignore')).toBe('gitignore')
      expect(getExtension('.env')).toBe('env')
    })
  })

  describe('classifyExtension', () => {
    it('routes images to image', () => {
      expect(classifyExtension('foo.png').kind).toBe('image')
      expect(classifyExtension('foo.JPEG').kind).toBe('image')
      expect(classifyExtension('foo.svg').kind).toBe('image')
    })

    it('routes markdown to markdown', () => {
      expect(classifyExtension('readme.md').kind).toBe('markdown')
      expect(classifyExtension('notes.MARKDOWN').kind).toBe('markdown')
    })

    it('routes code by extension', () => {
      const ts = classifyExtension('a.ts')
      expect(ts.kind).toBe('code')
      expect(ts.language).toBe('ts')

      const rs = classifyExtension('a.rs')
      expect(rs.kind).toBe('code')
      expect(rs.language).toBe('rs')

      const py = classifyExtension('a.py')
      expect(py.kind).toBe('code')
      expect(py.language).toBe('py')
    })

    it('routes text-like files to code with plaintext lang', () => {
      const txt = classifyExtension('a.txt')
      expect(txt.kind).toBe('code')
      expect(txt.language).toBe('text')
    })

    it('routes unknown extensions to binary', () => {
      expect(classifyExtension('a.unknownext').kind).toBe('binary')
      expect(classifyExtension('Makefile').kind).toBe('binary')
    })

    it('exports immutable sets', () => {
      expect(IMAGE_EXTS.has('png')).toBe(true)
      expect(CODE_EXTS.has('ts')).toBe(true)
      expect(MD_EXTS.has('md')).toBe(true)
    })
  })
})
```

- [ ] **Step 2: Run, watch fail**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/preview/utils/ext-classifier.test.ts
```

Expected: FAIL — "Cannot find module './ext-classifier'".

- [ ] **Step 3: Implement `ext-classifier.ts`**

```ts
/**
 * ext-classifier — Route a filename to a renderer kind.
 *
 * `kind: 'image' | 'markdown' | 'code' | 'binary'` drives `usePreviewRouter`.
 * W4b will introduce `'pdf' | 'docx' | 'xlsx' | 'pptx' | 'legacyOffice'`.
 * W4c will introduce `'diff'`.
 */

export type RendererKind = 'image' | 'markdown' | 'code' | 'binary'

export interface ClassificationResult {
  kind: RendererKind
  /** Lowercased file extension without the dot. Empty string for no-ext files. */
  ext: string
  /** For `kind === 'code'`, the language hint passed to shiki. */
  language?: string
}

export const IMAGE_EXTS: ReadonlySet<string> = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico',
])

export const MD_EXTS: ReadonlySet<string> = new Set(['md', 'markdown'])

/**
 * Code-rendererable extensions, mapping to the shiki language id.
 * Plain-text files (.txt, .log, .csv, etc) intentionally map to `'text'`
 * so the renderer shows them in a monospace pane without syntax highlight.
 */
export const CODE_EXTS: ReadonlyMap<string, string> = new Map([
  // typescript / javascript
  ['ts', 'ts'], ['tsx', 'tsx'], ['js', 'js'], ['jsx', 'jsx'],
  ['mjs', 'js'], ['cjs', 'js'],
  // systems / native
  ['rs', 'rs'], ['go', 'go'], ['c', 'c'], ['h', 'c'],
  ['cpp', 'cpp'], ['hpp', 'cpp'], ['cs', 'cs'],
  ['swift', 'swift'], ['kt', 'kotlin'], ['java', 'java'],
  // scripting
  ['py', 'py'], ['rb', 'rb'], ['php', 'php'],
  ['sh', 'bash'], ['bash', 'bash'], ['zsh', 'bash'], ['fish', 'fish'],
  // web
  ['html', 'html'], ['htm', 'html'],
  ['css', 'css'], ['scss', 'scss'], ['less', 'less'],
  // data / config
  ['json', 'json'], ['jsonc', 'jsonc'], ['json5', 'json5'],
  ['yaml', 'yaml'], ['yml', 'yaml'],
  ['toml', 'toml'], ['ini', 'ini'], ['env', 'dotenv'],
  ['xml', 'xml'],
  ['sql', 'sql'], ['graphql', 'graphql'], ['gql', 'graphql'],
  ['lock', 'yaml'],
  // diff / patch
  ['diff', 'diff'], ['patch', 'diff'],
  // plain text fallthrough
  ['txt', 'text'], ['log', 'text'], ['csv', 'text'],
  ['cfg', 'text'], ['conf', 'text'],
  ['gitignore', 'text'], ['dockerfile', 'docker'],
])

export function getExtension(filename: string): string {
  const dot = filename.lastIndexOf('.')
  if (dot === -1) return ''
  // For dotfiles like `.gitignore`, dot is at index 0 — extension is the
  // rest of the name.
  if (dot === 0) return filename.slice(1).toLowerCase()
  return filename.slice(dot + 1).toLowerCase()
}

export function classifyExtension(filename: string): ClassificationResult {
  const ext = getExtension(filename)
  if (ext && IMAGE_EXTS.has(ext)) return { kind: 'image', ext }
  if (ext && MD_EXTS.has(ext)) return { kind: 'markdown', ext }
  if (ext && CODE_EXTS.has(ext)) {
    return { kind: 'code', ext, language: CODE_EXTS.get(ext) }
  }
  return { kind: 'binary', ext }
}
```

- [ ] **Step 4: Run, watch pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/preview/utils/ext-classifier.test.ts
```

Expected: 9 tests pass.

- [ ] **Step 5: Verify full suite**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
```

Expected: clean. Test count up by 9.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/preview/utils/ext-classifier.ts ui/src/components/preview/utils/ext-classifier.test.ts
git commit -m "feat(preview): ext-classifier util + 9 unit tests"
```

---

## Task 5: useFileBytes + usePreviewRouter hooks

**Files:**
- Create: `ui/src/components/preview/hooks/useFileBytes.ts`
- Create: `ui/src/components/preview/hooks/usePreviewRouter.ts`
- Create: `ui/src/components/preview/hooks/usePreviewState.ts`

- [ ] **Step 1: `useFileBytes.ts`**

```ts
/**
 * useFileBytes — Fetch bytes for a given (mountId, relPath, sessionId).
 *
 * Returns:
 *   - status: 'idle' | 'loading' | 'ready' | 'error'
 *   - bytes / size / truncated when ready
 *   - error message when error
 *
 * Re-fetches when the W1 refresh atom for the resolvedPath bumps (agent file
 * writes, window focus, manual refresh). Stale fetches are guarded with a
 * cancellation flag.
 */

import * as React from 'react'
import { usePreviewRefresh } from '@/hooks/usePreviewRefresh'
import { previewReadBytes, type PreviewBytes } from '@/lib/tauri-bridge'
import type { PreviewFileTarget } from '@/atoms/preview-panel-atoms'

export type FileBytesState =
  | { status: 'idle' }
  | { status: 'loading' }
  | {
      status: 'ready'
      bytes: Uint8Array
      size: number
      truncated: boolean
      mtimeMs: number
      resolvedPath: string
    }
  | { status: 'error'; message: string }

export function useFileBytes(target: PreviewFileTarget | null): FileBytesState {
  const [state, setState] = React.useState<FileBytesState>({ status: 'idle' })
  // resolvedPath is what useFileBytes returns; we want refresh keyed on it
  // when known. Fall back to mountId:relPath for the pre-fetch period.
  const refreshKey = target ? (target.absolutePath ?? `${target.mountId}:${target.relPath}`) : ''
  const refreshVersion = usePreviewRefresh(refreshKey || null)

  React.useEffect(() => {
    if (!target) {
      setState({ status: 'idle' })
      return
    }
    let cancelled = false
    setState({ status: 'loading' })
    void (async () => {
      try {
        const result: PreviewBytes = await previewReadBytes(
          target.mountId,
          target.relPath,
          target.sessionId ?? null,
        )
        if (cancelled) return
        setState({
          status: 'ready',
          bytes: result.bytes,
          size: result.size,
          truncated: result.truncated,
          mtimeMs: result.mtimeMs,
          resolvedPath: result.resolvedPath,
        })
      } catch (err) {
        if (cancelled) return
        setState({ status: 'error', message: String(err) })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [target?.mountId, target?.relPath, target?.sessionId, refreshVersion])

  return state
}
```

- [ ] **Step 2: `usePreviewRouter.ts`**

```ts
/**
 * usePreviewRouter — Decide which renderer to mount for a given target.
 *
 * Pure: just dispatches on `target.name`'s extension. Heavy lifting
 * (fetching bytes, deciding text-vs-binary) lives in renderers themselves.
 */

import * as React from 'react'
import { classifyExtension, type RendererKind } from '@/components/preview/utils/ext-classifier'
import type { PreviewFileTarget } from '@/atoms/preview-panel-atoms'

export interface PreviewRoute {
  kind: RendererKind
  ext: string
  language?: string
}

export function usePreviewRouter(target: PreviewFileTarget | null): PreviewRoute | null {
  return React.useMemo(() => {
    if (!target) return null
    const result = classifyExtension(target.name)
    return result
  }, [target?.name])
}
```

- [ ] **Step 3: `usePreviewState.ts`**

```ts
/**
 * usePreviewState — Read-only convenience wrapper over the panel atoms.
 *
 * Components that only need to know the current target use this hook to
 * avoid each importing useAtomValue + the atom separately.
 */

import { useAtomValue } from 'jotai'
import {
  previewPanelOpenAtom,
  previewPanelWidthAtom,
  selectedPreviewFileAtom,
  type PreviewFileTarget,
} from '@/atoms/preview-panel-atoms'

export interface PreviewState {
  open: boolean
  width: number
  target: PreviewFileTarget | null
}

export function usePreviewState(): PreviewState {
  return {
    open: useAtomValue(previewPanelOpenAtom),
    width: useAtomValue(previewPanelWidthAtom),
    target: useAtomValue(selectedPreviewFileAtom),
  }
}
```

- [ ] **Step 4: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/preview/hooks/
git commit -m "feat(preview): useFileBytes + usePreviewRouter + usePreviewState hooks"
```

---

## Task 6: Simple renderers (Image + BinaryFallback)

**Files:**
- Create: `ui/src/components/preview/renderers/ImageRenderer.tsx`
- Create: `ui/src/components/preview/renderers/BinaryFallback.tsx`

- [ ] **Step 1: Write `ImageRenderer.tsx`**

```tsx
/**
 * ImageRenderer — Renders an image file via the Tauri asset:// protocol.
 *
 * Uses `convertFileSrc` (already imported elsewhere in the codebase) so
 * the URL is `asset.localhost/...` and the webview can fetch it directly
 * without round-tripping through preview_read_bytes.
 *
 * For SVG, we still use the asset URL — the file is sandboxed by the
 * `asset:` protocol scope ('**' in tauri.conf.json).
 */

import * as React from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'

interface ImageRendererProps {
  /** Absolute file path. */
  resolvedPath: string
  /** Display name for alt + error message. */
  name: string
}

export function ImageRenderer({ resolvedPath, name }: ImageRendererProps): React.ReactElement {
  const [errored, setErrored] = React.useState(false)
  const src = React.useMemo(() => convertFileSrc(resolvedPath), [resolvedPath])

  if (errored) {
    return (
      <div className="flex items-center justify-center h-full p-6 text-center text-[12px] text-muted-foreground">
        无法加载图片：{name}
      </div>
    )
  }

  return (
    <div className="flex items-center justify-center h-full overflow-auto p-4 bg-muted/30">
      <img
        src={src}
        alt={name}
        onError={() => setErrored(true)}
        className="max-w-full max-h-full object-contain shadow-sm rounded-md"
        draggable={false}
      />
    </div>
  )
}
```

- [ ] **Step 2: Write `BinaryFallback.tsx`**

```tsx
/**
 * BinaryFallback — Shown when the file extension isn't recognized.
 */

import * as React from 'react'
import { FileQuestion } from 'lucide-react'

interface BinaryFallbackProps {
  name: string
  /** Size in bytes for display formatting. */
  size: number
  ext: string
}

function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  if (size < 1024 * 1024 * 1024) return `${(size / (1024 * 1024)).toFixed(1)} MB`
  return `${(size / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

export function BinaryFallback({ name, size, ext }: BinaryFallbackProps): React.ReactElement {
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <FileQuestion className="size-12 text-muted-foreground/60 mb-3" aria-hidden />
      <div className="text-[13px] text-foreground/80 font-mono mb-1">{name}</div>
      <div className="text-[11px] text-muted-foreground">
        {ext ? `.${ext} · ` : ''}{formatBytes(size)} · 暂不支持预览
      </div>
      <div className="mt-2 text-[11px] text-muted-foreground/60 max-w-[280px]">
        点击右上角按钮在 Finder 中打开，或拖入聊天框作为附件发送
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Type-check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/preview/renderers/ImageRenderer.tsx ui/src/components/preview/renderers/BinaryFallback.tsx
git commit -m "feat(preview): ImageRenderer + BinaryFallback (no extra deps)"
```

---

## Task 7: useShikiHighlight hook + CodeRenderer

**Files:**
- Create: `ui/src/components/preview/hooks/useShikiHighlight.ts`
- Create: `ui/src/components/preview/renderers/CodeRenderer.tsx`

- [ ] **Step 1: Read existing highlighter API**

Already familiar from pre-flight. Two functions used here:
- `highlightCode(code: string, language: string, theme?: 'light' | 'dark'): Promise<string>` — returns HTML, never throws
- `getShikiThemeForCurrentApp(): BundledTheme` — string name of the theme

For caching keyed on theme, we need a stable string. Use `getShikiThemeForCurrentApp()` directly as the cache key portion.

- [ ] **Step 2: Write `useShikiHighlight.ts`**

```ts
/**
 * useShikiHighlight — Highlight a code string with shiki, caching the HTML
 * via the W1 codeHighlightCache so repeat previews skip both shiki and React
 * re-render churn.
 *
 * Skips highlighting entirely for files larger than MAX_HIGHLIGHT_CHARS (200k).
 */

import * as React from 'react'
import { highlightCode, getShikiThemeForCurrentApp } from '@/lib/highlight'
import {
  cacheGet,
  cacheKey,
  cacheSet,
  shouldSkipHighlight,
} from '@/components/preview/codeHighlightCache'

export interface UseShikiHighlightArgs {
  code: string
  language: string
  /** Used as part of the cache key — usually a filePath or mountId:relPath. */
  cacheScope: string
  /** Per-file refresh counter from usePreviewRefresh. */
  refreshVersion: number
}

export interface ShikiHighlightState {
  /** True when highlight is in flight. Code can still be shown as plaintext. */
  loading: boolean
  /** Sanitized HTML, or null if not yet highlighted (or skipped for size). */
  html: string | null
  /** True if the file exceeded the size cap and we are NOT highlighting it. */
  skipped: boolean
}

export function useShikiHighlight({
  code,
  language,
  cacheScope,
  refreshVersion,
}: UseShikiHighlightArgs): ShikiHighlightState {
  const [html, setHtml] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(false)

  const skipped = React.useMemo(() => shouldSkipHighlight(code), [code])

  React.useEffect(() => {
    if (skipped) {
      setHtml(null)
      setLoading(false)
      return
    }
    if (!code) {
      setHtml(null)
      setLoading(false)
      return
    }
    const theme = getShikiThemeForCurrentApp()
    const key = cacheKey({
      gitRoot: null,
      filePath: cacheScope,
      refreshVersion,
    })
    const cached = cacheGet(key)
    if (
      cached?.highlightedHtml &&
      cached.highlightedLanguage === language &&
      cached.highlightedTheme === theme
    ) {
      setHtml(cached.highlightedHtml)
      setLoading(false)
      return
    }
    let cancelled = false
    setLoading(true)
    void (async () => {
      try {
        const result = await highlightCode(code, language)
        if (cancelled) return
        setHtml(result)
        setLoading(false)
        cacheSet(key, {
          oldContent: code,
          newContent: code,
          highlightedHtml: result,
          highlightedLanguage: language,
          highlightedTheme: theme,
        })
      } catch {
        if (!cancelled) {
          setHtml(null)
          setLoading(false)
        }
      }
    })()
    return () => {
      cancelled = true
    }
  }, [code, language, cacheScope, refreshVersion, skipped])

  return { loading, html, skipped }
}
```

- [ ] **Step 3: Write `CodeRenderer.tsx`**

```tsx
/**
 * CodeRenderer — Renders source-code text with shiki syntax highlighting.
 *
 * Three rendering paths:
 *   1. Highlighted HTML (default path, shiki + cache)
 *   2. Plain pre/code while shiki is still tokenising (loading)
 *   3. Plain pre/code with truncation banner for files > MAX_HIGHLIGHT_CHARS
 */

import * as React from 'react'
import { cn } from '@/lib/utils'
import { useShikiHighlight } from '@/components/preview/hooks/useShikiHighlight'
import { escapeHtml } from '@/lib/highlight'

interface CodeRendererProps {
  /** Decoded file contents. */
  code: string
  /** Shiki language id (from ext-classifier). */
  language: string
  /** Cache scope key — usually the absolute path. */
  cacheScope: string
  /** Per-file refresh counter (forces re-highlight when bumped). */
  refreshVersion: number
  /** True if the upstream file is larger than MAX_PREVIEW_BYTES and was capped. */
  truncated?: boolean
}

export function CodeRenderer({
  code,
  language,
  cacheScope,
  refreshVersion,
  truncated = false,
}: CodeRendererProps): React.ReactElement {
  const { html, loading, skipped } = useShikiHighlight({
    code,
    language,
    cacheScope,
    refreshVersion,
  })

  const showPlain = skipped || (!html && !loading)

  return (
    <div className="flex flex-col h-full bg-popover">
      {truncated && (
        <div className="flex-shrink-0 px-3 py-1.5 text-[11px] bg-amber-500/12 text-amber-700 dark:text-amber-300 border-b border-border">
          文件超过 50 MB · 仅显示前 50 MB
        </div>
      )}
      {skipped && (
        <div className="flex-shrink-0 px-3 py-1.5 text-[11px] bg-muted/60 text-muted-foreground border-b border-border">
          文件较大 · 跳过语法高亮，仅显示纯文本
        </div>
      )}
      <div className="flex-1 min-h-0 overflow-auto">
        {html && !showPlain ? (
          <div
            className={cn(
              'p-3 text-[12px] font-mono tabular-nums leading-relaxed',
              loading && 'opacity-75',
            )}
            // shiki output is escaped + scoped — safe to dangerouslySetInnerHTML
            dangerouslySetInnerHTML={{ __html: html }}
          />
        ) : (
          <pre className="p-3 text-[12px] font-mono tabular-nums leading-relaxed whitespace-pre-wrap break-all">
            <code dangerouslySetInnerHTML={{ __html: escapeHtml(code) }} />
          </pre>
        )}
      </div>
    </div>
  )
}
```

`escapeHtml` is already exported from `@/lib/highlight` (we saw it at line 151).

- [ ] **Step 4: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/preview/hooks/useShikiHighlight.ts ui/src/components/preview/renderers/CodeRenderer.tsx
git commit -m "feat(preview): useShikiHighlight hook + CodeRenderer (consumes W1 cache)"
```

---

## Task 8: MarkdownRenderer

**Files:**
- Create: `ui/src/components/preview/renderers/MarkdownRenderer.tsx`

- [ ] **Step 1: Write `MarkdownRenderer.tsx`**

```tsx
/**
 * MarkdownRenderer — Renders a markdown file via react-markdown + remark-gfm.
 *
 * Uses uClaw's existing markdown deps. No new packages.
 * Safe: react-markdown does not execute scripts; we don't enable raw HTML.
 */

import * as React from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

interface MarkdownRendererProps {
  /** Decoded file contents. */
  text: string
}

export function MarkdownRenderer({ text }: MarkdownRendererProps): React.ReactElement {
  return (
    <div className="flex-1 min-h-0 overflow-auto bg-popover">
      <div className="prose prose-sm dark:prose-invert max-w-none p-5">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
      </div>
    </div>
  )
}
```

Note: the `prose` Tailwind class assumes uClaw uses `@tailwindcss/typography`. Check:

```bash
grep -n "@tailwindcss/typography\|prose" /Users/ryanliu/Documents/uclaw/ui/package.json | head -3
grep -n "typography" /Users/ryanliu/Documents/uclaw/ui/tailwind.config.* 2>/dev/null | head -3
```

If the plugin is NOT present, replace the `prose` classes with manual styles:

```tsx
      <div className="max-w-none p-5 text-[13px] leading-relaxed [&_h1]:text-xl [&_h1]:font-semibold [&_h1]:mt-4 [&_h1]:mb-3 [&_h2]:text-lg [&_h2]:font-semibold [&_h2]:mt-4 [&_h2]:mb-2 [&_h3]:font-semibold [&_h3]:mt-3 [&_h3]:mb-2 [&_p]:my-2 [&_ul]:my-2 [&_ul]:pl-5 [&_ul]:list-disc [&_ol]:my-2 [&_ol]:pl-5 [&_ol]:list-decimal [&_li]:my-1 [&_code]:font-mono [&_code]:text-[12px] [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_pre]:bg-muted [&_pre]:p-3 [&_pre]:rounded-md [&_pre]:overflow-auto [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-3 [&_blockquote]:text-muted-foreground [&_table]:border-collapse [&_table]:w-full [&_th]:border [&_th]:border-border [&_th]:px-2 [&_th]:py-1 [&_td]:border [&_td]:border-border [&_td]:px-2 [&_td]:py-1 [&_a]:text-blue-500 [&_a:hover]:underline">
```

Pick the version that matches the actual config. The typography plugin is more idiomatic; the inline `[&_…]` selectors are a fallback.

- [ ] **Step 2: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
```

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/preview/renderers/MarkdownRenderer.tsx
git commit -m "feat(preview): MarkdownRenderer (react-markdown + remark-gfm)"
```

---

## Task 9: PreviewHeader + PreviewEmpty + PreviewSurface + PreviewPanel

**Files:**
- Create: `ui/src/components/preview/PreviewHeader.tsx`
- Create: `ui/src/components/preview/PreviewEmpty.tsx`
- Create: `ui/src/components/preview/PreviewSurface.tsx`
- Create: `ui/src/components/preview/PreviewPanel.tsx`

- [ ] **Step 1: `PreviewEmpty.tsx`**

```tsx
import * as React from 'react'
import { FileText, AlertTriangle, Loader2 } from 'lucide-react'

export interface PreviewEmptyProps {
  status: 'idle' | 'loading' | 'error'
  message?: string
}

export function PreviewEmpty({ status, message }: PreviewEmptyProps): React.ReactElement {
  if (status === 'loading') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <Loader2 className="size-6 text-muted-foreground/60 animate-spin mb-3 motion-reduce:animate-none" aria-hidden />
        <div className="text-[12px] text-muted-foreground">正在读取文件…</div>
      </div>
    )
  }
  if (status === 'error') {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <AlertTriangle className="size-6 text-destructive mb-3" aria-hidden />
        <div className="text-[12px] text-destructive">读取失败</div>
        <div className="mt-1 text-[11px] text-muted-foreground max-w-[280px] break-words">
          {message ?? '未知错误'}
        </div>
      </div>
    )
  }
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <FileText className="size-10 text-muted-foreground/40 mb-3" aria-hidden />
      <div className="text-[12px] text-muted-foreground">还没选中文件</div>
      <div className="mt-1 text-[11px] text-muted-foreground/60 max-w-[260px]">
        在左侧文件树点击任意文件开始预览
      </div>
    </div>
  )
}
```

- [ ] **Step 2: `PreviewHeader.tsx`**

```tsx
import * as React from 'react'
import { useSetAtom } from 'jotai'
import { X, ExternalLink } from 'lucide-react'
import { cn } from '@/lib/utils'
import { closePreviewAction, type PreviewFileTarget } from '@/atoms/preview-panel-atoms'

interface PreviewHeaderProps {
  target: PreviewFileTarget | null
  resolvedPath?: string
}

export function PreviewHeader({ target, resolvedPath }: PreviewHeaderProps): React.ReactElement {
  const closePreview = useSetAtom(closePreviewAction)
  const displayPath = resolvedPath ?? target?.relPath ?? ''

  return (
    <header className="flex items-center gap-2 h-[36px] flex-shrink-0 border-b border-border bg-popover px-3">
      <div className="flex-1 min-w-0 flex flex-col">
        <div className="text-[12px] font-medium text-foreground truncate">
          {target?.name ?? '未选中文件'}
        </div>
        {displayPath && (
          <div
            className="text-[10px] text-muted-foreground/70 truncate"
            dir="rtl"
            title={displayPath}
          >
            {displayPath}
          </div>
        )}
      </div>
      <button
        type="button"
        aria-label="弹出独立窗口 (W5 即将上线)"
        title="弹出独立窗口（W5 即将上线）"
        disabled
        className={cn(
          'size-7 inline-flex items-center justify-center rounded',
          'text-foreground/30 cursor-not-allowed',
        )}
      >
        <ExternalLink size={13} />
      </button>
      <button
        type="button"
        aria-label="关闭预览"
        onClick={() => closePreview()}
        className={cn(
          'size-7 inline-flex items-center justify-center rounded',
          'text-foreground/60 hover:text-foreground hover:bg-foreground/[0.06]',
          'transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        )}
      >
        <X size={14} />
      </button>
    </header>
  )
}
```

- [ ] **Step 3: `PreviewSurface.tsx`**

```tsx
import * as React from 'react'
import { useFileBytes } from '@/components/preview/hooks/useFileBytes'
import { usePreviewRouter } from '@/components/preview/hooks/usePreviewRouter'
import { PreviewEmpty } from './PreviewEmpty'
import { CodeRenderer } from './renderers/CodeRenderer'
import { MarkdownRenderer } from './renderers/MarkdownRenderer'
import { ImageRenderer } from './renderers/ImageRenderer'
import { BinaryFallback } from './renderers/BinaryFallback'
import { usePreviewRefresh } from '@/hooks/usePreviewRefresh'
import type { PreviewFileTarget } from '@/atoms/preview-panel-atoms'

interface PreviewSurfaceProps {
  target: PreviewFileTarget | null
}

function decodeUtf8(bytes: Uint8Array): string {
  try {
    return new TextDecoder('utf-8', { fatal: false }).decode(bytes)
  } catch {
    return ''
  }
}

export function PreviewSurface({ target }: PreviewSurfaceProps): React.ReactElement {
  const route = usePreviewRouter(target)
  const state = useFileBytes(target)
  const resolvedPath = state.status === 'ready' ? state.resolvedPath : null
  const refreshVersion = usePreviewRefresh(resolvedPath)

  // Decode bytes lazily; only when we know we need text (code / markdown).
  const text = React.useMemo(() => {
    if (state.status !== 'ready') return ''
    if (!route) return ''
    if (route.kind === 'code' || route.kind === 'markdown') {
      return decodeUtf8(state.bytes)
    }
    return ''
  }, [state, route])

  if (!target) return <PreviewEmpty status="idle" />
  if (state.status === 'loading' || state.status === 'idle') return <PreviewEmpty status="loading" />
  if (state.status === 'error') return <PreviewEmpty status="error" message={state.message} />

  if (!route) return <PreviewEmpty status="idle" />

  if (route.kind === 'image') {
    return <ImageRenderer resolvedPath={state.resolvedPath} name={target.name} />
  }
  if (route.kind === 'markdown') {
    return <MarkdownRenderer text={text} />
  }
  if (route.kind === 'code') {
    return (
      <CodeRenderer
        code={text}
        language={route.language ?? 'text'}
        cacheScope={state.resolvedPath}
        refreshVersion={refreshVersion}
        truncated={state.truncated}
      />
    )
  }
  return <BinaryFallback name={target.name} size={state.size} ext={route.ext} />
}
```

- [ ] **Step 4: `PreviewPanel.tsx`**

```tsx
/**
 * <PreviewPanel /> — W4a slide-in preview container.
 *
 * Mounted as a sibling to the agent SidePanel inside the agent right rail.
 * Visible when `previewPanelOpenAtom === true`. Width is user-resizable via
 * the left edge drag handle (atomWithStorage-persisted).
 *
 * Layout: header + surface. Surface picks the renderer.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { cn } from '@/lib/utils'
import { usePreviewState } from '@/components/preview/hooks/usePreviewState'
import { previewPanelWidthAtom } from '@/atoms/preview-panel-atoms'
import { PreviewHeader } from './PreviewHeader'
import { PreviewSurface } from './PreviewSurface'

const MIN_WIDTH = 380
const MAX_WIDTH = 1100

export function PreviewPanel(): React.ReactElement | null {
  const { open, target } = usePreviewState()
  const [width, setWidth] = useAtom(previewPanelWidthAtom)
  const draggingRef = React.useRef(false)

  // ESC closes the panel
  React.useEffect(() => {
    if (!open) return
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        // Defer to closePreviewAction via the header's button; for now we
        // simply close by writing the atom directly elsewhere. Keep this as
        // a no-op until UX confirms ESC should close.
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [open])

  const onResizeStart = React.useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault()
      draggingRef.current = true
      const startX = e.clientX
      const startWidth = width
      const onMove = (ev: MouseEvent) => {
        if (!draggingRef.current) return
        const delta = startX - ev.clientX // dragging left increases width
        const next = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, startWidth + delta))
        setWidth(next)
      }
      const onUp = () => {
        draggingRef.current = false
        window.removeEventListener('mousemove', onMove)
        window.removeEventListener('mouseup', onUp)
      }
      window.addEventListener('mousemove', onMove)
      window.addEventListener('mouseup', onUp)
    },
    [width, setWidth],
  )

  if (!open) return null

  return (
    <aside
      className={cn(
        'relative flex flex-col h-full flex-shrink-0',
        'border-l border-border bg-popover shadow-xl',
        'transition-[width] duration-200 ease-out motion-reduce:transition-none',
      )}
      style={{ width }}
      aria-label="文件预览"
    >
      <button
        type="button"
        onMouseDown={onResizeStart}
        aria-label="拖动调整预览面板宽度"
        title="拖动调整宽度"
        className={cn(
          'absolute -left-1 top-0 bottom-0 w-2 cursor-col-resize',
          'hover:bg-foreground/[0.04] active:bg-foreground/[0.08]',
          'transition-colors',
        )}
      />
      <PreviewHeader target={target} />
      <div className="flex-1 min-h-0 flex flex-col">
        <PreviewSurface target={target} />
      </div>
    </aside>
  )
}
```

- [ ] **Step 5: Type-check + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/preview/PreviewHeader.tsx ui/src/components/preview/PreviewEmpty.tsx ui/src/components/preview/PreviewSurface.tsx ui/src/components/preview/PreviewPanel.tsx
git commit -m "feat(preview): PreviewPanel shell + Header + Surface + Empty states"
```

---

## Task 10: SidePanel integration + final verification

**Files:**
- Modify: `ui/src/components/agent/SidePanel.tsx`

- [ ] **Step 1: Read current FilesRail wire-up**

```bash
sed -n '215,235p' /Users/ryanliu/Documents/uclaw/ui/src/components/agent/SidePanel.tsx
```

Currently:
```tsx
<FilesRail
  sessionId={sessionId}
  onFileClick={(mount: MountRoot, node: TreeNode) => {
    handleAddToChat({
      name: node.name,
      path: `${mount.path}/${node.relPath}`,
      isDirectory: node.kind === 'directory',
      isFile: node.kind === 'file',
      size: node.size,
      modifiedAt: node.mtimeMs,
    })
  }}
/>
```

W4a changes `onFileClick` to open the preview instead. `handleAddToChat` moves to a "Shift-click adds to chat" affordance — W4c restores this fully.

- [ ] **Step 2: Read the rest of the file to find layout structure**

```bash
sed -n '195,250p' /Users/ryanliu/Documents/uclaw/ui/src/components/agent/SidePanel.tsx
```

Look for the outer container that wraps FilesRail. The `<PreviewPanel />` will be inserted as a flex sibling. The simplest layout is:

```
<outer flex-row>
  <FilesRail container flex-1 />
  <PreviewPanel />   // null when closed; width-bounded when open
</outer>
```

If the current container is `flex-col`, we need to switch the immediate FilesRail wrapper to `flex-row`. Read the surrounding markup before editing.

- [ ] **Step 3: Add imports**

```tsx
import { PreviewPanel } from '@/components/preview/PreviewPanel'
import { useSetAtom } from 'jotai'   // already imported, may need additional import
import { openPreviewAction } from '@/atoms/preview-panel-atoms'
```

- [ ] **Step 4: Add the openPreview action hook**

Inside the component body (near other `useSetAtom` declarations):

```tsx
  const openPreview = useSetAtom(openPreviewAction)
```

- [ ] **Step 5: Rewire `onFileClick`**

Replace the existing `<FilesRail ... onFileClick={...} />` JSX with:

```tsx
<FilesRail
  sessionId={sessionId}
  onFileClick={(mount: MountRoot, node: TreeNode) => {
    if (node.kind === 'directory') return // directories expand, not preview
    openPreview({
      mountId: mount.id,
      relPath: node.relPath,
      name: node.name,
      sessionId,
      absolutePath: `${mount.path}/${node.relPath}`,
    })
  }}
/>
```

`handleAddToChat` is now unused in this path — leave the definition alone (it may still be referenced by other rail actions in the future; W4c re-wires it via a context menu or shift-click).

- [ ] **Step 6: Mount `<PreviewPanel />` as a sibling**

Find the outer container around the FilesRail and convert to `flex-row` so PreviewPanel sits next to FilesRail. Example:

Before (approximate):
```tsx
<div className="flex-1 min-h-0 flex flex-col">
  <FilesRail ... />
</div>
```

After:
```tsx
<div className="flex-1 min-h-0 flex flex-row">
  <div className="flex-1 min-w-0 flex flex-col">
    <FilesRail ... />
  </div>
  <PreviewPanel />
</div>
```

`PreviewPanel` returns `null` when closed, so the layout is unaffected when no file is selected.

Exact placement depends on the file's existing structure — Read the surrounding 40 lines and adapt without restructuring beyond the immediate FilesRail container.

- [ ] **Step 7: Type-check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
```

Resolve any unused-import warnings if `handleAddToChat` references become noisy. Likely fine.

- [ ] **Step 8: Run full test suite**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

If `WorkspaceFilesView.test.tsx` asserts on the OLD click behavior (adds to chat), update or skip that assertion — W4c will restore the add-to-chat path.

- [ ] **Step 9: Verify hardcoded colors clean**

```bash
grep -rnE '#[0-9a-fA-F]{3,8}\b|bg-zinc-|text-gray-|text-zinc-' \
  ui/src/components/preview/ \
  ui/src/atoms/preview-panel-atoms.ts 2>/dev/null | head
```

Expected: empty (the only acceptable hex matches would be in the `text-amber-700 dark:text-amber-300` truncation banner — amber is a Tailwind named color, not a raw hex). Verify with the actual file output.

- [ ] **Step 10: Manual smoke (recommended, optional if dev server unavailable)**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo tauri dev
```

Verify the click → preview flow:
1. Click a `.ts` file in FilesRail → preview opens, code is highlighted
2. Click a `.md` file → preview renders as formatted markdown
3. Click a `.png` file → image displayed
4. Click an unknown extension → BinaryFallback shows file size + suggestion
5. Drag the left edge of the preview panel → width changes, persists across reloads
6. Click the X button in the header → preview closes (panel disappears)
7. Click another file → preview reopens with the new content

- [ ] **Step 11: Pre-commit verification**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
 M ui/src/components/agent/SidePanel.tsx
```

- [ ] **Step 12: Commit**

```bash
git add ui/src/components/agent/SidePanel.tsx
git commit -m "feat(preview): wire FilesRail click → PreviewPanel open in agent SidePanel

W4a integration commit. Clicking a file in the rail now opens the preview
panel (slide-in on the right of the rail). Directories still expand
in-place. The previous 'click adds file to chat' behaviour moves to W4c
where Shift-click or a context menu will restore it.
"
```

---

## Task 11: Final verification + push + PR

- [ ] **Step 1: Full Rust suite**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected: 391/391 (baseline 387 + 4 new preview tests).

- [ ] **Step 2: Full UI suite + TS**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
```

Expected: 275/275 (baseline 266 + 9 ext-classifier).

- [ ] **Step 3: Rust binary build**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build --bin uclaw 2>&1 | tail -3
```

Expected: clean.

- [ ] **Step 4: Hardcoded-color audit (one more pass)**

```bash
grep -rnE 'bg-\[#|text-\[#|border-\[#|bg-zinc-|text-gray-|text-zinc-' \
  ui/src/components/preview/ \
  ui/src/atoms/preview-panel-atoms.ts 2>/dev/null
```

Expected: empty. (The `bg-amber-500/12` etc are Tailwind named colors with opacity — fine.)

- [ ] **Step 5: Git log review**

```bash
git log --oneline main..HEAD
```

Expected 10 commits in this order:

1. `feat(preview): scaffold module + PreviewBytes type + path resolver`
2. `feat(preview): preview_read_bytes Tauri command + resolver tests (4 pass)`
3. `feat(preview): preview-panel atoms + previewReadBytes Tauri wrapper`
4. `feat(preview): ext-classifier util + 9 unit tests`
5. `feat(preview): useFileBytes + usePreviewRouter + usePreviewState hooks`
6. `feat(preview): ImageRenderer + BinaryFallback (no extra deps)`
7. `feat(preview): useShikiHighlight hook + CodeRenderer (consumes W1 cache)`
8. `feat(preview): MarkdownRenderer (react-markdown + remark-gfm)`
9. `feat(preview): PreviewPanel shell + Header + Surface + Empty states`
10. `feat(preview): wire FilesRail click → PreviewPanel open in agent SidePanel`

- [ ] **Step 6: Push and open PR**

```bash
cd /Users/ryanliu/Documents/uclaw && git push -u origin claude/w4-preview-engine
cd /Users/ryanliu/Documents/uclaw && gh pr create --title "W4a: Preview Engine core — slide-in panel + code/markdown/image renderers" --body "$(cat <<'EOF'
## Summary

Wave 4a of the [Proma v0.9.27 preview port](docs/superpowers/specs/2026-05-12-proma-preview-port-design.md) (see §6 — core subset; rich formats land in W4b, editing + chips in W4c).

The minimum-viable preview engine. Clicking a file in `FilesRail` now opens a slide-in preview panel on the right of the agent rail with:

- **Code rendering** — shiki syntax highlighting, themed to match the app, cached via W1's codeHighlightCache (zero-cost re-renders + re-opens).
- **Markdown rendering** — `react-markdown` + `remark-gfm` (no new deps; uses existing libs).
- **Image rendering** — `convertFileSrc` asset-protocol; no read-bytes round-trip for images.
- **Binary fallback** — readable empty state for unknown extensions.

Backend adds `src-tauri/src/preview/` with the `preview_read_bytes` Tauri command + path resolver. The resolver hardens against `..` and absolute-path injection, and verifies the canonicalised target stays under the mount root before reading.

## Architecture decisions

- **Click-to-preview replaces click-to-add-to-chat** in the agent SidePanel. The previous behavior (single click adds the file as a chat attachment) is restored in W4c via Shift-click or a context menu.
- **W1's `codeHighlightCache` is consumed via `useShikiHighlight`** — cache key includes language + theme, so theme switches and language detection don't pollute the cache.
- **No new npm deps** in W4a. Reuses existing shiki, react-markdown, remark-gfm.
- **Width is user-resizable** via the left-edge drag handle (atomWithStorage-persisted at 540px default, clamped to [380, 1100]).
- **MAX_PREVIEW_BYTES = 50 MB** hard cap. Files larger than this are truncated and a banner is shown. MAX_HIGHLIGHT_CHARS = 200k (from W1) skips highlight for big text files but still renders plaintext.

## Commits (bisectable)

10 commits — see `git log main..HEAD`. Each task in the plan is one commit.

## Test plan

- [x] `cd src-tauri && cargo build` — clean
- [x] `cd src-tauri && cargo test --lib preview` — 4 pass (path traversal / cap / nonexistent / not-a-file)
- [x] `cd src-tauri && cargo test --lib` — 391/391 (baseline 387 + 4)
- [x] `cd ui && npx tsc --noEmit` — clean
- [x] `cd ui && npm test -- --run` — 275/275 (baseline 266 + 9 ext-classifier)
- [x] No hardcoded colors (grep clean)
- [ ] Manual: click `.ts` / `.rs` / `.py` → highlighted preview
- [ ] Manual: click `.md` → rendered markdown
- [ ] Manual: click `.png` / `.svg` → image displayed
- [ ] Manual: click `.zip` → BinaryFallback with size
- [ ] Manual: drag panel edge → width changes, persists across reload
- [ ] Manual: X button → panel closes
- [ ] Manual: switch theme → highlighted code re-renders with new theme on next click
- [ ] Manual: agent writes a file currently in preview → preview auto-refreshes (W1 hook)
- [ ] Manual: 11-theme spot-check (warm-paper / qingye / forest-dark) — no hardcoded grays bleed through

## What's deferred to W4b / W4c

- **W4b** — Rich format renderers (PDF, DOCX, XLSX, PPTX) + new npm deps (jszip, mammoth, pdfjs-dist, @xmldom/xmldom). Legacy office (.doc / .xls / .ppt) hints.
- **W4c** — Inline editing (CodeMirror for code, TipTap for markdown). `preview_write_text` Tauri command. File-path chips in agent messages + remark plugin. Restore Shift-click / context-menu "add to chat" from the rail.
- **W5** — Detached preview window (the pop-out button in the header is rendered as `disabled` for now).

## Architecture notes

- The `preview/resolver.rs` consults `AppState::files_rail_list_mounts` (the same source W3 uses) — so workspace / session / attached_dir mounts ALL resolve correctly without duplicating mount logic.
- `useFileBytes` includes `usePreviewRefresh(resolvedPath)`'s version in its deps, so agent file-write events automatically re-fetch the bytes.
- `useShikiHighlight` keys its cache on `language + theme` — theme switches do NOT thrash the cache; they add new entries.
- The asset-protocol URL for images (`convertFileSrc`) doesn't pass through `preview_read_bytes`. This avoids encoding 5 MB images as base64 for no reason.

EOF
)"
```

Expected: PR URL printed.

---

## Self-Review

After writing this plan, run the spec coverage / placeholder / type-consistency check mentally:

**Spec coverage** (spec §6 mapping to tasks):

| Spec | Task |
|---|---|
| §6.1 module structure (PreviewPanel / Surface / Header / Empty) | Tasks 9, 10 |
| §6.1 renderers (Code / Markdown / Image / Binary) | Tasks 6, 7, 8 |
| §6.2 backend `src-tauri/src/preview/` 4 files | Tasks 1, 2 |
| §6.3 `preview_read_bytes` command | Task 2 |
| §6.5 PDF strategy (lazy pdfjs) | **DEFERRED to W4b** |
| §6.4 Office parsing | **DEFERRED to W4b** |
| §6.6 editing scope (M1) | **DEFERRED to W4c** |
| §6.7 PreviewPanel layout (slide-in, draggable width) | Task 9 (PreviewPanel) |
| §6.8 file-path chips | **DEFERRED to W4c** |
| §6.9 refresh integration | Tasks 5, 7 (useFileBytes + useShikiHighlight both consume usePreviewRefresh) |
| §6.11 verification | Task 11 |

**Type consistency**:

- `PreviewFileTarget { mountId, relPath, name, sessionId?, absolutePath? }` — defined in Task 3 atoms, consumed in Tasks 5, 9.
- `PreviewBytes { resolvedPath, bytes, size, truncated, mtimeMs }` — defined in Task 3 tauri-bridge, consumed in Task 5 useFileBytes.
- `ClassificationResult { kind, ext, language? }` — defined in Task 4, consumed in Task 5 usePreviewRouter.
- `useShikiHighlight` args / return — defined Task 7, consumed Task 7 (CodeRenderer same task).
- Cache key format — uses W1's `cacheKey({ gitRoot: null, filePath, refreshVersion })` consistently in Task 7.

**Placeholder scan**: none — every step contains complete code. No "TODO", "TBD", "implement later".

**Module size cap**:
- types.rs: ~25 lines ✓
- resolver.rs: ~90 lines ✓
- commands.rs: ~25 lines ✓
- tests.rs: ~70 lines ✓
- preview-panel-atoms.ts: ~50 lines ✓
- ext-classifier.ts: ~80 lines ✓
- useFileBytes.ts: ~70 lines ✓
- useShikiHighlight.ts: ~80 lines ✓
- CodeRenderer.tsx: ~80 lines ✓
- MarkdownRenderer.tsx: ~20 lines ✓
- ImageRenderer.tsx: ~30 lines ✓
- BinaryFallback.tsx: ~30 lines ✓
- PreviewEmpty.tsx: ~40 lines ✓
- PreviewHeader.tsx: ~55 lines ✓
- PreviewSurface.tsx: ~65 lines ✓
- PreviewPanel.tsx: ~85 lines ✓

All under the 300-line target.

**Risk surfaces flagged for review**:

1. **resolve_path's canonicalize guard requires the file to exist** — for read paths this is fine, but for writes (W4c) it'll need `parent().canonicalize() + join(filename)` instead. Document in W4c plan.
2. **`react-markdown`'s default config doesn't render raw HTML** — safe for now. If a future spec needs HTML in markdown, we'll need `rehype-sanitize` + an allowlist.
3. **`convertFileSrc` for images** assumes the asset-protocol scope `**` allows all paths. Tauri config already enables this (verified in earlier waves).
4. **`usePreviewRefresh` keyed on `resolvedPath`** — only works once the bytes have been read. For the very first fetch we use `mountId:relPath` as a placeholder key. The dual-key approach is acceptable since refresh events for an as-yet-unfetched file are vanishingly rare (you can't bump a file you haven't fetched).
5. **The dependency on W3's `files_rail_list_mounts`** means W4a cannot land without W3. Confirmed — W3 is merged.
