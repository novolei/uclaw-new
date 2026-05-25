# Browser Runtime Phase 6I - Payment Confirmation Harness

## Phase Goal

Close the ADR Phase 6 payment-confirmation gate with explicit harness evidence.
The runtime already detects payment boundaries and routes human-boundary prompts
through the agent `ask_user` bridge. This phase makes that requirement visible
in the Browser parity harness so payment-sensitive actions cannot be scored as
covered unless the run records both a `payment` boundary and an
`ask_user_response` confirmation.

## ADR 11 Questions

1. User intent:
   Prevent browser automation from continuing through payment-sensitive actions
   without a visible user confirmation boundary.

2. Autonomy level:
   L1/L2 only. Detection and scorecard evidence are automatic, but the payment
   continuation decision is represented as a user `ask_user` response.

3. Canonical truth source:
   Browser task run steps remain canonical: `needs_user_intervention` records
   the payment boundary and `ask_user_response` records the confirmation.

4. TaskEvent entries:
   This phase adds no TaskEvent writes. It verifies existing task-step evidence
   and harness scorecards.

5. Context read and citation:
   Reads only task-step action names and redacted action args such as boundary
   kind and confirmation decision. It does not expose billing, card, cookie, or
   profile secret data.

6. Capability cards:
   No provider card changes. This is harness evidence over the existing browser
   boundary/ask-user behavior.

7. Policy hooks:
   Payment boundaries remain policy-sensitive and must have `ask_user`
   confirmation evidence before the harness can pass the payment case.

8. World projection:
   No UI change. Existing ask-user projection stays the user-visible surface.

9. Harness cases:
   Adds `browser.payment.confirmation` and scorecard checks for payment boundary
   precision plus `payment_confirmation`.

10. Rollback or disable path:
   Revert this PR. It removes only the harness case/checks and tracker notes;
   runtime payment-boundary detection remains unchanged.

11. Deliberately not owned:
   No payment UI, no checkout execution, no billing-data handling, no provider
   promotion, no DB migration, no hosted provider, no task-loop rewrite, and no
   production side effects.

## Allowed Files

- `src-tauri/src/harness/adapters/browser.rs`
- `src-tauri/src/harness/cases/browser/payment-confirmation.json`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-25-browser-runtime-phase6i-payment-confirmation-harness.md`

## Non-Goals

- Do not implement a new payment confirmation UI.
- Do not change provider routing, action execution, or browser policy.
- Do not touch `agentic_loop.rs`, `tauri_commands.rs`, migrations, or Settings.
- Do not add real payment, network purchase, or hosted-provider behavior.

## Impact Targets

- `BUILTIN_BROWSER_PARITY_CASES` in `src-tauri/src/harness/adapters/browser.rs`.
- `score_browser_run` in `src-tauri/src/harness/adapters/browser.rs`.
- Browser parity adapter tests in the same file.

## Rollback

Revert this PR. No migrations, runtime pack changes, provider selection changes,
or user data changes are involved.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib harness::adapters::browser`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::boundary`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/harness/adapters/browser.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
