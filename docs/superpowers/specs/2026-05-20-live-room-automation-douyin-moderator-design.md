# Live Room Automation Contract + Douyin Moderator Design

> Status: design approved in-chat through section review; written for user review on 2026-05-20.
> Next: `superpowers:writing-plans` after user review.

## Goal

Build the first uClaw built-in automation spec for live-room operations: a long-running AI room moderator that can enter a live room, monitor new comments every 30 seconds, answer selected questions using gbrain, learn useful knowledge back into gbrain, and perform real moderation actions such as warning, muting, and removing users.

Douyin is the first platform adapter, but the runtime must not become Douyin-specific. The durable product surface is a generic live-room automation contract:

- `enter_room`
- `scan_comments`
- `send_reply`
- `warn_user`
- `mute_user`
- `remove_user`
- `extract_knowledge`

The first built-in spec is `Douyin Live Moderator`, implemented on top of that contract.

## Current Code Truth

The browser layer is close enough to support this product, but automation cannot use it yet.

- `src-tauri/src/browser/tools.rs` already exposes browser navigation, DOM, screenshot, extraction, click, type, select, scroll, keys, evaluate, cookies, tabs, session control, `browser_task`, `browser_task_resume`, and `retry_with_browser_agent`.
- `browser_task` accepts `auth_profile_id` and `auth_origin`, so it can run with an authorized browser identity profile.
- `src-tauri/src/browser/agent_loop.rs` already has an observe-decide-act loop, boundary detection, checkpointing, and `ask_user` intervention.
- `src-tauri/src/browser/memory_adapter.rs` already records browser task events into the memory system and can write browser-task pages to gbrain through `put_page`.
- `src-tauri/src/gbrain/browse.rs` exposes typed gbrain browsing and `put_page`.
- `src-tauri/src/automation/runtime/service.rs::build_automation_tool_registry` currently excludes browser tools from headless automation runs. This is the hard blocker for a spec-only implementation.

Therefore the first delivery must include a small automation runtime capability bridge before the built-in spec can run.

## Reference: Halo Digital Human Pattern

The implementation should copy Halo's Digital Human pattern at the architecture level, not blindly port Electron code.

Halo's useful ideas:

- `browser_login` declares which sites the user should log into.
- Login happens in a real browser window using the same persistent browser session as the AI browser.
- Automation runs in a scoped browser context so it does not steal the user's visible browser state.
- AI browser tools are injected only when the spec declares the browser permission.
- `browser_run` executes reviewed JavaScript files in the page context, with cookies and localStorage available naturally through the browser session.
- Script paths are restricted to safe roots such as the working directory and skill directories.
- Specs must not define cookie, session token, password, or verification-code config fields.

uClaw's version should map these ideas to the existing Rust/Tauri browser stack:

- Use `BrowserIdentityProfile` / storage state as the standard identity object.
- Add a Halo-style login window and status flow around browser identity profiles.
- Add a `browser_run_script` tool for reviewed script execution in the page context.
- Use `BrowserContextManager` scoped sessions for automation.
- Keep secrets out of specs, traces, and gbrain pages.

## Architecture

### 1. Automation Runtime Capability Bridge

Automation runs must be able to register browser and gbrain tools when a spec declares the relevant capability.

Add a browser-capable registry path parallel to the interactive chat registry:

- If `permissions` contains `ai_browser`, register the browser tool family.
- Register `browser_task`, `browser_task_resume`, and `retry_with_browser_agent` with:
  - `BrowserContextManager`
  - `LlmBrowserDecisionAdapter`
  - `BrowserTaskStore`
  - `BrowserAskUserBridge`
  - `BrowserLongTermMemoryAdapter`
- Add `browser_run_script`, modeled after Halo's `browser_run`.
- If gbrain is connected or declared, expose narrow gbrain tools:
  - `gbrain_search`
  - `gbrain_get_page`
  - `gbrain_put_page`

The bridge must preserve automation permission checks. Browser tools require `Permission::AiBrowser`. gbrain writes require an explicit memory/knowledge capability in the spec or a documented built-in exception for this first shipped spec.

### 2. Browser Identity and Login

`browser_login` becomes first-class in uClaw automation.

For the Douyin spec:

```yaml
browser_login:
  - url: https://www.douyin.com/
    label: Douyin
```

Runtime behavior:

- The Automation UI shows a login notice for specs with `browser_login`.
- The user can open a standalone login window.
- The login window uses uClaw browser identity/profile infrastructure, not spec config secrets.
- The user completes QR/password/SMS/CAPTCHA login manually when required.
- The resulting browser storage state is associated with a `BrowserIdentityProfile`.
- The automation uses `auth_profile_id` or `auth_origin = "https://www.douyin.com"` when launching browser work.
- If the profile is stale, the run pauses as `needs_login` or `waiting_user`, emits a boundary event, and resumes after the user restores login.

Rules:

- Do not store Douyin password, SMS code, QR token, cookies, or raw localStorage in the spec.
- Do not write secrets into activity, diagnostics, browser task memory, or gbrain.
- Challenge/CAPTCHA behavior is detect, ask user, checkpoint, resume. The default path does not bypass third-party anti-abuse systems.

### 3. `browser_run_script`

Add a uClaw browser tool that executes reviewed JavaScript in the active page context.

Purpose:

- Let platform adapters use stable scripts instead of asking the model to rediscover DOM/API behavior every 30 seconds.
- Allow page-context `fetch(..., { credentials: "include" })`, DOM reads, and platform UI actions using the current browser session.
- Keep scripts auditable and testable.

Tool shape:

```json
{
  "file": "adapters/live/douyin/scan_comments.js",
  "params": {
    "cursor": "...",
    "limit": 100
  },
  "timeout_ms": 30000
}
```

Path policy:

- Allowed: built-in uClaw adapter directory.
- Allowed: spec/workspace script directory.
- Future allowed: installed skill directories.
- Denied: arbitrary absolute paths outside approved roots.

Execution policy:

- Max timeout: 120 seconds.
- Default timeout: 30 to 60 seconds depending on operation.
- Result size limit with structured truncation.
- Every script call writes trace metadata: file, adapter, operation, duration, success, error kind.
- Script output must be JSON-serializable.

### 4. Live Room Adapter Contract

Create a platform-neutral contract. The first adapter is `douyin`.

#### `enter_room`

Inputs:

- `platform`
- `live_url`
- `auth_profile_id` or `auth_origin`

Outputs:

- `room_id`
- `room_title`
- `host_name`
- `tab_id`
- `status`: `entered | login_required | room_not_live | blocked | failed`

#### `scan_comments`

Inputs:

- `room_id`
- `cursor`
- `limit`

Outputs:

- `next_cursor`
- `comments`

Comment shape:

```json
{
  "platform": "douyin",
  "platform_comment_id": "...",
  "author_id": "...",
  "author_name": "...",
  "text": "...",
  "timestamp_ms": 0,
  "badges": [],
  "is_new": true
}
```

#### `send_reply`

Sends a short room comment as the logged-in moderator account.

Inputs:

- `room_id`
- `text`
- optional `reply_to_comment_id`

#### `warn_user`

Warns a user publicly or through the available platform affordance.

Inputs:

- `author_id`
- `author_name`
- `reason`
- `evidence_comment_ids`

#### `mute_user`

Mutes a user using room-manager permissions.

Inputs:

- `author_id`
- `reason`
- `duration`
- `evidence_comment_ids`

#### `remove_user`

Removes a user from the room using room-manager permissions.

Inputs:

- `author_id`
- `reason`
- `evidence_comment_ids`

#### `extract_knowledge`

Converts comments into candidate knowledge records.

Outputs:

- `facts`
- `faqs`
- `feedback`
- `moderation_notes`

The adapter only extracts candidates. The automation policy decides what to write to gbrain.

## Douyin Adapter v1

Douyin v1 should be script-first, browser-task fallback.

Primary path:

- Use `browser_run_script` scripts under a built-in adapter directory.
- Scripts operate in the live room page context.
- Scripts return structured JSON for comments and action results.

Fallback path:

- If a script fails due to UI changes or missing APIs, call `browser_task` with a constrained task prompt and max step count.
- If browser task reaches a human boundary, checkpoint and ask the user.

Adapter responsibilities:

- Normalize Douyin-specific IDs into contract fields.
- Maintain comment cursor and duplicate filtering.
- Re-verify a moderation target immediately before `mute_user` or `remove_user`.
- Detect room-not-live, login-required, insufficient-permission, and action-denied states.

The adapter must not make policy decisions such as whether a comment deserves punishment. It only reports observations and executes commands.

## Built-In Spec: Douyin Live Moderator

### Config

```yaml
config_schema:
  - key: live_url
    label: Live room URL
    type: url
    required: true
  - key: poll_interval_seconds
    label: Poll interval
    type: number
    default: 30
  - key: moderator_role
    label: Moderator role
    type: text
    default: "You are the logged-in room moderator assistant."
  - key: atmosphere_reply_rate
    label: Atmosphere reply rate
    type: number
    default: 0.08
  - key: max_replies_per_minute
    label: Max replies per minute
    type: number
    default: 3
  - key: spam_window_seconds
    label: Spam detection window
    type: number
    default: 60
  - key: spam_threshold
    label: Repeated comment threshold
    type: number
    default: 5
  - key: punishment_rate_limit_per_5m
    label: Punishment rate limit
    type: number
    default: 5
  - key: remove_user_enabled
    label: Allow removing users
    type: boolean
    default: true
  - key: knowledge_scope
    label: Knowledge scope
    type: select
    default: room_only
    options:
      - label: Current room only
        value: room_only
      - label: Current room plus platform shared knowledge
        value: room_plus_platform
```

### Permissions

```yaml
permissions:
  - ai_browser
  - notification
```

Add a gbrain/memory capability if the protocol gains one before implementation.

### Requires

```yaml
requires:
  mcps:
    - id: gbrain
      reason: Search and update the room knowledge base.
```

### Run Model

This spec is a long-running live run, not a fresh scheduled run every 30 seconds.

Lifecycle:

1. User starts the automation.
2. Runtime enters the room with the Douyin adapter.
3. Runtime stores `room_id`, `tab_id`, and `comment_cursor` in run memory.
4. Runtime repeats every `poll_interval_seconds`:
   - scan comments
   - classify comments
   - answer selected questions
   - update gbrain
   - execute moderation actions
   - write trace
5. Run stops when the user stops it, the room ends, login becomes stale, permissions are insufficient, or a fatal adapter error occurs.

## Moderation Policy

The user explicitly wants first-version moderation to execute real room-manager actions by default.

Therefore the spec must implement hard safety gates that do not require user confirmation but reduce accidental punishment.

### Default Actions

- First violation: warn the user.
- Continued violation after two warnings: mute the user.
- Severe violation: mute immediately, and remove if configured.
- Continued violation after mute or severe platform abuse: remove user if `remove_user_enabled = true`.

### Violation Types

- `spam_repeated`: repeated or near-duplicate comments inside the spam window.
- `sensitive`: platform-sensitive or configured sensitive content.
- `abusive`: insults, harassment, threats, or attacks.
- `negative_disruption`: repeated negative disruption aimed at derailing the room.
- `scam_or_external_link`: suspected scam, phishing, or unsafe external promotion.

### Hard Gates

Before `mute_user` or `remove_user`:

- The adapter must re-read the target by stable `author_id` or equivalent platform identity.
- If only a DOM index is available and identity cannot be verified, do not punish.
- Never punish host, moderator, configured whitelist users, or platform staff badges.
- Low-confidence classifications only log and optionally reply; they do not punish.
- The run enforces `punishment_rate_limit_per_5m`.
- Every real punishment must include evidence comment IDs/text in trace.

### Warning Count

The moderation ledger stores warning counts per `platform + room_id + author_id`.

Two warnings are counted only when:

- They target the same stable author ID.
- The later violation occurs after the warning.
- The violation type is the same or compatible, such as repeated spam plus repeated disruption.

## Reply Policy

The logged-in account acts as a room moderator/assistant, not as the host.

Rules:

- Reply first to clear questions.
- Use gbrain before answering factual questions whenever possible.
- Keep replies short and live-room-native.
- Do not answer every comment.
- Respect `max_replies_per_minute`.
- Atmosphere replies are optional and sampled by `atmosphere_reply_rate`.
- Do not invent prices, inventory, medical/legal/financial facts, or host commitments.
- If gbrain/page context is insufficient, say a concise uncertainty-safe response.

## gbrain Integration

The spec uses gbrain for both recall and learning.

### Scope Isolation

The live-room moderator must not expose the full global gbrain knowledge base to the room agent. Knowledge access is scoped by platform and room.

Scope keys:

- `platform`: `douyin`
- `room_id`: stable platform room ID when available
- `host_id`: stable host/account ID when available
- `knowledge_scope`: `room_only` by default, or `room_plus_platform` when the user explicitly enables shared platform knowledge

Read policy:

- Default recall searches only the current room namespace: `live/douyin/<room_id>/...`.
- If `knowledge_scope = room_plus_platform`, recall may also search `live/douyin/shared/...`.
- The agent must never receive an unrestricted `gbrain_search` result set across the whole brain.
- The tool layer should enforce scoped query filters or slug prefixes, not rely only on prompt wording.
- Cross-room recall is disabled unless a later product setting explicitly maps rooms into a shared campaign/brand scope.

Write policy:

- Room-specific facts, FAQ, feedback, and moderation records write only under the current room prefix.
- Platform-wide reusable rules may write under `live/douyin/shared/...` only when the spec or user config marks the knowledge as platform-shared.
- Moderation ledger is always room-scoped and must not be promoted to global/shared knowledge.

Implementation requirement:

- Add scoped gbrain automation helpers such as `gbrain_room_search`, `gbrain_room_get_page`, and `gbrain_room_put_page`, or add required `scope`/`slug_prefix` parameters to the narrow gbrain tools.
- The built-in Douyin spec should use the room-scoped helpers exclusively.
- Direct unscoped `gbrain_search` and arbitrary `gbrain_get_page` should not be exposed to this automation run.

### Recall

For question-like comments:

1. Build a search query from the comment and room context.
2. Call scoped room recall, e.g. `gbrain_room_search` with `platform = "douyin"` and `room_id`.
3. Optionally call scoped page read for the strongest hits.
4. Generate a short reply grounded in returned knowledge.

### Writeback

Only write durable knowledge, not every comment.

Write categories:

- FAQ: repeated questions and stable answers.
- Feedback: user objections, pain points, product feedback.
- Room facts: host-confirmed facts, activity rules, price/process facts visible in the room.
- Moderation ledger: warning and punishment records.

Suggested slugs:

- `live/douyin/<room_id>/faq/<topic>`
- `live/douyin/<room_id>/feedback/<yyyy-mm-dd>`
- `live/douyin/<room_id>/facts/<topic>`
- `live/douyin/<room_id>/moderation/<yyyy-mm-dd>`
- `live/douyin/shared/<topic>` only for explicitly platform-shared knowledge

Avoid storing:

- Raw cookies, tokens, localStorage, QR data, phone numbers, SMS codes, passwords.
- Full raw comment firehoses.
- Unverified rumors as facts.

## Observability

Every tick should produce a compact trace:

```json
{
  "tick_id": "...",
  "room_id": "...",
  "comments_seen": 42,
  "questions_answered": 3,
  "atmosphere_replies": 1,
  "knowledge_candidates": 4,
  "gbrain_writes": 2,
  "warnings": 1,
  "mutes": 1,
  "removals": 0,
  "errors": []
}
```

Every real moderation action must produce an auditable action trace:

```json
{
  "action": "mute_user",
  "platform": "douyin",
  "room_id": "...",
  "author_id": "...",
  "author_name": "...",
  "reason": "spam_repeated_after_two_warnings",
  "evidence_comments": ["...", "..."],
  "warning_count": 2,
  "adapter_result": "success",
  "timestamp": "..."
}
```

Trace should be visible in Automation activity and available to the Agent view for run replay.

## UI

Automation detail page:

- Shows `browser_login` notice for Douyin.
- Lets the user open the login window.
- Shows identity status: `live`, `stale`, or `unknown`.
- Shows current room status: entering, live, room ended, login required, insufficient permissions, blocked, failed.
- Shows live tick summary and recent moderation actions.
- Provides a stop button for the long-running live run.

Agent/Browser panel:

- Browser tab remains visible when the run uses browser tools.
- User can inspect the live controlled browser when needed.
- `ask_user` prompts for login/CAPTCHA/checkpoint flow appear in the normal tool-activity stream.

## Implementation Scope

### In Scope

- Browser-enabled automation tool registry.
- `browser_run_script`.
- Browser login/profile UX sufficient for Douyin.
- Generic live-room adapter contract.
- Douyin adapter v1.
- Built-in Douyin Live Moderator spec.
- gbrain recall and writeback tools for automation.
- Long-running 30-second tick loop.
- Real default moderation actions with hard gates.
- Activity trace and moderation ledger.

### Deferred

- Other live platforms.
- Full marketplace packaging for third-party live adapters.
- Automatic CAPTCHA solving.
- Password or SMS-code storage.
- Perfect Douyin API stability guarantees.
- Multi-room concurrent moderation for one spec instance.
- Human approval queue for moderation, since first version intentionally defaults to real execution.

## Testing

### Rust Unit Tests

- `automation` registry includes browser tools when `Permission::AiBrowser` is declared.
- `automation` registry does not include browser tools without browser permission.
- `browser_run_script` accepts allowed paths and rejects arbitrary absolute paths.
- `browser_run_script` times out and truncates large results safely.
- gbrain automation tools map to `search`, `get_page`, and `put_page`.
- room-scoped gbrain tools enforce platform/room slug prefixes and reject unscoped searches.
- live-room moderation policy increments warnings and escalates to mute after two warnings.
- moderation policy never punishes whitelisted users.
- moderation policy refuses punishment when target identity cannot be re-verified.
- rate limit blocks excess punishments.

### Adapter Tests

- Douyin `scan_comments` normalizes fixture JSON/DOM into contract comments.
- Cursor dedup prevents repeat processing.
- `send_reply` returns success/error with stable error kinds.
- `mute_user` and `remove_user` require stable `author_id`.

### Frontend Tests

- Automation detail shows `browser_login` status and login action.
- Long-running run surface shows tick summary.
- Moderation action trace renders without exposing secrets.

### Harness / Smoke

- Fake live-room fixture page with comments, question replies, spam user, and severe user.
- Run the built-in spec against the fixture.
- Assert:
  - room entered
  - comments scanned incrementally
  - gbrain queried for a question
  - gbrain recall is limited to the current room namespace by default
  - useful knowledge written
  - room-specific writes land under `live/douyin/<room_id>/...`
  - two warnings lead to mute
  - severe case can remove when enabled
  - no raw auth material appears in trace or gbrain page content

## Rollout Plan

1. Bridge automation to browser/gbrain tools.
2. Add `browser_run_script`.
3. Add login/profile UX parity slice.
4. Add live-room contract and fixture adapter.
5. Add Douyin adapter v1.
6. Add Douyin Live Moderator built-in spec.
7. Add harness fixture and scorecard.
8. Run real Douyin smoke with a controlled room/moderator account before enabling broad use.
