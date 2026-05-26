# Lark-CLI Native Skills Integration & General IM Channel Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade uClaw's IM message-channel framework from notify-first Feishu webhook support to a global, policy-gated, skills-aware, lark-cli-powered IM ingress for the whole uClaw app. IM close-loop must be able to drive normal uClaw chat/session/runtime capabilities, desktop assist, future automation/spec runs, bidirectional Lark bot operation, Proma-grade queue/card/context behavior, and Agent OS projection/harness evidence.

**Architecture:** Keep uClaw's existing `src-tauri/src/channels/` framework as the legacy configured transport runtime, and extend the existing `src-tauri/src/im_channels/` adapter contract into the canonical global communication protocol: transport adapters emit a normalized `MessageFlowEnvelope`, the app routes it through a pluggable ingress router, and close-loop output returns through a `CloseLoopSink`. The execution kernel should be extracted from `send_agent_message` into an Agent session runner, not from `HeadlessDelegate` or the automation registry. Preserve current notify-only `feishu` semantics, add a separate bidirectional Lark/Feishu bot lane, model lark-cli after the managed Playwright CLI lane, and route external IM users through least-privilege policy gates, scoped queues, and auditable TaskEvents.

**Tech Stack:** Rust/Tauri backend, SQLite channel/session state, existing uClaw skills registry, managed built-in skills directories, official Go `lark-cli` from `/Users/ryanliu/Documents/cli`, Feishu/Lark IM event consume NDJSON, CardKit JSON, React settings UI, GitNexus impact/detect gates.

This document outlines the revised design, ROI evaluation, and detailed implementation plan to integrate the official Go-based **Lark/Feishu CLI** (`lark-cli`) into uClaw. It establishes a declarative, thin-lane child-process architecture, eliminating the need for any intermediate custom SDK or local MCP server wrappers.

It satisfies three distinct operational modes:
1. **Global IM Close-Loop Mode**: Treat external IM as an ingress into the normal uClaw app/session runtime, with policy-gated tools, skills, memory, provider routing, and IM reply/card sinks.
2. **IM Message Bot Mode (`--as bot`)**: Running a daemonized socket consumer (`lark-cli event consume`) to receive chat events, debouncing rapid bursts using a thread-safe `ScopedQueue`, and streaming progress indicators back via Interactive Cards.
3. **Desktop Assist Mode (`--as user`)**: Natively embedding `lark-cli`'s **26 pre-packaged AI Skills** (Calendar, Docs, Sheets, Wikis, Mail, Tasks, etc.) into a uClaw-managed built-in skills directory, modeled after the Playwright managed skills path. This allows uClaw's active desktop agent to execute controlled Lark commands (e.g. `lark-cli calendar +agenda --as user`) under the user's secure keychain context.

## Previous Round Summary + Current Research Addendum

The previous round produced the correct strategic direction: treat **lark-cli as the primary fast execution lane**, treat MCP as an optional ecosystem/discovery lane, and copy the Playwright design posture: managed built-in skills plus a controlled runtime adapter, not raw tool sprawl. This round adds four source-truth corrections from subagent comparison across uClaw, Proma, hello-halo, and `/Users/ryanliu/Documents/cli`.

1. **uClaw current truth**: `feishu` in `src-tauri/src/channels/types.rs` is currently a notify-only webhook sender implemented in `src-tauri/src/channels/notify/feishu.rs`. It is not a bidirectional Lark bot. New bidirectional work should add a distinct `lark_bot` or `feishu_bot` channel type and must not silently reuse notify-only `feishu` semantics.
2. **Proma truth**: Proma does not have a broad standalone `feishu_mcp` implementation. It has a Feishu Bot Bridge, a scoped dynamic `feishu_chat` MCP for group history, and a UI prompt that guides users toward `@larksuite/cli` plus lark skills. Therefore, "copy Proma's design" means copy its **IM runtime patterns**: `ScopedQueue`, `RunCoordinator`, CardKit run-state rendering, binding restore, attachment handling, and group-context prompt building.
3. **hello-halo truth**: hello-halo's strongest reusable idea is provider/instance separation and normalized `InboundMessage + ReplyHandle`. It proves that IM should be an external doorway into the same runtime, not a second chat system.
4. **lark-cli truth**: `/Users/ryanliu/Documents/cli` already ships 26 `skills/lark-*` skills, `event consume` NDJSON streaming, bot/user identity switching, OS keychain storage, `--dry-run`, `--format json|ndjson`, and exit-code `10` confirmation envelopes. This is enough to justify lark-cli as the higher-ROI default over a custom Feishu MCP wrapper.

## Granular Design Comparison

| Area | uClaw current codebase | Proma | hello-halo | lark-cli |
| --- | --- | --- | --- | --- |
| IM abstraction | Rust `ImChannelType`, `ImChannelInstanceConfig`, `InboundMessage`, `ReplyHandle`, `ImChannelSender` under `src-tauri/src/channels/`; lifecycle currently hard-codes WeCom/iLink bidirectional paths. | One large Feishu Bridge class with message parsing, binding, queueing, cards, attachments, and run orchestration. | Clean provider/instance contract in shared types; runtime manager diffs configs and routes normalized inbound messages. | Not an app-level IM framework; provides command/event primitives and Lark-native schemas. |
| Feishu/Lark support | Notify-only webhook with optional HMAC signing. | Full bot bridge via `createLarkChannel`, CardKit, bindings, group context, attachments, commands. | Feishu exists only as notification channel; no Feishu bot provider. | Full IM send/reply/search/history/resources plus event consume. |
| Message batching | No general `ScopedQueue` in the IM dispatcher; inbound messages dispatch one by one. | Strong `ScopedQueue` quiet-window batching and `RunCoordinator` per-scope serial/global concurrency. | Supplement buffer merges messages arriving while the agent is busy. | No batching policy; parent app owns orchestration. |
| Session model | SQLite `im_sessions` maps `(space_id, channel_type, chat_id)` to `agent_session_id`. | JSON binding files map `chatId` to session/workspace/channel/model. | IM session key reuses app chat namespace; JSONL persistence. | CLI has no uClaw session model. |
| Skills/tooling | Static/learned skills registry plus managed Playwright built-in skills; current IM chat path does not clearly inject lark skills. | User-facing prompt encourages installing `@larksuite/cli` and skills; not a managed built-in runtime. | Skills/MCP exist but not Lark-specific. | 26 ready `SKILL.md` packs; best source for uClaw managed lark skills. |
| Safety | Owners check exists, but `GuestPolicy.tool_allowlist` and `mcp_enabled` are not fully enforced in IM agent chat. | Remote IM uses `permissionModeOverride: bypassPermissions`, which is explicitly unsafe for uClaw. | Reply scope and file export gate are useful, but tool allowlist depends on JS SDK lists. | Strong CLI-side dry-run, typed errors, high-risk exit 10; uClaw still needs SafetyManager approval and audit. |
| Observability | Tauri status event and DB state exist; TaskEvent/projection integration is incomplete for IM. | Lots of logs and card state, but not structured trace/event taxonomy. | Status APIs and app session events; not uClaw projection-first. | Structured stdout/stderr envelopes; event consume ready/exited markers. |

## Decision: lark-cli over Feishu MCP for the Default Lane

Use **lark-cli as the primary execution and built-in skill lane**. Keep MCP as a secondary lane only for ecosystem interoperability or transient session tools such as `feishu_chat.fetch_group_chat_history`.

Reasons:

- lark-cli covers IM, Docs, Drive, Wiki, Sheets, Base, Mail, Calendar, Tasks, Approval, OKR, VC, Minutes, Attendance, Contact, and Apps through a maintained official CLI.
- It avoids token-heavy MCP tool schemas. uClaw can inject compact skill manifests and load full `SKILL.md` files only when relevant.
- It has OS keychain-backed credentials and bot/user identity semantics; uClaw does not need to persist raw Feishu secrets for desktop user workflows.
- It is scriptable like Playwright CLI and can run without model-token mediation for each API schema.
- It already defines an agent-suitable subprocess contract: JSON/NDJSON output, `--dry-run`, scope hints, event consume ready marker, stdin EOF shutdown, and exit code `10` for confirmation-required operations.

Counterweight risks:

- High-frequency event ingestion still needs a supervised daemon around `lark-cli event consume`.
- First-run auth and scope repair need a polished uClaw UI flow.
- CLI commands must be routed through a constrained `lark_cli` adapter; lark skills must not teach the agent to run arbitrary shell snippets.
- MCP remains useful for short-lived scoped tools, especially group-history fetch during a single IM run.

## Updated Implementation Plan

### Execution Checklist

- [x] Phase 0.1: Preserve the previous-round research summary and source-truth corrections in this active plan.
- [x] Phase 0.2: Confirm the current code truth with subagents: uClaw IM ingress, Proma Feishu bridge patterns, and lark-cli/skills ROI.
- [x] Phase 0.3: Record the decision that current `feishu` remains notify-only and future bidirectional support uses `lark_bot` / `feishu_bot`.
- [x] Phase 0.4: Run plan hygiene verification: `git diff --check -- docs/superpowers/plans/2026-05-26-feishu-kit-integration.md`.
- [ ] Phase 1.1: Run GitNexus impact before editing IM dispatcher symbols and record LOW/HIGH/CRITICAL blast radius.
- [x] Phase 1.2a: Extend `src-tauri/src/im_channels/` with pluggable global message-flow types: `MessageFlowEnvelope`, `MessageFlowOrigin`, `MessageFlowTarget`, `CloseLoopSink`, and `MessageCapabilityProfile`.
- [ ] Phase 1.2b: Extract the global Agent session runner from `send_agent_message` into `src-tauri/src/agent/session_runner.rs`.
- [ ] Phase 1.2c: Make desktop `send_agent_message` call the extracted runner as a behavior-preserving refactor.
- [ ] Phase 1.2d: Make IM inbound create a `MessageFlowEnvelope`, resolve an `agent_sessions` app-session target, and call the same runner with an IM close-loop sink.
- [ ] Phase 1.3: Enforce owner-vs-guest policy in that global communication route: owners can use the configured close-loop profile, guests receive only explicitly allowlisted capabilities/tools.
- [ ] Phase 1.4: Add `skill_search` and `load_skill` to global IM chat runs through the same skills manifest path used by desktop Agent chat.
- [ ] Phase 1.5: Keep `skill_write` and unrestricted MCP hidden from external IM users by default; honor `GuestPolicy.mcp_enabled=false` as no MCP exposure.
- [ ] Phase 1.6: Preserve IM reply handles and streaming/card callbacks as close-loop output sinks for the global session run.
- [ ] Phase 1.7: Harden Feishu/DingTalk/webhook URL validation across both `config_json` and `credentials_json`, including `config.webhook_url` and generic `config.url`.
- [ ] Phase 1.8: Add focused tests for global message-flow types, IM policy filtering, IM skills visibility, and URL validation.
- [ ] Phase 2.1: Add a managed `lark_skills` module modeled after `browser/playwright_skills.rs`.
- [ ] Phase 2.2: Seed/sync `/Users/ryanliu/Documents/cli/skills/lark-*` into `<uclaw_data_dir>/builtin-skills/lark-cli/`.
- [ ] Phase 2.3: Register managed Lark skills in `AppState::new` with `SkillProvenance::Bundled` before user/project skills.
- [ ] Phase 2.4: Add tests proving managed Lark skills are discovered and can be shadowed by user skills.
- [ ] Phase 3.1: Add a general `lark_cli` tool adapter that accepts argv arrays only.
- [ ] Phase 3.2: Enforce allowed domains, timeouts, redaction, structured JSON/NDJSON parsing, and exit-code `10` confirmation envelopes.
- [ ] Phase 3.3: Add dry-run smoke coverage using the local lark-cli binary without requiring live Feishu side effects.
- [ ] Phase 4.1: Add a separate bidirectional channel type, not a silent reinterpretation of notify-only `feishu`.
- [ ] Phase 4.2: Implement lark-cli event-consume process supervision, ready-marker handling, stdin EOF shutdown, and duplicate event suppression.
- [ ] Phase 4.3: Normalize `ImMessageReceiveOutput` into uClaw `InboundMessage` plus Lark-specific channel context.
- [ ] Phase 4.4: Route outbound replies through `lark-cli im +messages-reply` / `+messages-send`.
- [ ] Phase 5.1: Implement Proma-inspired `ScopedQueue` and `RunCoordinator` in Rust with per-scope serialization and global concurrency caps.
- [ ] Phase 5.2: Add bridge-aware prompt construction for group metadata, quoted messages, interactive-card context, and attachments.
- [ ] Phase 5.3: Add safe attachment ingest and FileExportGate-equivalent protection before uploads/sends.
- [ ] Phase 5.4: Add CardKit run-state reducer/renderer and throttled Feishu card updates with text fallback.
- [ ] Phase 6.1: Emit TaskEvent/World Projection entries for IM inbound/outbound, Lark CLI execution, confirmation, session update, and delivery status.
- [ ] Phase 6.2: Add scoped, read-only group-history helper lane that is visible only during the active IM run.
- [ ] Phase 6.3: Add Settings UI for Lark CLI readiness, bot status, auth state, skill-pack state, and provider priority.
- [ ] Phase 6.4: Run final backend/UI/harness verification plus `npx gitnexus detect-changes`.

### Phase 0 Self-Review

- Spec coverage: this plan keeps the user's requested cross-repo comparison, Proma reference design, lark-cli-vs-Feishu-MCP ROI decision, managed skills integration, general desktop tool adapter, and Phase 0-6 implementation path in one active artifact.
- Placeholder scan: no task may remain as "TBD" before implementation; uncertain production packaging choices must be captured as explicit review gates rather than vague placeholders.
- Type consistency: current `ImChannelType::Feishu` means notify-only webhook; bidirectional Lark/Feishu bot work must use a distinct type and separate runtime code path.
- Policy consistency: Proma's `bypassPermissions` is explicitly rejected; uClaw external IM defaults to least privilege, with owner bypass and guest allowlists handled in code. IM close-loop is a pluggable global communication protocol, not an automation-only escape hatch.
- Verification discipline: every phase lists focused verification, and code phases require GitNexus impact before symbol edits plus detect-changes before commit.

### Phase 0 - Plan and Source-Truth Cleanup

- Keep this plan as the active implementation plan.
- Explicitly record that current `feishu` is webhook-only and future bidirectional bot support uses `lark_bot` / `feishu_bot`.
- Before code edits, run GitNexus impact on changed symbols as required by `AGENTS.md`; note that one subagent saw the GitNexus index stale, so refresh with `npx gitnexus analyze` if the tool warns.
- Verification: `git diff --check`.

### Phase 1 - Harden Current IM Baseline

- Replace the current automation-biased IM agent-chat path with a global communication boundary: an inbound IM message becomes a normalized `MessageFlowEnvelope`, resolves an app session, and runs through the same Agent session capability stack as desktop uClaw wherever possible.
- Extract that capability stack from `send_agent_message` into `src-tauri/src/agent/session_runner.rs`. The existing `HeadlessDelegate` remains a transition-only automation runner, not the future global close-loop kernel.
- Keep automation trigger phrases as one explicit routed target, but do not make automation `HeadlessDelegate` the default close-loop for normal IM chat.
- Enforce `GuestPolicy.tool_allowlist` and `mcp_enabled` in the global IM route. Owner bypass stays; guests default to least privilege.
- Inject `skill_search` / `load_skill` and Skills XML into global IM chat, but do not expose `skill_write` or unrestricted MCP to external IM users by default.
- Preserve the IM output sink: normal app-session runs must still deliver final text/streaming/card updates back through `ReplyHandle` / future rich Feishu handles.
- Harden Feishu/DingTalk webhook URL validation so both `config_json` and `credentials_json` are checked.
- Verification:
  - `cargo test --manifest-path src-tauri/Cargo.toml channels::dispatcher --lib`
  - `cargo test --manifest-path src-tauri/Cargo.toml channels::notify::feishu --lib`
  - `cargo test --manifest-path src-tauri/Cargo.toml im_channels --lib`

### Global Communication Protocol

The pluggable protocol has five stable pieces:

- `Transport Adapter`: platform-specific connector such as Lark, WeCom, Slack, email, or local desktop. It owns auth, receive/send mechanics, and platform schemas.
- `MessageFlowEnvelope`: transport-neutral user/system message with origin, target, sink, sender, metadata, and capability profile.
- `Ingress Router`: resolves the envelope into an app-session run, automation trigger, notification-only dispatch, or future workflow target.
- `Capability Profile`: compact allow/deny surface for tools, MCP, skills, write-capable skills, and future capability cards.
- `CloseLoopSink`: output target for final reply, stream, card update, desktop event, or no-op audit flow.

This is the target shape for easy plugability. Feishu/Lark is the first high-value adapter, but the abstraction must also fit WeCom, Slack, email, desktop chat, automation escalation, and future plugin-defined transports.

### Phase 2 - Managed Lark Skills Pack

- Add a `lark_skills` module modeled after `src-tauri/src/browser/playwright_skills.rs`.
- Seed/sync official `/Users/ryanliu/Documents/cli/skills/lark-*` into a uClaw-managed built-in skills directory such as `<uclaw_data_dir>/builtin-skills/lark-cli/`.
- Register that directory as `SkillProvenance::Bundled` during `AppState::new`, after static bundled skills and before user skills so user forks can shadow it.
- Prefer managed sync over copying all 26 skills into repo root `skills/feishu/` for production; repo `skills/` can remain a dev fallback.
- Verification:
  - `cargo test --manifest-path src-tauri/Cargo.toml browser::playwright_skills skills --lib` plus new `lark_skills` tests.
  - Manual startup log confirms lark skills are discovered.

### Phase 3 - General `lark_cli` Tool Adapter

- Add a built-in tool or capability adapter that executes `lark-cli` by argv array only, never by shell string.
- Cap domains and risks through policy: start with `auth`, `event`, `im`, `calendar`, `docs`, `drive`, `wiki`, `sheets`, `task`, `mail`, `approval`.
- Parse stdout JSON envelopes and stderr typed errors. Map exit `10` to uClaw approval UI / IM confirmation cards, then retry with `--yes` only after explicit approval.
- Redact secrets, URLs with tokens, app secrets, access tokens, and user-provided credential env vars from logs and TaskEvents.
- Verification:
  - Unit tests for argv construction, timeout, redaction, JSON parsing, non-JSON Cobra errors, and exit `10`.
  - Dry-run smoke: `/Users/ryanliu/Documents/cli/lark-cli im +messages-send --chat-id oc_xxx --text hello --dry-run`.

### Phase 4 - Lark Bot IM Adapter

- Add `ImChannelType::LarkBot` or `FeishuBot`; keep existing `Feishu` as webhook notify.
- Add `src-tauri/src/channels/im/feishu_cli.rs` for `lark-cli event consume im.message.receive_v1 --as bot` process supervision.
- Add `feishu_queue.rs` with Proma-inspired `ScopedQueue` and `RunCoordinator`: per `chat_id` quiet-window batching, run-time block, post-run quiet-window flush, max global concurrency.
- Normalize CLI `ImMessageReceiveOutput` into uClaw `InboundMessage`, including `event_id`, `message_id`, `chat_id`, `chat_type`, `sender_id`, `content`, and thread/reply context where available.
- Route replies through `lark-cli im +messages-reply` or `+messages-send`; keep interactive cards optional until CardKit rendering is stable.
- Verification:
  - Unit tests for NDJSON parsing, ready marker, EOF/SIGTERM shutdown, duplicate event/message suppression, batching, and error recovery.
  - `cargo test --manifest-path src-tauri/Cargo.toml channels:: --lib`.

### Phase 5 - Proma-Grade Cards, Context, and Attachments

- Port Proma concepts, not the giant class: `RunState` reducer, CardKit renderer, throttled card updates, quoted-message context, attachment download/save, and group metadata.
- Render mobile progress as card updates when possible; behind NAT or callback-limited setups fall back to text commands such as `/stop`.
- Add a FileExportGate equivalent before any lark-cli file upload/send command.
- Verification:
  - Card JSON snapshot tests.
  - Attachment path traversal and symlink tests.
  - Manual QA: private chat, group @Bot, burst messages, image/file, quoted reply, `/stop`, restart binding restore.

### Phase 6 - Projection, Harness, and Rollout

- Emit TaskEvent/World Projection entries for `ImInboundReceived`, `ImOutboundDispatched`, `LarkCliExecuted`, `LarkCliConfirmationRequired`, `ImSessionUpdated`, and `DeliveryStatus`.
- Add a transient MCP lane only for scoped helpers like `feishu_chat.fetch_group_chat_history`; it must be session-scoped, read-only, and hidden outside the active IM run.
- Add Settings UI for Lark CLI readiness, bot event status, auth status, skill-pack state, and provider priority, following Browser Runtime Control Center patterns.
- Verification:
  - Rust integration harness: fake event consume process -> IM dispatcher -> headless run -> reply/card update.
  - UI tests: `cd ui && npm test -- ImChannelsSettings ImChannelAccordionRow BrowserRuntimeSettings`.
  - Final gate: `npx gitnexus detect-changes` before commit.

---

## Technical Decision: Why We Skip a Local MCP Server

During our evaluation, we compared a local stdio JSON-RPC MCP Server (`feishu_mcp`) against **Direct Shell-Execution via Built-In Skills** (like uClaw's Playwright integration) and found that **Direct Shell-Execution is superior in every dimension**:

1. **Redundancy of JSON-RPC Wrapper**: uClaw's desktop agent is already a shell-enabled agent. Exposing CLI commands through an intermediate MCP server over stdio adds an unnecessary JSON-RPC serialization, deserialization, and schema-routing layer.
2. **Pre-packaged AI Skills (High ROI)**: `lark-cli` natively ships with **26 highly-optimized AI Agent Skills** (folders of Markdown instructions like `SKILL.md` under `skills/`). Syncing them into uClaw's managed built-in skills directory allows uClaw to auto-scan and load them at boot while still letting user-owned skills shadow them.
3. **Keychain State Security**: `lark-cli` manages credentials using OS-native secure keychains (macOS Keychain, DPAPI on Windows). Direct shell execution delegates security management to `lark-cli` out-of-the-box, keeping uClaw's codebase clean and secure.
4. **Interactive Flows (Exit Code 10)**: High-risk operations (e.g. deleting files) exit with **Exit Code 10** and stderr JSON detailing the safety prompt. The shell agent can catch this, request natural-language user confirmation, and re-run with `--yes`. Replicating this state machine over an MCP protocol is extremely complex.

---

## ROI Comparison: Custom TS SDK vs. Lark-CLI Native Bridge

| Dimension | Proma's Custom TS SDK | Lark-CLI Native Bridge | ROI Winner & Rationale |
| :--- | :--- | :--- | :--- |
| **Development & Maintenance Cost** | **High**: Manually write wrappers for every Feishu Open API, handle multi-part file uploads, and token refresh. | **Near Zero**: Taps into a pre-existing, fully-tested Go binary covering 18 domains and 200+ commands. | **Lark-CLI**: Zero SDK maintenance. Lark's API updates are absorbed by upgrading the Go CLI. |
| **Token Cost (LLM Efficiency)** | **High**: Exposing granular REST APIs to the LLM consumes massive reasoning/context tokens. | **Zero**: High-level Shortcuts starting with `+` (e.g. `+agenda`, `+create`) bundle complex operations into a single command. | **Lark-CLI**: Massive token savings. The agent reads high-level Skill descriptions and runs short CLI command strings. |
| **Security & Credential Vaulting** | **Custom/Challenging**: Requires building safe encrypted storage on disk or binding Electron's main-process `safeStorage` to protect secrets. | **Built-in & Native**: Seamlessly leverages OS-native Keychains (macOS Keychain, Windows DPAPI, Linux Secret Service). | **Lark-CLI**: Fully secure. uClaw never holds or persists raw Secrets on the disk. |
| **Identity Versatility** | **Limited**: Strictly runs under **Bot Identity** (tenant token), preventing access to user personal calendars, docs, or mail. | **Dual-Identity**: Fluidly supports both **Bot** (`--as bot`) and **User** (`--as user`) contexts on demand. | **Lark-CLI**: Unlocks Desktop Assist mode, turning uClaw into a daily personal desk copilot. |
| **Event Gateway Robustness** | **Custom**: Manually manage WebSocket handshakes, ping/pong heartbeats, and reconnection backoffs. | **Production-grade Daemon**: Spawns `lark-cli event consume` which manages the daemon and automatically terminates via stdin EOF. | **Lark-CLI**: Prevents background orphan processes. Cleanly terminates when uClaw closes. |

---

## User Review Required

We have identified several product and architectural decisions that require your explicit approval:

> [!IMPORTANT]
> **1. Lark-CLI Binary Packaging & Path Resolution**
> During development, uClaw will execute the compiled `lark-cli` directly from your local directory `/Users/ryanliu/Documents/cli/lark-cli`. For production packaging, we propose bundling the precompiled platform-specific Go binary inside Tauri's resource directory (`tauri.conf.json` -> `resources`), resolving it dynamically via `tauri::api::path::resolve_path`. Please confirm if this is acceptable.
>
> **2. Managed Lark Skills Pack**
> We will sync the 26 pre-packaged skills from `/Users/ryanliu/Documents/cli/skills/` into a uClaw-managed built-in skills directory such as `<uclaw_data_dir>/builtin-skills/lark-cli/`, following the Playwright managed skills model. The repo-local `skills/` tree can remain a development fallback, but production should not depend on copying all upstream lark skills into `skills/feishu/`.
>
> **3. High-Risk Write Confirmations (Exit Code 10)**
> We propose that uClaw's Rust executor intercepts `exit 10` on any subprocess execution, extracts the risk detail, and renders an interactive warning dialog in the Tauri UI (or a confirmation card in IM chat). Only when the user clicks "Approve" will uClaw retry the command with `--yes` appended.
>
> **4. NAT Limitation for Interactive Card Buttons**
> Clicking buttons on Feishu Interactive Cards requires a public HTTP callback. Since uClaw runs locally behind NAT, button clicks will not trigger callback endpoints.
> We will handle this gracefully like Proma:
> - Display helper text: *"Behind NAT: to stop, please reply with `/stop`"*.
> - The WebSocket event daemon will stream `/stop` as an inbound message, which our `dispatcher.rs` will process instantly.

---

## ADR §18 Strategic Alignment (11 Questions)

1. **What user intent does this support?**
   It supports running local agent workflows remotely via their mobile/desktop Feishu IM messenger, and allowing local desktop agents to operate their daily office tools (Calendar, Docs, Sheets, Wikis, approvals) directly.
2. **What autonomy level can it run at?**
   It operates at **Autonomy Level 3 (Supervised Interaction)** for high-risk write operations (utilizing the `exit 10` confirmation dialog gate) and **Level 4 (Autonomous Execution)** for read/non-destructive write tasks.
3. **What is the canonical truth source?**
   The canonical truth source for credentials is the **OS-native Keychain** managed by `lark-cli`. The truth source for chat bindings and local session states is uClaw's SQLite database (`im_channel_instances` and `im_sessions` tables).
4. **What TaskEvent entries does it emit?**
   - `ImInboundReceived`: Emitted when the stdout reader parses a message from `lark-cli event consume`.
   - `ImOutboundDispatched`: Emitted when `lark-cli im +messages-reply` or `+messages-send` completes successfully.
   - `LarkCliExecuted`: Emitted on every subprocess invocation, capturing the exact argv (redacting secrets), execution duration, and exit status.
5. **What context does it read, and how is it cited?**
   It reads the active `agent_session` conversational context. When a user replies to a card in Feishu, uClaw executes `lark-cli im +messages-mget` or parses the parent message context from the event payload.
6. **What capability cards does it add or consume?**
   - Adds `LarkCliBotCapability`: Background process lifecycle supervisor for event consumption.
   - Adds `LarkCliUserCapability`: Loads the managed `builtin-skills/lark-cli/` markdown files to uClaw's system prompt loader for Desktop Assist mode.
7. **What policy hooks can block it?**
   - `ImPermissionGate`: Rejects messages from unauthorized guest accounts.
   - `LarkWriteSafetyHook`: Blocks the execution of commands with high-risk writes unless explicitly confirmed.
8. **What world projection does the UI render?**
   The Tauri settings panel renders real-time status of the connection (Online, Connecting, Config Missing, Offline), the authorized identity profile (User/Bot Name, Tenant ID), and a button to trigger QR-code based auth login.
9. **What harness cases prove it works?**
   - Simulating `lark-cli event consume` stdout lines to test Rust NDJSON stream parsing.
   - Mocking an `exit 10` subprocess return to verify that uClaw's confirmation dialog trigger works.
   - Testing the `ScopedQueue` message aggregator with multiple rapid simulated event inputs.
10. **What is the rollback or disable path?**
    Disabling the new Lark/Feishu bot channel sends a SIGTERM to the `lark-cli` child process, which closes the socket connection instantly. Deleting the channel cleans up local SQLite bindings. Existing notify-only `feishu` webhook instances remain independent.
11. **What does it deliberately not own?**
    It deliberately does **not** own credentials storage, token refreshing, or network-level socket keep-alives. It delegates all of this to the compiled `lark-cli` Go binary.

---

## Proposed Changes

We replace our previous custom SDK design and MCP server wrappers with the **LarkCliProvider Native Skills Integration & Subprocess Bridge**.

```
[ uClaw Rust Tauri Backend ]
  |
  +--- [ builtin-skills/lark-cli/ ] (NEW: managed 26 Skill Directories with SKILL.md and references)
  |
  +--- [ src-tauri/src/channels/ ]
  |       |
  |       +--- manager.rs         (Integrate LarkCliProvider activation/deactivation)
  |       |
  |       +--- dispatcher.rs      (Process inbound messages debounced by ScopedQueue)
  |       |
  |       +--- types.rs           (CardKit 2.0 Rust structures & cli capabilities)
  |
  +--- [ src-tauri/src/channels/im/ ]
          |
          +--- feishu_cli.rs      (NEW: Subprocess exec, Event Consumer, Auth QR code)
          |
          +--- feishu_queue.rs    (NEW: Tokio ScopedQueue & RunCoordinator)
          |
          +--- feishu_card.rs     (NEW: State-to-CardKit-JSON converter for CLI replies)
```

---

### Component A: Managed Built-in Skills Catalog

We will integrate `lark-cli`'s pre-packaged AI skills into uClaw's native built-in skills directory.

#### [NEW] `src-tauri/src/lark_skills.rs`
- **Changes**: Sync the 26 skills from `/Users/ryanliu/Documents/cli/skills/` into `<uclaw_data_dir>/builtin-skills/lark-cli/` (e.g., `lark-calendar/SKILL.md`, `lark-doc/SKILL.md`, etc.).
- **Mechanism**: Mirror `browser/playwright_skills.rs`: seed the managed skills pack, register the managed directory as `SkillProvenance::Bundled` during `AppState::new`, and allow user skills to shadow managed lark skills.
- **Agent Behavior**: The active desktop agent reads these skills and can construct controlled `lark-cli` tool requests. The skills should instruct the agent to call the `lark_cli` adapter, not arbitrary shell.

---

### Component B: Core IM Framework Upgrades

We will upgrade uClaw's active channel manager and dispatcher to support the subprocess bridge.

#### [MODIFY] [manager.rs](file:///Users/ryanliu/Documents/uclaw/src-tauri/src/channels/manager.rs)
- **Changes**: Integrate `feishu_cli::LarkCliProvider` into `ImChannelManager` under a new bidirectional `lark_bot` / `feishu_bot` channel type.
- When a Lark/Feishu bot channel instance is started:
  - Verify configuration via `lark-cli auth status`. If configured, spawn the event consumer background task: `lark-cli event consume im.message.receive_v1 --quiet --as bot`.
  - Save the child process handle (to allow clean termination).
- When the instance is stopped:
  - Close the stdin pipe of the event consumer child process. This triggers `lark-cli` to exit cleanly via stdin EOF. If it doesn't exit within 2 seconds, send `SIGTERM`.

#### [MODIFY] [dispatcher.rs](file:///Users/ryanliu/Documents/uclaw/src-tauri/src/channels/dispatcher.rs)
- **Changes**: Route inbound messages from `LarkCliProvider` through the `ScopedQueue` debouncer on a per `chat_id` basis.
- Messages received within the 600ms quiet window are merged into a single turn and routed to the corresponding agent session, preventing race conditions and context pollution.

#### [MODIFY] [types.rs](file:///Users/ryanliu/Documents/uclaw/src-tauri/src/channels/types.rs)
- Define Rust structs mapping `ImMessageReceiveOutput` (from `lark-cli`'s flattened schema).
- Add support for custom execution errors, mapping `exit 10` to `LarkCliError::ConfirmationRequired`.

---

### Component C: Lark-CLI Subprocess Bridge (feishu_cli)

We will build the thin-lane Go subprocess bridge under `src-tauri/src/channels/im/`.

#### [NEW] [feishu_cli.rs](file:///Users/ryanliu/Documents/uclaw/src-tauri/src/channels/im/feishu_cli.rs)
The heart of the `LarkCliProvider`.
- **Subprocess Executor (`execute_lark_cli`)**:
  - Thread-safe asynchronous executor that takes a slice of arguments `&[&str]` and runs the compiled binary (`/Users/ryanliu/Documents/cli/lark-cli`).
  - Automatically appends `--json` or `--format json` to ensure structured stdout.
  - Redacts sensitive credential tokens from log files.
  - If `exit code == 10`, parses the stderr JSON and returns `LarkCliError::ConfirmationRequired(action, level, hint)`.
- **WebSocket Event Consumer Daemon**:
  - Background task that spawns `lark-cli event consume im.message.receive_v1 --quiet`.
  - Reads `stdout` line-by-line. Each line is an NDJSON payload representing a received message.
  - Decodes each line into `ImMessageReceiveOutput`.
  - Resolves `@Bot` mentions and triggers the `ImInboundReceived` pipeline.
- **QR Code Auth Orchestration**:
  - Implements `LarkCliProvider::auth_login_flow()`:
    1. Spawns `lark-cli auth login --recommend --no-wait --json` in the background.
    2. Parses stdout to extract `device_code` and `verification_url`.
    3. Executes `lark-cli auth qrcode --output <temp_path>` to generate a PNG QR-code.
    4. Triggers a Tauri frontend event emitting the image path and verification URL.
    5. Runs a background loop polling `lark-cli auth login --device-code <device_code>` every 2 seconds. When it returns success, triggers a success event in the UI.

#### [NEW] [feishu_queue.rs](file:///Users/ryanliu/Documents/uclaw/src-tauri/src/channels/im/feishu_queue.rs)
- Keeps the exact debouncer & coordinator logic for IM execution to maintain parity with Proma's high-reliability architecture:
  - **`ScopedQueue`**: Merges messages arriving within a 600ms window on a per `chat_id` basis.
  - **`RunCoordinator`**: Implements thread-safe locking and caps global concurrency to a maximum of 3 concurrent agent runs.

#### [NEW] [feishu_card.rs](file:///Users/ryanliu/Documents/uclaw/src-tauri/src/channels/im/feishu_card.rs)
- **`CardRunState`**: Translates agent step-by-step thinking logs, running tools, and completed tools into Feishu Interactive Card JSON schemas.
- Incorporates the protection limits: collapses completed tools when the count exceeds 3, protecting against the 30KB payload limit.
- Pushes card updates to Feishu by calling:
  `lark-cli im +messages-reply --message-id <om_xxx> --content '<JSON_CARD>' --msg-type interactive`

---

## Verification Plan

Every code change must pass strict automated validation and manual verification before merging.

### Automated Tests
Run the following commands to verify backend stability and subprocess integration:
```bash
# Verify lark-cli subprocess execution and exit code handling
cargo test -p uclaw --lib channels::im::feishu_cli::tests::test_subprocess_execution

# Verify NDJSON event stream parsing from stdout
cargo test -p uclaw --lib channels::im::feishu_cli::tests::test_ndjson_event_stream

# Verify ScopedQueue message batching and merging
cargo test -p uclaw --lib channels::im::feishu_queue::tests::test_scoped_queue

# Verify CardRenderer state-to-card-JSON formatting
cargo test -p uclaw --lib channels::im::feishu_card::tests::test_card_rendering
```

### Manual Verification
1. **Built-in Skills Auto-Discovery**:
   - Start uClaw in development mode.
   - Verify in the startup logs: `Discovered 26 skill(s) at startup` (plus writing and ux skills).
   - In uClaw's desktop chat, ask: *"How do I search the calendar?"*.
   - Verify that the agent references `lark-calendar`'s specific directives and rules correctly.
2. **Desktop Copilot Execution Test**:
   - In uClaw's desktop chat, ask: *"Show my schedule for today"*.
   - Verify that the agent executes the terminal command `lark-cli calendar +agenda --as user` in the background.
3. **QR Code Auth Login Flow**:
   - Open uClaw, click "Feishu Auth Login".
   - Verify that the Tauri window displays a beautiful QR code and verification URL.
   - Scan the QR code with your mobile Feishu app and approve.
   - Verify that the uClaw UI instantly transitions to "Online", showing your personal profile name.
4. **Interactive IM Bot Test**:
   - Open Feishu on your phone, send a message to the bot.
   - Rapidly type three messages in under 600ms.
   - Verify that the messages are merged, the bot replies with an Interactive Progress Card, and collapses the reasoning steps when completed.
5. **Safety Confirmation Test (Exit Code 10)**:
   - Ask the desktop agent to delete a document.
   - Verify that the subprocess intercepts `exit 10`, renders a warning dialog in uClaw's UI, and does *not* execute the deletion until you click "Approve".
