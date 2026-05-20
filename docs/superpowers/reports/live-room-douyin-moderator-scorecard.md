# Live Room Douyin Moderator Scorecard

Status: fixture contract added for the first built-in automation/humane live-room moderator.

This scorecard is intentionally stricter than a DOM smoke test. The Douyin adapter must prove that it can run as a room manager, preserve per-room state, use only the configured room's gbrain namespace, execute real moderation actions with target verification, stop on live-ended or user-stop signals, and write a terminal report without leaking credential material.

| Assertion | Status |
| --- | --- |
| Comments scanned incrementally | fixture_contract |
| Room A and Room B cursors remain separate | fixture_contract |
| gbrain recall uses `live/douyin/{room_id}/` prefix | fixture_contract |
| gbrain writes use `live/douyin/{room_id}/` prefix | fixture_contract |
| Two warnings lead to mute | fixture_contract |
| Severe violation can remove | fixture_contract |
| Room ended signal auto-stops run | fixture_contract |
| User stop writes final report | fixture_contract |
| Final report includes counts and stop reason | fixture_contract |
| Auth material absent from trace | fixture_contract |

Verification:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::adapters::live_room::tests -- --nocapture
```

Next hardening step: connect the fixture trace to a deterministic browser page and the live-room executor, replacing fixture booleans with events emitted by `enter_room`, `scan_comments`, scoped gbrain calls, moderation actions, stop handling, and report persistence.
