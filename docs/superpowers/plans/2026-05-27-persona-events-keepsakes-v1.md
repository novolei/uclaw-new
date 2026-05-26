# Persona Events and Keepsakes v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build PR1 + PR2 for Living Persona: a deterministic persona event ledger and a user-confirmed keepsake proposal flow that powers the relationship timeline.

**Architecture:** Keep relationship growth as explicit, inspectable data under `agent::persona`, not as hidden prompt mutation. Events aggregate into affinity factors; keepsakes move through a visible proposed/accepted/hidden/discarded lifecycle. The UI reads relationship state through Tauri IPC and can accept or hide proposed keepsakes, while prompt rendering remains style-only by default.

**Tech Stack:** Rust/Tauri v2, SQLite via `rusqlite`, Serde DTOs, React 18 + TypeScript, Settings primitives, Vitest, Rust unit tests.

---

## Current Context

- Existing MVP module: `src-tauri/src/agent/persona/`.
- Existing schema V53 already has persona profiles, journal, keepsakes, badges, and evolution candidates.
- This plan adds V54 for `persona_events`, because event ledger deserves an append-only table rather than overloading `persona_journal_entries`.
- Existing `persona_keepsakes` is reused for PR2.
- Relationship data must not alter model routing, tools, permissions, safety policy, or capability.

## File Structure

- Modify `CONTEXT.md`
  - Claim V54 in the migration registry.
- Modify `src-tauri/src/db/migrations.rs`
  - Add `V54_PERSONA_EVENTS` migration and tests.
- Modify `src-tauri/src/agent/persona/types.rs`
  - Add `PersonaEventKind`, `PersonaEvent`, `RecordPersonaEventInput`, `ProposePersonaKeepsakeInput`, `UpdatePersonaKeepsakeStatusInput`, and `PersonaRelationshipTimeline`.
- Modify `src-tauri/src/agent/persona/store.rs`
  - Add event recording/listing, affinity factor aggregation, keepsake proposal/list/status update, and timeline loading.
- Modify `src-tauri/src/agent/persona/mod.rs`
  - Export the new public types.
- Modify `src-tauri/src/tauri_commands.rs`
  - Add thin IPC shims for timeline, event recording, keepsake proposal, and keepsake status update.
- Modify `src-tauri/src/main.rs`
  - Register new Tauri commands.
- Modify `ui/src/lib/persona-types.ts`
  - Mirror new timeline/event/keepsake DTOs.
- Modify `ui/src/lib/persona.ts`
  - Add invoke helpers.
- Modify `ui/src/components/settings/PersonaBondTimeline.tsx`
  - Replace placeholder timeline with real loaded data and accept/hide actions.
- Modify `ui/src/components/settings/PersonaBondTimeline.test.tsx`
  - Cover loaded affinity, proposed keepsake rendering, and accept action.

---

## Task 1: V54 Persona Event Ledger Schema

**Files:**
- Modify: `CONTEXT.md`
- Modify: `src-tauri/src/db/migrations.rs`

- [ ] **Step 1: Run GitNexus impact**

Run:

```bash
npx gitnexus impact migrations --repo uclaw-new --direction upstream
```

Expected: report risk. Stop for HIGH/CRITICAL.

- [ ] **Step 2: Add V54 registry entry**

Add this row after V53 in `CONTEXT.md`:

```markdown
| V54 | persona_events — append-only Living Persona event ledger | in progress |
```

- [ ] **Step 3: Add migration constant**

Add `V54_PERSONA_EVENTS` after `V53_LIVING_PERSONA`:

```rust
pub const V54_PERSONA_EVENTS: &str = "
CREATE TABLE IF NOT EXISTS persona_events (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    session_id TEXT,
    task_id TEXT,
    minutes INTEGER NOT NULL DEFAULT 0,
    weight INTEGER NOT NULL DEFAULT 1,
    note TEXT,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_persona_events_kind ON persona_events(kind);
CREATE INDEX IF NOT EXISTS idx_persona_events_session ON persona_events(session_id);
CREATE INDEX IF NOT EXISTS idx_persona_events_created ON persona_events(created_at);
";
```

- [ ] **Step 4: Wire V54 into `run`**

Add a V54 block after V53:

```rust
tracing::debug!("Running migration V54: persona events");
for stmt in V54_PERSONA_EVENTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
    if let Err(e) = conn.execute(stmt, []) {
        tracing::warn!("V54 stmt skipped: {} :: {}", e, stmt);
    }
}
```

- [ ] **Step 5: Add migration tests**

Add tests that assert the table/indexes exist and invalid event kinds are not blocked by schema. Event-kind validation belongs in Rust, so schema remains append-friendly.

Run:

```bash
cd src-tauri && cargo test v54_persona_events -- --nocapture
```

Expected: V54 tests pass.

- [ ] **Step 6: Commit Task 1**

```bash
git add CONTEXT.md src-tauri/src/db/migrations.rs
git commit -m "feat(persona): add event ledger schema" -m "Verification:
- cd src-tauri && cargo test v54_persona_events -- --nocapture
  Expected: V54 persona event schema tests pass."
```

---

## Task 2: Persona Event Types and Store Aggregation

**Files:**
- Modify: `src-tauri/src/agent/persona/types.rs`
- Modify: `src-tauri/src/agent/persona/store.rs`
- Modify: `src-tauri/src/agent/persona/affinity.rs`
- Modify: `src-tauri/src/agent/persona/mod.rs`

- [ ] **Step 1: Run GitNexus impact**

Run:

```bash
npx gitnexus impact PersonaStore --repo uclaw-new --direction upstream || true
npx gitnexus impact calculate_affinity --repo uclaw-new --direction upstream
```

Expected: report risk. If the new `PersonaStore` symbol is not indexed yet, note it and continue with narrow module-local edits.

- [ ] **Step 2: Add event DTOs**

Add Rust types:

```rust
pub enum PersonaEventKind {
    CollaborationMinutes,
    TaskSucceeded,
    PositiveFeedback,
    StylePreferenceAccepted,
    FailureRecovered,
    InactivityDecay,
    KeepsakeAccepted,
    CandidateRejected,
    FailureUnresolved,
    Correction,
}

pub struct PersonaEvent {
    pub id: String,
    pub kind: PersonaEventKind,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub minutes: i64,
    pub weight: i64,
    pub note: Option<String>,
    pub evidence: Vec<String>,
    pub created_at: String,
}

pub struct RecordPersonaEventInput {
    pub kind: PersonaEventKind,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub minutes: i64,
    pub weight: i64,
    pub note: Option<String>,
    pub evidence: Vec<String>,
}
```

Each type uses `#[serde(rename_all = "camelCase")]`; `PersonaEventKind` uses `#[serde(rename_all = "snake_case")]`.

- [ ] **Step 3: Add store methods**

Implement:

```rust
pub fn record_event(&self, input: &RecordPersonaEventInput) -> rusqlite::Result<PersonaEvent>
pub fn list_recent_events(&self, limit: i64) -> rusqlite::Result<Vec<PersonaEvent>>
pub fn affinity_factors_from_events(&self) -> rusqlite::Result<AffinityFactors>
```

Aggregation maps:

- `CollaborationMinutes` -> `successful_minutes += minutes`
- `TaskSucceeded` -> `successful_minutes += 30 * weight`
- `PositiveFeedback` -> `positive_feedback += weight`
- `StylePreferenceAccepted` -> `stable_style_fragments += weight`
- `FailureRecovered` -> `recovered_failures += weight`
- `InactivityDecay` -> `inactivity_days += weight`
- `KeepsakeAccepted` -> `accepted_keepsakes += weight`
- `CandidateRejected` -> `rejected_candidates += weight`
- `FailureUnresolved` -> `unresolved_failures += weight`
- `Correction` -> `correction_count += weight`

- [ ] **Step 4: Add tests**

Add tests in `store.rs`:

```rust
#[test]
fn events_aggregate_into_affinity_factors()
```

Run:

```bash
cd src-tauri && cargo test agent::persona::store::tests::events_aggregate -- --nocapture
```

Expected: store event aggregation test passes.

- [ ] **Step 5: Commit Task 2**

```bash
git add src-tauri/src/agent/persona
git commit -m "feat(persona): aggregate relationship events" -m "Verification:
- cd src-tauri && cargo test agent::persona::store::tests::events_aggregate -- --nocapture
  Expected: event aggregation test passes."
```

---

## Task 3: Keepsake Proposal Lifecycle Store

**Files:**
- Modify: `src-tauri/src/agent/persona/types.rs`
- Modify: `src-tauri/src/agent/persona/store.rs`

- [ ] **Step 1: Add keepsake DTOs**

Add:

```rust
pub struct ProposePersonaKeepsakeInput {
    pub title: String,
    pub narrative: String,
    pub learned_text: Option<String>,
    pub evidence: Vec<String>,
}

pub enum PersonaKeepsakeStatus {
    Proposed,
    Accepted,
    Hidden,
    Discarded,
}

pub struct UpdatePersonaKeepsakeStatusInput {
    pub id: String,
    pub status: PersonaKeepsakeStatus,
}
```

- [ ] **Step 2: Add store methods**

Implement:

```rust
pub fn propose_keepsake(&self, input: &ProposePersonaKeepsakeInput) -> rusqlite::Result<PersonaKeepsake>
pub fn update_keepsake_status(&self, input: &UpdatePersonaKeepsakeStatusInput) -> rusqlite::Result<PersonaKeepsake>
pub fn list_keepsakes(&self) -> rusqlite::Result<Vec<PersonaKeepsake>>
```

When status becomes `Accepted`, call `record_event` with `PersonaEventKind::KeepsakeAccepted`, `weight = 1`, and evidence containing the keepsake id.

- [ ] **Step 3: Add tests**

Add tests:

```rust
#[test]
fn accepting_keepsake_records_affinity_event()
```

Run:

```bash
cd src-tauri && cargo test agent::persona::store::tests::accepting_keepsake -- --nocapture
```

Expected: accepted keepsake appears in factors as `accepted_keepsakes = 1`.

- [ ] **Step 4: Commit Task 3**

```bash
git add src-tauri/src/agent/persona
git commit -m "feat(persona): add keepsake proposal lifecycle" -m "Verification:
- cd src-tauri && cargo test agent::persona::store::tests::accepting_keepsake -- --nocapture
  Expected: accepted keepsake records affinity event."
```

---

## Task 4: Relationship Timeline IPC

**Files:**
- Modify: `src-tauri/src/agent/persona/types.rs`
- Modify: `src-tauri/src/agent/persona/store.rs`
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Run GitNexus impact**

Run:

```bash
npx gitnexus impact get_persona_config --repo uclaw-new --direction upstream
npx gitnexus impact main --repo uclaw-new --direction upstream
```

Expected: report risk. Stop for HIGH/CRITICAL.

- [ ] **Step 2: Add timeline DTO**

Add:

```rust
pub struct PersonaRelationshipTimeline {
    pub affinity: RelationshipAffinity,
    pub factors: AffinityFactors,
    pub keepsakes: Vec<PersonaKeepsake>,
    pub recent_events: Vec<PersonaEvent>,
}
```

- [ ] **Step 3: Add store timeline loader**

Implement:

```rust
pub fn relationship_timeline(&self) -> rusqlite::Result<PersonaRelationshipTimeline>
```

It uses `affinity_factors_from_events`, `calculate_affinity`, `list_keepsakes`, and `list_recent_events(20)`.

- [ ] **Step 4: Add Tauri commands**

Add:

```rust
pub async fn get_persona_relationship_timeline(...)
pub async fn record_persona_event(...)
pub async fn propose_persona_keepsake(...)
pub async fn update_persona_keepsake_status(...)
```

Register all four in `main.rs::invoke_handler!`.

- [ ] **Step 5: Verify command path compiles**

Run:

```bash
cd src-tauri && cargo check
```

Expected: no compiler errors.

- [ ] **Step 6: Commit Task 4**

```bash
git add src-tauri/src/agent/persona src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(persona): expose relationship timeline ipc" -m "Verification:
- cd src-tauri && cargo check
  Expected: no compiler errors."
```

---

## Task 5: Relationship Timeline UI

**Files:**
- Modify: `ui/src/lib/persona-types.ts`
- Modify: `ui/src/lib/persona.ts`
- Modify: `ui/src/components/settings/PersonaBondTimeline.tsx`
- Modify: `ui/src/components/settings/PersonaBondTimeline.test.tsx`

- [ ] **Step 1: Add TypeScript DTOs and bridge methods**

Mirror Rust DTOs in `persona-types.ts` and add:

```ts
export async function getPersonaRelationshipTimeline(): Promise<PersonaRelationshipTimeline>
export async function proposePersonaKeepsake(input: ProposePersonaKeepsakeInput): Promise<PersonaRelationshipTimeline>
export async function updatePersonaKeepsakeStatus(input: UpdatePersonaKeepsakeStatusInput): Promise<PersonaRelationshipTimeline>
```

- [ ] **Step 2: Replace placeholder UI**

`PersonaBondTimeline` should:

- load `getPersonaRelationshipTimeline()` on mount;
- show affinity score and explanations;
- render proposed and accepted keepsakes;
- show `接受` and `隐藏` controls for proposed keepsakes;
- keep the boundary copy: relationship rewards do not change Agent capability.

- [ ] **Step 3: Update Vitest**

Mock `@/lib/persona` and assert:

- score is rendered;
- proposed keepsake title is rendered;
- clicking `接受` calls `updatePersonaKeepsakeStatus`.

Run:

```bash
cd ui && npm test -- --run src/components/settings/PersonaBondTimeline.test.tsx
```

Expected: timeline UI test passes.

- [ ] **Step 4: Commit Task 5**

```bash
git add ui/src/lib/persona-types.ts ui/src/lib/persona.ts ui/src/components/settings/PersonaBondTimeline.tsx ui/src/components/settings/PersonaBondTimeline.test.tsx
git commit -m "feat(persona): show relationship timeline data" -m "Verification:
- cd ui && npm test -- --run src/components/settings/PersonaBondTimeline.test.tsx
  Expected: relationship timeline UI test passes."
```

---

## Task 6: Final Verification

**Files:**
- No new files unless fixing verification failures.

- [ ] **Step 1: Run focused Rust verification**

```bash
cd src-tauri
cargo test v54_persona_events -- --nocapture
cargo test agent::persona -- --nocapture
cargo check
```

Expected: tests pass and cargo check has no compiler errors.

- [ ] **Step 2: Run focused UI verification**

```bash
cd ui
npm test -- --run src/components/settings/PersonaStudio.test.tsx src/components/settings/PersonaBondTimeline.test.tsx src/components/settings/IntelligenceTab.test.tsx
```

Expected: tests pass. Existing `GeneEvolutionSection act(...)` warning is acceptable if tests pass.

- [ ] **Step 3: Diff hygiene and GitNexus detect**

```bash
git diff --check
npx gitnexus detect-changes --repo uclaw-new
```

Expected: `git diff --check` has no output; GitNexus reports no unexpected HIGH/CRITICAL risk.

---
