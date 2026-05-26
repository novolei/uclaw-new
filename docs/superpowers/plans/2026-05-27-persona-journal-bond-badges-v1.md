# Persona Journal Bond Badges v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the next Living Persona MVP slices: inner journal, editable bond profile, and cosmetic badge/settings support on top of the merged event ledger and keepsake timeline.

**Architecture:** Keep persona state inspectable and user-controlled inside `agent::persona`. Journal entries are explicit records, bond profile updates are visible edits or promoted observations, and badges are derived cosmetic artifacts with hide controls. None of this changes model routing, tool access, permission mode, safety policy, memory policy, or verification requirements.

**Tech Stack:** Rust/Tauri v2, SQLite via `rusqlite`, Serde DTOs, React 18 + TypeScript, Settings primitives, Vitest, Rust unit tests.

---

## Current Context

- PR #549 merged V54 `persona_events`, keepsake lifecycle, relationship timeline IPC, and a first relationship UI.
- V53 already created `persona_bond_profiles`, `persona_journal_entries`, and `persona_badges`; this plan reuses those tables without adding a new migration.
- `PersonaStore` is the correct backend boundary for journal, bond, settings, and badge behavior.
- Relationship gamification must be user-visible and optional; disabling it hides score/badges but does not delete underlying events.

## File Structure

- Modify `src-tauri/src/agent/persona/types.rs`
  - Add journal DTOs, bond update DTOs, relationship settings DTOs, and richer badge/timeline DTOs.
- Modify `src-tauri/src/agent/persona/store.rs`
  - Add journal CRUD, bond profile load/save/promotion, settings key helpers, badge derivation/list/hide, and timeline expansion.
- Modify `src-tauri/src/agent/persona/mod.rs`
  - Export new public DTOs.
- Modify `src-tauri/src/tauri_commands.rs`
  - Add thin IPC shims that mutate persona state and return the refreshed timeline.
- Modify `src-tauri/src/main.rs`
  - Register new Tauri commands.
- Modify `ui/src/lib/persona-types.ts`
  - Mirror new Rust DTOs.
- Modify `ui/src/lib/persona.ts`
  - Add invoke helpers.
- Modify `ui/src/components/settings/PersonaBondTimeline.tsx`
  - Show journal entries, bond profile, badge list, and relationship gamification toggle.
- Modify `ui/src/components/settings/PersonaBondTimeline.test.tsx`
  - Cover journal creation, bond promotion, badge hide, and settings toggle.

---

## Task 1: Plan and Baseline

**Files:**
- Create: `docs/superpowers/plans/2026-05-27-persona-journal-bond-badges-v1.md`

- [ ] **Step 1: Verify isolated worktree**

Run:

```bash
git status --short --branch
git log --oneline --max-count=1
```

Expected: branch is `codex/persona-journal-bond-badges-v1`, based on merge commit `4b07da67`.

- [ ] **Step 2: Run baseline persona tests**

Run:

```bash
cd src-tauri && cargo test agent::persona -- --nocapture
```

Expected: existing persona tests pass.

- [ ] **Step 3: Commit plan**

```bash
git add docs/superpowers/plans/2026-05-27-persona-journal-bond-badges-v1.md
git commit -m "docs(plan): persona journal bond badges v1" -m "Verification:
- cd src-tauri && cargo test agent::persona -- --nocapture
  Expected: existing persona tests pass."
```

---

## Task 2: Journal, Bond, and Badge Store

**Files:**
- Modify: `src-tauri/src/agent/persona/types.rs`
- Modify: `src-tauri/src/agent/persona/store.rs`
- Modify: `src-tauri/src/agent/persona/mod.rs`

- [ ] **Step 1: Run GitNexus impact**

Run:

```bash
npx gitnexus impact 'Impl:src-tauri/src/agent/persona/store.rs:PersonaStore' --repo uclaw-new --direction upstream
npx gitnexus impact calculate_affinity --repo uclaw-new --direction upstream
```

Expected: LOW risk or stale-index warning with narrow persona-module edits.

- [ ] **Step 2: Add Rust DTOs**

Add:

```rust
pub enum PersonaJournalConfidence { Low, Medium, High }
pub struct PersonaJournalEntry { ... }
pub struct CreatePersonaJournalEntryInput { ... }
pub enum PersonaBondField { CollaborationRhythm, ChallengeContract, SupportStyle, CommunicationDislikes }
pub struct PromotePersonaJournalEntryInput { pub id: String, pub field: PersonaBondField }
pub struct PersonaRelationshipSettings { pub gamification_enabled: bool }
pub struct UpdatePersonaRelationshipSettingsInput { pub gamification_enabled: bool }
pub struct UpdatePersonaBadgeVisibilityInput { pub badge_key: String, pub hidden: bool }
```

Extend `PersonaBadge` with `id`, `evidence`, and `awarded_at`; extend `PersonaRelationshipTimeline` with `bond`, `journal_entries`, `badges`, and `settings`.

- [ ] **Step 3: Add store methods**

Implement:

```rust
get_global_bond_profile
upsert_global_bond_profile
create_journal_entry
list_journal_entries
delete_journal_entry
promote_journal_entry
relationship_settings
update_relationship_settings
list_badges
update_badge_visibility
```

Promotion appends the journal observation to the selected bond field, marks `promoted_at`, and records a `StylePreferenceAccepted` event once.

- [ ] **Step 4: Add tests**

Add tests:

```rust
journal_entries_can_promote_into_bond_profile
badge_derivation_respects_hidden_state
relationship_settings_round_trip
```

Run:

```bash
cd src-tauri && cargo test agent::persona::store::tests::journal_entries_can_promote -- --nocapture
cd src-tauri && cargo test agent::persona::store::tests::badge_derivation -- --nocapture
cd src-tauri && cargo test agent::persona::store::tests::relationship_settings -- --nocapture
```

Expected: all three tests pass.

- [ ] **Step 5: Commit Task 2**

```bash
git add src-tauri/src/agent/persona
git commit -m "feat(persona): add journal bond and badge store" -m "Verification:
- cd src-tauri && cargo test agent::persona::store::tests::journal_entries_can_promote -- --nocapture
- cd src-tauri && cargo test agent::persona::store::tests::badge_derivation -- --nocapture
- cd src-tauri && cargo test agent::persona::store::tests::relationship_settings -- --nocapture
  Expected: journal, bond, badge, and settings store tests pass."
```

---

## Task 3: Persona Timeline IPC

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Run GitNexus impact**

Run:

```bash
npx gitnexus impact 'Function:src-tauri/src/main.rs:main' --repo uclaw-new --direction upstream
```

Expected: LOW risk or stale-index warning.

- [ ] **Step 2: Add Tauri commands**

Add commands:

```rust
create_persona_journal_entry
delete_persona_journal_entry
promote_persona_journal_entry
update_persona_bond_profile
update_persona_relationship_settings
update_persona_badge_visibility
```

Each command mutates through `PersonaStore` and returns `PersonaRelationshipTimeline` where useful.

- [ ] **Step 3: Register commands**

Add the six new commands next to the existing Persona commands in `src-tauri/src/main.rs`.

- [ ] **Step 4: Verify compile**

Run:

```bash
cd src-tauri && cargo check
```

Expected: no compiler errors.

- [ ] **Step 5: Commit Task 3**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(persona): expose journal bond badge ipc" -m "Verification:
- cd src-tauri && cargo check
  Expected: command registration compiles."
```

---

## Task 4: Settings Timeline UI

**Files:**
- Modify: `ui/src/lib/persona-types.ts`
- Modify: `ui/src/lib/persona.ts`
- Modify: `ui/src/components/settings/PersonaBondTimeline.tsx`
- Modify: `ui/src/components/settings/PersonaBondTimeline.test.tsx`

- [ ] **Step 1: Add TypeScript DTOs and bridge helpers**

Mirror the new Rust DTOs and add invoke helpers for journal creation/deletion/promotion, bond profile update, relationship setting update, and badge visibility.

- [ ] **Step 2: Expand `PersonaBondTimeline`**

Render:

- relationship gamification toggle;
- bond profile lists;
- inner journal entries with promote/delete actions;
- cosmetic badges with hide action;
- existing keepsake and affinity sections.

- [ ] **Step 3: Update Vitest**

Mock `@/lib/persona` and assert:

- journal entry appears and can be promoted;
- badge hide calls `updatePersonaBadgeVisibility`;
- disabling relationship gamification calls `updatePersonaRelationshipSettings`;
- boundary copy still states that capability is unchanged.

Run:

```bash
cd ui && npm test -- --run src/components/settings/PersonaBondTimeline.test.tsx
```

Expected: test passes.

- [ ] **Step 4: Commit Task 4**

```bash
git add ui/src/lib/persona-types.ts ui/src/lib/persona.ts ui/src/components/settings/PersonaBondTimeline.tsx ui/src/components/settings/PersonaBondTimeline.test.tsx
git commit -m "feat(persona): render journal bond badges timeline" -m "Verification:
- cd ui && npm test -- --run src/components/settings/PersonaBondTimeline.test.tsx
  Expected: expanded timeline UI test passes."
```

---

## Task 5: Final Verification

**Files:**
- No new files unless fixing verification failures.

- [ ] **Step 1: Rust verification**

```bash
cd src-tauri
cargo test agent::persona -- --nocapture
cargo check
```

Expected: tests pass and cargo check succeeds.

- [ ] **Step 2: UI verification**

```bash
cd ui
npm test -- --run src/components/settings/PersonaStudio.test.tsx src/components/settings/PersonaBondTimeline.test.tsx src/components/settings/IntelligenceTab.test.tsx
```

Expected: tests pass. Existing `GeneEvolutionSection act(...)` warning is acceptable if tests pass.

- [ ] **Step 3: Diff hygiene and GitNexus**

```bash
git diff --check
npx gitnexus detect-changes --repo uclaw-new
```

Expected: no whitespace errors; no unexpected HIGH/CRITICAL GitNexus risk.
