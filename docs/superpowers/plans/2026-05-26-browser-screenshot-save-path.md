# Browser Screenshot Save Path

## Goal

Let `browser_screenshot` save the current PNG directly to a workspace file when the user asks for a saved screenshot, without requiring the LLM to decode base64 image data.

## Evidence

- Recent Apple Support screenshot calls returned `{ ok, data, width, height }` and the UI rendered the image from base64.
- Image-blind providers receive a stripped screenshot placeholder instead of raw base64, so they cannot reliably write the PNG through `write_file`.
- The tool had no `path_args` or `preview_target_path`, so safety and preview plumbing treated screenshots as read-only.

## Scope

- Add optional `path` / `save_path` to `browser_screenshot`.
- Resolve relative screenshot paths against the active/session workspace root.
- Decode PNG base64 in Rust and write the file from the browser tool.
- Surface the saved path in the tool output and auto-preview metadata.

## ADR 18 Questions

1. User value: screenshot save requests succeed from the same browser tool that captured the image.
2. Current truth: screenshots render in UI but are not persistable by image-blind LLMs.
3. Target truth: Rust owns binary screenshot persistence.
4. Boundary: no change to MCP raw-tool exposure or provider routing.
5. Data model: no schema change.
6. Runtime: writes only when an explicit path is supplied.
7. Safety: path goes through existing tool path policy before execution.
8. UX: saved files can open in the preview panel.
9. Rollback: remove optional path handling and explicit registration field.
10. Verification: focused Rust unit tests and cargo test for browser tool helpers.
11. Open risk: GitNexus does not resolve the macro-generated `BrowserScreenshotTool` type, so impact is UNKNOWN and the diff stays narrow.

## Implementation Checklist

- [x] Add screenshot save path helper tests.
- [x] Add optional path schema and tool path metadata.
- [x] Write decoded PNG to workspace-resolved path.
- [x] Register screenshot tool with the correct workspace root.
- [x] Run focused Rust tests.
- [x] Run diff and GitNexus change checks.
