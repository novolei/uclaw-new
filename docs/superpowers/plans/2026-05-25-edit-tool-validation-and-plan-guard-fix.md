# Plan: Fix Edit Tool Parameter Validation and Expand Plan-Aware Continuation Guard

This plan outlines the systematic fix for Edit tool parameters validation errors and unsolicited loop interruptions during complex agent planning workflows.

## Proposed Changes

### Backend

#### [MODIFY] [edit.rs](file:///Users/ryanliu/Documents/uclaw/src-tauri/src/agent/tools/builtin/edit.rs)

- Refactor `execute_single_file`'s signature to take a deserialized `Vec<EditArg>` rather than a raw `serde_json::Value`.
- Update parameter schema to declare `old_text` as an optional property by removing it from the `"required"` parameters lists.
- Update `execute` to deserialize parameters using Serde directly.

#### [MODIFY] [dispatcher.rs](file:///Users/ryanliu/Documents/uclaw/src-tauri/src/agent/dispatcher.rs)

- Add highly common Chinese/English action-intent verbs in `text_signals_plan_work`'s keyword checklist to ensure transitions like `"升级按钮样式："` keep the plan execution guard active.

## Verification Plan

### Automated Tests
- Run `CARGO_TARGET_DIR=target/test_dir cargo test -p uclaw -- agent::tools::builtin::edit`
- Run `CARGO_TARGET_DIR=target/test_dir cargo test -p uclaw -- agent::dispatcher`
