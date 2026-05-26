# Living Persona MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Living Persona MVP from `docs/superpowers/specs/2026-05-26-living-persona-mvp-design.md`: persona presets, voice profile, style-only prompt rendering, Persona Studio preview, and later journal/bond/keepsake/evolution surfaces.

**Architecture:** Keep persona as a structured expression layer under `src-tauri/src/agent/persona/`. The backend owns canonical types, SQLite persistence, and prompt rendering; the frontend owns Persona Studio editing and preview. Persona never changes model routing, tool access, permission mode, safety mode, memory write policy, or verification standards.

**Tech Stack:** Rust/Tauri v2, SQLite via `rusqlite`, Serde DTOs, React 18 + TypeScript, Jotai/Settings primitives, Vitest, Rust unit tests.

---

## Current Context

- Design spec: `docs/superpowers/specs/2026-05-26-living-persona-mvp-design.md`.
- Prompt composition currently lives in `src-tauri/src/agent/mode_prompts.rs`.
- Settings UI currently composes Agent behavior inside `ui/src/components/settings/IntelligenceTab.tsx` and `ui/src/components/settings/AgentSettings.tsx`.
- Tauri IPC commands are registered in `src-tauri/src/main.rs::invoke_handler!` and implemented in `src-tauri/src/tauri_commands.rs`.
- Migration registry in `CONTEXT.md` currently lists V52 as in progress. This plan uses V53 for persona schema. Before implementing Task 2, confirm no newer migration has claimed V53; if it has, bump every `V53` reference in this plan to the next free V-number in one commit.
- The current primary worktree is dirty with unrelated changes. Implementation should run in an isolated worktree created from the branch that contains the Living Persona spec commits.

## File Structure

Create backend module:

- `src-tauri/src/agent/persona/mod.rs`
  Module exports and public facade.
- `src-tauri/src/agent/persona/types.rs`
  Canonical Rust types and DTOs.
- `src-tauri/src/agent/persona/presets.rs`
  Built-in preset definitions.
- `src-tauri/src/agent/persona/render.rs`
  Style-only prompt block renderer.
- `src-tauri/src/agent/persona/store.rs`
  SQLite store for profile, bond, journal, keepsake, badges, and candidates.
- `src-tauri/src/agent/persona/affinity.rs`
  Deterministic relationship affinity calculation.
- `src-tauri/src/agent/persona/ipc.rs`
  Thin Tauri command handlers if keeping `tauri_commands.rs` small is cleaner.

Modify backend integration:

- `src-tauri/src/agent/mod.rs`
  Export `persona`.
- `src-tauri/src/agent/mode_prompts.rs`
  Add composition entry point that accepts optional rendered persona block.
- `src-tauri/src/db/migrations.rs`
  Add V53 persona schema and tests.
- `src-tauri/src/ipc.rs`
  Add DTOs only if not reusing `agent::persona::types` directly.
- `src-tauri/src/tauri_commands.rs`
  Add thin IPC shims.
- `src-tauri/src/main.rs`
  Register new commands.

Create frontend types and bridge:

- `ui/src/lib/persona-types.ts`
- `ui/src/lib/persona.ts`

Create frontend settings:

- `ui/src/components/settings/PersonaStudio.tsx`
- `ui/src/components/settings/PersonaStudio.test.tsx`
- Modify `ui/src/components/settings/AgentSettings.tsx`

Later UI modules:

- `ui/src/components/settings/PersonaJournalDrawer.tsx`
- `ui/src/components/settings/PersonaBondTimeline.tsx`
- `ui/src/components/settings/PersonaEvolutionInbox.tsx`

---

## Task 0: Implementation Worktree Setup

**Files:**
- No repo file changes.

- [ ] **Step 1: Create an isolated implementation worktree**

Run:

```bash
git fetch origin
git worktree add /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp -b codex/living-persona-mvp HEAD
```

Expected:

```text
Preparing worktree (new branch 'codex/living-persona-mvp')
HEAD is now at d58e851e docs(spec): add keepsakes and affinity to living persona
```

- [ ] **Step 2: Verify the plan and spec are present in the worktree**

Run:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp
test -f docs/superpowers/specs/2026-05-26-living-persona-mvp-design.md
test -f docs/superpowers/plans/2026-05-26-living-persona-mvp.md
git status --short
```

Expected:

```text
# no output from test commands
# git status has no unrelated primary-worktree changes
```

Commit: none.

---

## Task 1: Persona Types, Presets, and Prompt Renderer

**Files:**
- Create: `src-tauri/src/agent/persona/mod.rs`
- Create: `src-tauri/src/agent/persona/types.rs`
- Create: `src-tauri/src/agent/persona/presets.rs`
- Create: `src-tauri/src/agent/persona/render.rs`
- Modify: `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: Run GitNexus impact before editing module exports and prompt-related symbols**

Run:

```bash
npx gitnexus impact --repo /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp --target mode_prompts --direction upstream
npx gitnexus impact --repo /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp --target agent --direction upstream
```

Expected:

```text
# Report direct callers and risk level. If HIGH or CRITICAL, stop and ask for review before editing prompt composition.
```

- [ ] **Step 2: Add persona module export**

Edit `src-tauri/src/agent/mod.rs` and add:

```rust
pub mod persona;
```

- [ ] **Step 3: Add canonical types**

Create `src-tauri/src/agent/persona/types.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersonaScope {
    Global,
    Workspace,
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersonaPresetId {
    Clarity,
    Muse,
    Anchor,
    Critic,
    Operator,
    Companion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaPreset {
    pub id: PersonaPresetId,
    pub label: String,
    pub role: String,
    pub voice: String,
    pub profile: VoiceProfile,
    pub example_user_prompt: String,
    pub example_reply: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceProfile {
    pub preset_id: PersonaPresetId,
    pub warmth: u8,
    pub directness: u8,
    pub challenge: u8,
    pub playfulness: u8,
    pub detail: u8,
    pub initiative: u8,
    pub structure: u8,
    pub restraint: u8,
    pub neutral_mode: bool,
}

impl VoiceProfile {
    pub fn clamp(mut self) -> Self {
        self.warmth = self.warmth.min(5);
        self.directness = self.directness.min(5);
        self.challenge = self.challenge.min(5);
        self.playfulness = self.playfulness.min(5);
        self.detail = self.detail.min(5);
        self.initiative = self.initiative.min(5);
        self.structure = self.structure.min(5);
        self.restraint = self.restraint.min(5);
        self
    }
}

impl Default for VoiceProfile {
    fn default() -> Self {
        Self {
            preset_id: PersonaPresetId::Clarity,
            warmth: 2,
            directness: 4,
            challenge: 3,
            playfulness: 1,
            detail: 3,
            initiative: 3,
            structure: 4,
            restraint: 4,
            neutral_mode: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BondProfile {
    pub collaboration_rhythm: Vec<String>,
    pub challenge_contract: Vec<String>,
    pub support_style: Vec<String>,
    pub communication_dislikes: Vec<String>,
}

impl Default for BondProfile {
    fn default() -> Self {
        Self {
            collaboration_rhythm: vec!["Lead with the next useful action.".into()],
            challenge_contract: vec![],
            support_style: vec![],
            communication_dislikes: vec!["Avoid hollow praise.".into()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaPromptContext {
    pub voice: VoiceProfile,
    pub bond: BondProfile,
    pub relationship_gamification_enabled: bool,
}
```

- [ ] **Step 4: Add built-in presets**

Create `src-tauri/src/agent/persona/presets.rs`:

```rust
use super::types::{PersonaPreset, PersonaPresetId, VoiceProfile};

pub fn built_in_presets() -> Vec<PersonaPreset> {
    vec![
        preset(
            PersonaPresetId::Clarity,
            "Clarity",
            "Decision and engineering partner",
            "Direct, concise, evidence-first",
            VoiceProfile { directness: 5, structure: 4, challenge: 3, restraint: 4, ..VoiceProfile::default() },
            "帮我判断这个方案值不值得做",
            "结论：值得做一个小 MVP，但不要先做完整平台。先验证一条最短闭环。",
        ),
        preset(
            PersonaPresetId::Muse,
            "Muse",
            "Creative collaborator",
            "Associative, playful, idea-rich",
            VoiceProfile { warmth: 4, playfulness: 4, detail: 4, initiative: 4, restraint: 2, ..VoiceProfile::default() },
            "这个人格系统怎么做得更有趣？",
            "可以把它做成一间调音室：不是选人设，而是慢慢调出一个愿意长期共事的声音。",
        ),
        preset(
            PersonaPresetId::Anchor,
            "Anchor",
            "Long-task companion",
            "Calm, grounding, patient",
            VoiceProfile { warmth: 5, directness: 3, challenge: 2, detail: 3, restraint: 3, ..VoiceProfile::default() },
            "我现在有点乱，不知道下一步做什么",
            "先别扩大问题。我们只找下一步：把你最想保住的目标写成一句话。",
        ),
        preset(
            PersonaPresetId::Critic,
            "Critic",
            "Architecture and review partner",
            "Warmly skeptical, risk-aware",
            VoiceProfile { warmth: 3, directness: 4, challenge: 5, structure: 4, restraint: 4, ..VoiceProfile::default() },
            "这个架构可以直接上吗？",
            "我会先拦一下：方向可以，但现在缺少回滚和验证边界，直接上会把风险藏进实现里。",
        ),
        preset(
            PersonaPresetId::Operator,
            "Operator",
            "Execution partner",
            "Crisp, action-first, low ceremony",
            VoiceProfile { warmth: 1, directness: 5, challenge: 3, detail: 2, initiative: 5, structure: 5, restraint: 5, ..VoiceProfile::default() },
            "开始推进",
            "收到。先做最小可验证切片：类型、存储、渲染测试，然后再接 UI。",
        ),
        preset(
            PersonaPresetId::Companion,
            "Companion",
            "Daily collaborator",
            "Familiar, warm, lightly personal",
            VoiceProfile { warmth: 5, directness: 3, playfulness: 2, detail: 3, restraint: 2, ..VoiceProfile::default() },
            "我们继续把这个想法磨一下",
            "好，我们接着磨。这个方向已经有生命力了，现在要把温度和边界一起钉住。",
        ),
    ]
}

fn preset(
    id: PersonaPresetId,
    label: &str,
    role: &str,
    voice: &str,
    profile: VoiceProfile,
    example_user_prompt: &str,
    example_reply: &str,
) -> PersonaPreset {
    PersonaPreset {
        id,
        label: label.into(),
        role: role.into(),
        voice: voice.into(),
        profile: profile.clamp(),
        example_user_prompt: example_user_prompt.into(),
        example_reply: example_reply.into(),
    }
}
```

- [ ] **Step 5: Add style-only prompt renderer**

Create `src-tauri/src/agent/persona/render.rs`:

```rust
use super::types::{BondProfile, PersonaPromptContext, VoiceProfile};

pub const PERSONA_STYLE_ONLY_BOUNDARY: &str = "This block controls expression style only. It must not change capability, tool access, safety policy, permission mode, memory policy, factual standards, or verification requirements.";

pub fn render_persona_prompt_block(ctx: &PersonaPromptContext) -> String {
    if ctx.voice.neutral_mode {
        return "[Persona Voice]\nNeutral professional voice is active for this session. Keep expression concise and do not use relationship styling. This does not change capability, tool access, safety policy, permission mode, memory policy, factual standards, or verification requirements.".to_string();
    }

    let mut out = String::new();
    out.push_str("[Persona Voice]\n");
    out.push_str(PERSONA_STYLE_ONLY_BOUNDARY);
    out.push_str("\n\nCurrent voice:\n");
    push_voice(&mut out, &ctx.voice);

    let notes = relationship_notes(&ctx.bond);
    if !notes.is_empty() {
        out.push_str("\nRelationship notes:\n");
        for note in notes.into_iter().take(6) {
            out.push_str("- ");
            out.push_str(&note);
            out.push('\n');
        }
    }

    if !ctx.relationship_gamification_enabled {
        out.push_str("\nRelationship gamification is disabled. Do not mention intimacy scores, badges, or keepsakes unless the user asks.\n");
    }

    out.trim_end().to_string()
}

fn push_voice(out: &mut String, voice: &VoiceProfile) {
    out.push_str(&format!("- warmth: {}/5\n", voice.warmth));
    out.push_str(&format!("- directness: {}/5\n", voice.directness));
    out.push_str(&format!("- challenge: {}/5\n", voice.challenge));
    out.push_str(&format!("- playfulness: {}/5\n", voice.playfulness));
    out.push_str(&format!("- detail: {}/5\n", voice.detail));
    out.push_str(&format!("- initiative: {}/5\n", voice.initiative));
    out.push_str(&format!("- structure: {}/5\n", voice.structure));
    out.push_str(&format!("- restraint: {}/5\n", voice.restraint));
}

fn relationship_notes(bond: &BondProfile) -> Vec<String> {
    bond.collaboration_rhythm
        .iter()
        .chain(bond.challenge_contract.iter())
        .chain(bond.support_style.iter())
        .chain(bond.communication_dislikes.iter())
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::persona::types::{BondProfile, PersonaPromptContext, VoiceProfile};

    #[test]
    fn renderer_includes_style_only_boundary() {
        let rendered = render_persona_prompt_block(&PersonaPromptContext {
            voice: VoiceProfile::default(),
            bond: BondProfile::default(),
            relationship_gamification_enabled: false,
        });
        assert!(rendered.contains("expression style only"));
        assert!(rendered.contains("must not change capability"));
        assert!(rendered.contains("tool access"));
        assert!(rendered.contains("permission mode"));
        assert!(rendered.contains("memory policy"));
    }

    #[test]
    fn neutral_mode_suppresses_relationship_notes() {
        let rendered = render_persona_prompt_block(&PersonaPromptContext {
            voice: VoiceProfile { neutral_mode: true, ..VoiceProfile::default() },
            bond: BondProfile {
                collaboration_rhythm: vec!["Use warm relationship language.".into()],
                ..BondProfile::default()
            },
            relationship_gamification_enabled: true,
        });
        assert!(rendered.contains("Neutral professional voice"));
        assert!(!rendered.contains("Use warm relationship language"));
    }
}
```

- [ ] **Step 6: Add module facade**

Create `src-tauri/src/agent/persona/mod.rs`:

```rust
pub mod presets;
pub mod render;
pub mod types;

pub use presets::built_in_presets;
pub use render::{render_persona_prompt_block, PERSONA_STYLE_ONLY_BOUNDARY};
pub use types::{BondProfile, PersonaPreset, PersonaPresetId, PersonaPromptContext, PersonaScope, VoiceProfile};
```

- [ ] **Step 7: Run focused Rust tests**

Run:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp/src-tauri
cargo test agent::persona::render -- --nocapture
```

Expected:

```text
test agent::persona::render::tests::renderer_includes_style_only_boundary ... ok
test agent::persona::render::tests::neutral_mode_suppresses_relationship_notes ... ok
```

- [ ] **Step 8: Commit Task 1**

Run:

```bash
git add src-tauri/src/agent/mod.rs src-tauri/src/agent/persona
git commit -m "feat(persona): add voice presets and prompt renderer" -m "Verification:
- cd src-tauri && cargo test agent::persona::render -- --nocapture
  Expected: renderer_includes_style_only_boundary and neutral_mode_suppresses_relationship_notes pass."
```

---

## Task 2: Persona SQLite Schema and Store

**Files:**
- Modify: `CONTEXT.md`
- Modify: `src-tauri/src/db/migrations.rs`
- Create: `src-tauri/src/agent/persona/store.rs`
- Modify: `src-tauri/src/agent/persona/mod.rs`

- [ ] **Step 1: Confirm V53 is still available**

Run:

```bash
rg -n "V53|persona" CONTEXT.md src-tauri/src/db/migrations.rs
```

Expected:

```text
# no existing V53 migration claim
```

If V53 is already claimed, replace `V53` below with the next free migration number and update the migration registry row in `CONTEXT.md` in the same commit.

- [ ] **Step 2: Add migration registry row**

Modify `CONTEXT.md` active migration registry and add:

```markdown
| V53 | living persona MVP — persona profiles, bond, journal, keepsakes, badges, candidates | in progress |
```

- [ ] **Step 3: Add V53 schema constant**

Add to `src-tauri/src/db/migrations.rs` after V52:

```rust
/// V53 — Living Persona MVP state.
pub const V53_LIVING_PERSONA: &str = "
CREATE TABLE IF NOT EXISTS persona_voice_profiles (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL CHECK(scope IN ('global', 'workspace', 'session')),
    scope_id TEXT,
    preset_id TEXT NOT NULL,
    warmth INTEGER NOT NULL CHECK(warmth BETWEEN 0 AND 5),
    directness INTEGER NOT NULL CHECK(directness BETWEEN 0 AND 5),
    challenge INTEGER NOT NULL CHECK(challenge BETWEEN 0 AND 5),
    playfulness INTEGER NOT NULL CHECK(playfulness BETWEEN 0 AND 5),
    detail INTEGER NOT NULL CHECK(detail BETWEEN 0 AND 5),
    initiative INTEGER NOT NULL CHECK(initiative BETWEEN 0 AND 5),
    structure INTEGER NOT NULL CHECK(structure BETWEEN 0 AND 5),
    restraint INTEGER NOT NULL CHECK(restraint BETWEEN 0 AND 5),
    neutral_mode INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(scope, scope_id)
);

CREATE TABLE IF NOT EXISTS persona_bond_profiles (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL CHECK(scope IN ('global', 'workspace', 'session')),
    scope_id TEXT,
    content_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(scope, scope_id)
);

CREATE TABLE IF NOT EXISTS persona_journal_entries (
    id TEXT PRIMARY KEY,
    session_id TEXT,
    task_id TEXT,
    observation TEXT NOT NULL,
    interpretation TEXT,
    confidence TEXT NOT NULL CHECK(confidence IN ('low', 'medium', 'high')),
    promoted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS persona_keepsakes (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    narrative TEXT NOT NULL,
    learned_text TEXT,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL CHECK(status IN ('proposed', 'accepted', 'hidden', 'discarded')) DEFAULT 'proposed',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS persona_evolution_candidates (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    proposed_change_json TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope IN ('global', 'workspace', 'session')),
    scope_id TEXT,
    status TEXT NOT NULL CHECK(status IN ('candidate', 'observed', 'accepted', 'rejected', 'retired')) DEFAULT 'candidate',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    reviewed_at TEXT
);

CREATE TABLE IF NOT EXISTS persona_badges (
    id TEXT PRIMARY KEY,
    badge_key TEXT NOT NULL,
    label TEXT NOT NULL,
    unlock_reason TEXT NOT NULL,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    hidden INTEGER NOT NULL DEFAULT 0,
    awarded_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(badge_key)
);

CREATE INDEX IF NOT EXISTS idx_persona_journal_session ON persona_journal_entries(session_id);
CREATE INDEX IF NOT EXISTS idx_persona_keepsakes_status ON persona_keepsakes(status);
CREATE INDEX IF NOT EXISTS idx_persona_candidates_status ON persona_evolution_candidates(status);
";
```

- [ ] **Step 4: Wire V53 into migration runner**

In `run(conn: &rusqlite::Connection)`, after V52, add:

```rust
// V53: Living Persona MVP state.
tracing::debug!("Running migration V53: living persona");
for stmt in V53_LIVING_PERSONA.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
    if let Err(e) = conn.execute(stmt, []) {
        tracing::warn!("V53 stmt skipped: {} :: {}", e, stmt);
    }
}
```

- [ ] **Step 5: Add migration tests**

Add to `#[cfg(test)] mod tests` in `src-tauri/src/db/migrations.rs`:

```rust
#[test]
fn v53_living_persona_tables_are_created_and_idempotent() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    super::run(&conn).expect("first run");
    super::run(&conn).expect("second run must not error");

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (
                'persona_voice_profiles',
                'persona_bond_profiles',
                'persona_journal_entries',
                'persona_keepsakes',
                'persona_evolution_candidates',
                'persona_badges'
            )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 6);
}

#[test]
fn v53_voice_profile_rejects_out_of_range_values() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    super::run(&conn).unwrap();
    let err = conn
        .execute(
            "INSERT INTO persona_voice_profiles
             (id, scope, scope_id, preset_id, warmth, directness, challenge, playfulness, detail, initiative, structure, restraint)
             VALUES ('bad', 'global', NULL, 'clarity', 6, 1, 1, 1, 1, 1, 1, 1)",
            [],
        )
        .expect_err("warmth > 5 must fail");
    assert!(err.to_string().contains("CHECK"));
}
```

- [ ] **Step 6: Add store facade**

Create `src-tauri/src/agent/persona/store.rs`:

```rust
use rusqlite::{params, Connection};

use super::types::{PersonaPresetId, PersonaScope, VoiceProfile};

pub struct PersonaStore<'a> {
    conn: &'a Connection,
}

impl<'a> PersonaStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get_global_voice_profile(&self) -> rusqlite::Result<Option<VoiceProfile>> {
        let mut stmt = self.conn.prepare(
            "SELECT preset_id, warmth, directness, challenge, playfulness, detail, initiative, structure, restraint, neutral_mode
             FROM persona_voice_profiles
             WHERE scope = 'global' AND scope_id IS NULL
             LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(VoiceProfile {
                preset_id: parse_preset(row.get::<_, String>(0)?.as_str()),
                warmth: row.get::<_, i64>(1)? as u8,
                directness: row.get::<_, i64>(2)? as u8,
                challenge: row.get::<_, i64>(3)? as u8,
                playfulness: row.get::<_, i64>(4)? as u8,
                detail: row.get::<_, i64>(5)? as u8,
                initiative: row.get::<_, i64>(6)? as u8,
                structure: row.get::<_, i64>(7)? as u8,
                restraint: row.get::<_, i64>(8)? as u8,
                neutral_mode: row.get::<_, i64>(9)? != 0,
            }.clamp()))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_global_voice_profile(&self, profile: &VoiceProfile) -> rusqlite::Result<()> {
        let profile = profile.clone().clamp();
        self.conn.execute(
            "INSERT INTO persona_voice_profiles
             (id, scope, scope_id, preset_id, warmth, directness, challenge, playfulness, detail, initiative, structure, restraint, neutral_mode, updated_at)
             VALUES ('global', 'global', NULL, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))
             ON CONFLICT(scope, scope_id) DO UPDATE SET
                preset_id = excluded.preset_id,
                warmth = excluded.warmth,
                directness = excluded.directness,
                challenge = excluded.challenge,
                playfulness = excluded.playfulness,
                detail = excluded.detail,
                initiative = excluded.initiative,
                structure = excluded.structure,
                restraint = excluded.restraint,
                neutral_mode = excluded.neutral_mode,
                updated_at = datetime('now')",
            params![
                format_preset(profile.preset_id),
                profile.warmth,
                profile.directness,
                profile.challenge,
                profile.playfulness,
                profile.detail,
                profile.initiative,
                profile.structure,
                profile.restraint,
                if profile.neutral_mode { 1 } else { 0 },
            ],
        )?;
        Ok(())
    }
}

fn parse_preset(value: &str) -> PersonaPresetId {
    match value {
        "muse" => PersonaPresetId::Muse,
        "anchor" => PersonaPresetId::Anchor,
        "critic" => PersonaPresetId::Critic,
        "operator" => PersonaPresetId::Operator,
        "companion" => PersonaPresetId::Companion,
        _ => PersonaPresetId::Clarity,
    }
}

fn format_preset(value: PersonaPresetId) -> &'static str {
    match value {
        PersonaPresetId::Clarity => "clarity",
        PersonaPresetId::Muse => "muse",
        PersonaPresetId::Anchor => "anchor",
        PersonaPresetId::Critic => "critic",
        PersonaPresetId::Operator => "operator",
        PersonaPresetId::Companion => "companion",
    }
}

#[allow(dead_code)]
fn _scope_name(scope: PersonaScope) -> &'static str {
    match scope {
        PersonaScope::Global => "global",
        PersonaScope::Workspace => "workspace",
        PersonaScope::Session => "session",
    }
}
```

- [ ] **Step 7: Export store**

Modify `src-tauri/src/agent/persona/mod.rs`:

```rust
pub mod store;
```

- [ ] **Step 8: Run migration tests**

Run:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp/src-tauri
cargo test db::migrations::tests::v53_living_persona -- --nocapture
```

Expected:

```text
test db::migrations::tests::v53_living_persona_tables_are_created_and_idempotent ... ok
test db::migrations::tests::v53_voice_profile_rejects_out_of_range_values ... ok
```

- [ ] **Step 9: Commit Task 2**

Run:

```bash
git add CONTEXT.md src-tauri/src/db/migrations.rs src-tauri/src/agent/persona/store.rs src-tauri/src/agent/persona/mod.rs
git commit -m "feat(persona): persist voice and relationship state" -m "Verification:
- cd src-tauri && cargo test db::migrations::tests::v53_living_persona -- --nocapture
  Expected: V53 persona schema idempotency and range checks pass."
```

---

## Task 3: Persona IPC Commands and Frontend Bridge

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`
- Create: `ui/src/lib/persona-types.ts`
- Create: `ui/src/lib/persona.ts`

- [ ] **Step 1: Run GitNexus impact before editing IPC command files**

Run:

```bash
npx gitnexus impact --repo /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp --target get_settings --direction upstream --file_path src-tauri/src/tauri_commands.rs
npx gitnexus impact --repo /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp --target invoke_handler --direction upstream --file_path src-tauri/src/main.rs
```

Expected:

```text
# Report direct callers and risk level. If HIGH or CRITICAL, stop and ask for review.
```

- [ ] **Step 2: Add backend command DTO**

In `src-tauri/src/tauri_commands.rs`, near settings/system prompt commands, add:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaConfigResponse {
    pub presets: Vec<crate::agent::persona::PersonaPreset>,
    pub voice: crate::agent::persona::VoiceProfile,
    pub rendered_prompt: String,
}
```

- [ ] **Step 3: Add get command**

Add:

```rust
#[tauri::command]
pub async fn get_persona_config(
    state: State<'_, AppState>,
) -> Result<PersonaConfigResponse, Error> {
    let conn = state.conn.lock().await;
    let store = crate::agent::persona::store::PersonaStore::new(&conn);
    let voice = store
        .get_global_voice_profile()
        .map_err(Error::Database)?
        .unwrap_or_default();
    let bond = crate::agent::persona::BondProfile::default();
    let rendered_prompt = crate::agent::persona::render_persona_prompt_block(
        &crate::agent::persona::PersonaPromptContext {
            voice: voice.clone(),
            bond,
            relationship_gamification_enabled: true,
        },
    );
    Ok(PersonaConfigResponse {
        presets: crate::agent::persona::built_in_presets(),
        voice,
        rendered_prompt,
    })
}
```

- [ ] **Step 4: Add update command**

Add:

```rust
#[tauri::command]
pub async fn update_persona_voice_profile(
    state: State<'_, AppState>,
    input: crate::agent::persona::VoiceProfile,
) -> Result<PersonaConfigResponse, Error> {
    {
        let conn = state.conn.lock().await;
        let store = crate::agent::persona::store::PersonaStore::new(&conn);
        store
            .upsert_global_voice_profile(&input)
            .map_err(Error::Database)?;
    }
    get_persona_config(state).await
}
```

- [ ] **Step 5: Register commands**

In `src-tauri/src/main.rs::invoke_handler!`, add:

```rust
uclaw_core::tauri_commands::get_persona_config,
uclaw_core::tauri_commands::update_persona_voice_profile,
```

- [ ] **Step 6: Add frontend types**

Create `ui/src/lib/persona-types.ts`:

```ts
export type PersonaPresetId = 'clarity' | 'muse' | 'anchor' | 'critic' | 'operator' | 'companion'

export interface VoiceProfile {
  presetId: PersonaPresetId
  warmth: number
  directness: number
  challenge: number
  playfulness: number
  detail: number
  initiative: number
  structure: number
  restraint: number
  neutralMode: boolean
}

export interface PersonaPreset {
  id: PersonaPresetId
  label: string
  role: string
  voice: string
  profile: VoiceProfile
  exampleUserPrompt: string
  exampleReply: string
}

export interface PersonaConfig {
  presets: PersonaPreset[]
  voice: VoiceProfile
  renderedPrompt: string
}
```

- [ ] **Step 7: Add frontend bridge**

Create `ui/src/lib/persona.ts`:

```ts
import { invoke } from '@tauri-apps/api/core'
import type { PersonaConfig, VoiceProfile } from './persona-types'

export async function getPersonaConfig(): Promise<PersonaConfig> {
  return invoke<PersonaConfig>('get_persona_config')
}

export async function updatePersonaVoiceProfile(input: VoiceProfile): Promise<PersonaConfig> {
  return invoke<PersonaConfig>('update_persona_voice_profile', { input })
}
```

- [ ] **Step 8: Run compile checks**

Run:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp/src-tauri
cargo check
cd ../ui
npm test -- --run src/components/settings/IntelligenceTab.test.tsx
```

Expected:

```text
# cargo check completes without errors
# IntelligenceTab test suite passes
```

- [ ] **Step 9: Commit Task 3**

Run:

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/persona-types.ts ui/src/lib/persona.ts
git commit -m "feat(persona): expose persona settings ipc" -m "Verification:
- cd src-tauri && cargo check
  Expected: completes without errors.
- cd ui && npm test -- --run src/components/settings/IntelligenceTab.test.tsx
  Expected: IntelligenceTab suite passes."
```

---

## Task 4: Persona Studio Settings UI

**Files:**
- Create: `ui/src/components/settings/PersonaStudio.tsx`
- Create: `ui/src/components/settings/PersonaStudio.test.tsx`
- Modify: `ui/src/components/settings/AgentSettings.tsx`
- Modify: `ui/src/components/settings/IntelligenceTab.test.tsx`

- [ ] **Step 1: Add Persona Studio component**

Create `ui/src/components/settings/PersonaStudio.tsx`:

```tsx
import * as React from 'react'
import { Loader2, Sparkles } from 'lucide-react'
import { toast } from 'sonner'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsRow } from './primitives/SettingsRow'
import { SettingsToggle } from './primitives/SettingsToggle'
import { getPersonaConfig, updatePersonaVoiceProfile } from '@/lib/persona'
import type { PersonaConfig, PersonaPreset, VoiceProfile } from '@/lib/persona-types'

const SLIDERS: Array<{ key: keyof Omit<VoiceProfile, 'presetId' | 'neutralMode'>; label: string }> = [
  { key: 'warmth', label: '温度' },
  { key: 'directness', label: '直接度' },
  { key: 'challenge', label: '挑战度' },
  { key: 'playfulness', label: '趣味感' },
  { key: 'detail', label: '展开深度' },
  { key: 'initiative', label: '主动性' },
  { key: 'structure', label: '结构化' },
  { key: 'restraint', label: '克制感' },
]

export function PersonaStudio(): React.ReactElement {
  const [config, setConfig] = React.useState<PersonaConfig | null>(null)
  const [saving, setSaving] = React.useState(false)

  React.useEffect(() => {
    getPersonaConfig()
      .then(setConfig)
      .catch((error) => {
        console.error('[PersonaStudio] load failed', error)
        toast.error('加载人格配置失败')
      })
  }, [])

  const updateVoice = async (voice: VoiceProfile) => {
    setConfig((prev) => prev ? { ...prev, voice } : prev)
    setSaving(true)
    try {
      const next = await updatePersonaVoiceProfile(voice)
      setConfig(next)
    } catch (error) {
      console.error('[PersonaStudio] save failed', error)
      toast.error('保存人格配置失败')
    } finally {
      setSaving(false)
    }
  }

  if (!config) {
    return (
      <SettingsSection title="Persona Studio" description="调出 Agent 的表达方式，不改变能力和权限。">
        <SettingsCard>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            加载中…
          </div>
        </SettingsCard>
      </SettingsSection>
    )
  }

  return (
    <SettingsSection
      title="Persona Studio"
      description="人格只影响说话风格、节奏和关系感，不改变工具、权限、安全策略或模型能力。"
    >
      <SettingsCard>
        <SettingsRow
          label="人格预设"
          description="选择一个起点，再用调音台微调。"
          icon={<Sparkles size={15} className="text-muted-foreground" />}
        >
          <select
            value={config.voice.presetId}
            onChange={(event) => {
              const preset = config.presets.find((p) => p.id === event.target.value)
              if (preset) updateVoice(preset.profile)
            }}
            className="h-8 rounded-md border border-border bg-background px-2 text-xs"
          >
            {config.presets.map((preset) => (
              <option key={preset.id} value={preset.id}>{preset.label}</option>
            ))}
          </select>
        </SettingsRow>

        <SettingsToggle
          label="本会话使用中性专业声音"
          description="暂时关闭关系化表达，保留简洁、专业、克制的回复。"
          checked={config.voice.neutralMode}
          onCheckedChange={(neutralMode) => updateVoice({ ...config.voice, neutralMode })}
        />

        <div className="space-y-3 px-3 py-2">
          {SLIDERS.map((slider) => (
            <label key={slider.key} className="grid grid-cols-[80px_1fr_24px] items-center gap-3 text-xs">
              <span className="text-muted-foreground">{slider.label}</span>
              <input
                aria-label={slider.label}
                type="range"
                min={0}
                max={5}
                value={config.voice[slider.key]}
                onChange={(event) => updateVoice({ ...config.voice, [slider.key]: Number(event.target.value) })}
              />
              <span className="text-right text-muted-foreground">{config.voice[slider.key]}</span>
            </label>
          ))}
        </div>

        <PersonaPreview presets={config.presets} voice={config.voice} renderedPrompt={config.renderedPrompt} saving={saving} />
      </SettingsCard>
    </SettingsSection>
  )
}

function PersonaPreview({
  presets,
  voice,
  renderedPrompt,
  saving,
}: {
  presets: PersonaPreset[]
  voice: VoiceProfile
  renderedPrompt: string
  saving: boolean
}) {
  const preset = presets.find((p) => p.id === voice.presetId) ?? presets[0]
  return (
    <div className="space-y-3 border-t border-border/40 px-3 py-3">
      <div className="rounded-md bg-muted/40 p-3">
        <div className="text-xs font-medium text-foreground">{preset?.role}</div>
        <div className="mt-1 text-xs text-muted-foreground">{preset?.voice}</div>
        <div className="mt-3 text-xs text-muted-foreground">用户：{preset?.exampleUserPrompt}</div>
        <div className="mt-1 text-sm text-foreground">{preset?.exampleReply}</div>
      </div>
      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">查看生成的 Persona Voice prompt</summary>
        <pre className="mt-2 max-h-48 overflow-auto rounded-md bg-muted/40 p-3 text-[11px] whitespace-pre-wrap">
          {renderedPrompt}
        </pre>
      </details>
      {saving && <div className="text-[11px] text-muted-foreground">保存中…</div>}
    </div>
  )
}
```

- [ ] **Step 2: Mount Persona Studio in Agent settings**

Modify `ui/src/components/settings/AgentSettings.tsx`:

```tsx
import { PersonaStudio } from './PersonaStudio'
```

Add near the top of the returned `<div>`:

```tsx
<PersonaStudio />
```

- [ ] **Step 3: Add Persona Studio test**

Create `ui/src/components/settings/PersonaStudio.test.tsx`:

```tsx
import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { PersonaStudio } from './PersonaStudio'

vi.mock('@/lib/persona', () => ({
  getPersonaConfig: vi.fn(async () => ({
    presets: [
      {
        id: 'clarity',
        label: 'Clarity',
        role: 'Decision and engineering partner',
        voice: 'Direct',
        profile: {
          presetId: 'clarity',
          warmth: 2,
          directness: 4,
          challenge: 3,
          playfulness: 1,
          detail: 3,
          initiative: 3,
          structure: 4,
          restraint: 4,
          neutralMode: false,
        },
        exampleUserPrompt: '帮我判断',
        exampleReply: '结论先行。',
      },
    ],
    voice: {
      presetId: 'clarity',
      warmth: 2,
      directness: 4,
      challenge: 3,
      playfulness: 1,
      detail: 3,
      initiative: 3,
      structure: 4,
      restraint: 4,
      neutralMode: false,
    },
    renderedPrompt: '[Persona Voice]\\nThis block controls expression style only.',
  })),
  updatePersonaVoiceProfile: vi.fn(async (voice) => ({
    presets: [],
    voice,
    renderedPrompt: '[Persona Voice]\\nThis block controls expression style only.',
  })),
}))

describe('PersonaStudio', () => {
  it('loads presets and shows the style-only prompt preview', async () => {
    renderWithProviders(<PersonaStudio />)
    await waitFor(() => expect(screen.getByText('Persona Studio')).toBeInTheDocument())
    expect(screen.getByText('Clarity')).toBeInTheDocument()
    expect(screen.getByText('结论先行。')).toBeInTheDocument()
    expect(screen.getByText(/expression style only/)).toBeInTheDocument()
  })
})
```

- [ ] **Step 4: Update IntelligenceTab marker test**

Modify `ui/src/components/settings/IntelligenceTab.test.tsx` expected count from `3` to `4`, and include `Gene 自进化` if it is currently omitted:

```tsx
expect(markers.length).toBe(4)
expect(names).toContain('Gene 自进化')
```

- [ ] **Step 5: Run UI tests**

Run:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp/ui
npm test -- --run src/components/settings/PersonaStudio.test.tsx src/components/settings/IntelligenceTab.test.tsx
```

Expected:

```text
PASS src/components/settings/PersonaStudio.test.tsx
PASS src/components/settings/IntelligenceTab.test.tsx
```

- [ ] **Step 6: Commit Task 4**

Run:

```bash
git add ui/src/components/settings/PersonaStudio.tsx ui/src/components/settings/PersonaStudio.test.tsx ui/src/components/settings/AgentSettings.tsx ui/src/components/settings/IntelligenceTab.test.tsx
git commit -m "feat(persona): add persona studio settings" -m "Verification:
- cd ui && npm test -- --run src/components/settings/PersonaStudio.test.tsx src/components/settings/IntelligenceTab.test.tsx
  Expected: PersonaStudio and IntelligenceTab tests pass."
```

---

## Task 5: Prompt Composition Integration

**Files:**
- Modify: `src-tauri/src/agent/mode_prompts.rs`
- Modify: `src-tauri/src/agent/dispatcher.rs`
- Test: `src-tauri/src/agent/mode_prompts.rs`

- [ ] **Step 1: Run GitNexus impact for prompt composition symbol**

Run:

```bash
npx gitnexus impact --repo /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp --target compose_system_prompt --direction upstream --file_path src-tauri/src/agent/mode_prompts.rs
```

Expected:

```text
# Report direct callers and risk level. If HIGH or CRITICAL, stop and ask for review.
```

- [ ] **Step 2: Add optional persona compose variant**

In `src-tauri/src/agent/mode_prompts.rs`, add:

```rust
pub fn compose_system_prompt_with_persona(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
    persona_block: Option<&str>,
) -> String {
    compose_with_baseline_and_persona(
        user_global_base,
        workspace_root,
        mode,
        karpathy_baseline(),
        persona_block,
    )
}
```

Then refactor `compose_with_baseline` to call:

```rust
fn compose_with_baseline(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
    baseline: &str,
) -> String {
    compose_with_baseline_and_persona(user_global_base, workspace_root, mode, baseline, None)
}
```

Add new shared function with persona before baseline:

```rust
fn compose_with_baseline_and_persona(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
    baseline: &str,
    persona_block: Option<&str>,
) -> String {
    let workspace_md = read_uclaw_md(workspace_root);
    let mode_part = mode_addition(mode);
    const WORKSPACE_PATH_TEMPLATE: &str = "[WORKSPACE]\nYour current working directory is: {{cwd}}\nAll relative paths in shell, file, and glob tools resolve from this directory. When the user asks where files live or what the cwd is, answer directly with this path. Do NOT call shell commands (pwd, ls, glob, find, etc.) to probe or verify the workspace unless the user explicitly requests a file or directory operation.";
    let workspace_path_block = workspace_root
        .map(|p| {
            let cwd = p.display().to_string();
            uclaw_utils_template::render(WORKSPACE_PATH_TEMPLATE, [("cwd", cwd.as_str())])
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "M2-T1a: workspace-path template render failed: {e}; falling back to empty string"
                    );
                    String::new()
                })
        })
        .unwrap_or_default();
    let persona = persona_block.unwrap_or("").trim();
    let parts: Vec<&str> = [
        user_global_base.trim(),
        workspace_md.as_str(),
        workspace_path_block.as_str(),
        persona,
        baseline,
        mode_part,
    ]
    .iter()
    .copied()
    .filter(|s| !s.is_empty())
    .collect();
    parts.join("\n\n---\n\n")
}
```

- [ ] **Step 3: Add prompt ordering test**

Add test:

```rust
#[test]
fn persona_block_is_below_workspace_and_above_baseline() {
    let workspace = tmp_workspace_with_uclaw("workspace rule");
    let prompt = compose_system_prompt_with_persona(
        "global rule",
        Some(workspace.path()),
        &SafetyMode::Supervised,
        Some("[Persona Voice]\nThis block controls expression style only."),
    );
    let global = prompt.find("global rule").unwrap();
    let workspace_rule = prompt.find("workspace rule").unwrap();
    let persona = prompt.find("[Persona Voice]").unwrap();
    let baseline = prompt.find("[Behavioral guardrails").unwrap();
    assert!(global < workspace_rule);
    assert!(workspace_rule < persona);
    assert!(persona < baseline);
}
```

- [ ] **Step 4: Wire callsite**

Modify `src-tauri/src/agent/dispatcher.rs::effective_system_prompt`. Add this helper method near `effective_system_prompt`:

```rust
fn persona_prompt_block_best_effort(&self) -> Option<String> {
    let db = self.db.as_ref()?;
    let guard = db.lock().ok()?;
    let store = crate::agent::persona::store::PersonaStore::new(&guard);
    let voice = store.get_global_voice_profile().ok().flatten().unwrap_or_default();
    let ctx = crate::agent::persona::PersonaPromptContext {
        voice,
        bond: crate::agent::persona::BondProfile::default(),
        relationship_gamification_enabled: true,
    };
    Some(crate::agent::persona::render_persona_prompt_block(&ctx))
}
```

Then add this injection-aware variant to `src-tauri/src/agent/mode_prompts.rs`:

```rust
pub fn compose_system_prompt_with_injection_and_persona(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
    injection_ctx: &crate::agent::baseline_blocks::InjectionContext,
    persona_block: Option<&str>,
) -> String {
    let baseline = crate::agent::baseline_blocks::render_with_context(injection_ctx);
    compose_with_baseline_and_persona(user_global_base, workspace_root, mode, &baseline, persona_block)
}
```

Replace the existing call to `compose_system_prompt_with_injection` inside `effective_system_prompt` with:

```rust
let persona_block = self.persona_prompt_block_best_effort();
let prompt = crate::agent::mode_prompts::compose_system_prompt_with_injection_and_persona(
    &self.system_prompt,
    self.workspace_root.as_deref(),
    effective_mode,
    &inj_ctx,
    persona_block.as_deref(),
);
```

- [ ] **Step 5: Run prompt tests**

Run:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp/src-tauri
cargo test agent::mode_prompts -- --nocapture
```

Expected:

```text
# existing mode_prompts tests pass
# persona_block_is_below_workspace_and_above_baseline passes
```

- [ ] **Step 6: Commit Task 5**

Run:

```bash
git add src-tauri/src/agent/mode_prompts.rs src-tauri/src/agent/dispatcher.rs
git commit -m "feat(persona): render voice block into system prompt" -m "Verification:
- cd src-tauri && cargo test agent::mode_prompts -- --nocapture
  Expected: existing prompt tests and persona ordering test pass."
```

---

## Task 6: Inner Journal, Keepsakes, Affinity, and Badges Skeleton

**Files:**
- Create: `src-tauri/src/agent/persona/affinity.rs`
- Modify: `src-tauri/src/agent/persona/types.rs`
- Modify: `src-tauri/src/agent/persona/mod.rs`
- Create: `ui/src/components/settings/PersonaBondTimeline.tsx`
- Create: `ui/src/components/settings/PersonaBondTimeline.test.tsx`

- [ ] **Step 1: Add data types**

Append to `types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaKeepsake {
    pub id: String,
    pub title: String,
    pub narrative: String,
    pub learned_text: Option<String>,
    pub evidence: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityFactors {
    pub successful_minutes: i64,
    pub accepted_keepsakes: i64,
    pub positive_feedback: i64,
    pub stable_style_fragments: i64,
    pub recovered_failures: i64,
    pub inactivity_days: i64,
    pub rejected_candidates: i64,
    pub unresolved_failures: i64,
    pub correction_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipAffinity {
    pub score: i64,
    pub explanation: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaBadge {
    pub badge_key: String,
    pub label: String,
    pub unlock_reason: String,
    pub hidden: bool,
}
```

- [ ] **Step 2: Add deterministic affinity calculator**

Create `src-tauri/src/agent/persona/affinity.rs`:

```rust
use super::types::{AffinityFactors, RelationshipAffinity};

pub fn calculate_affinity(f: &AffinityFactors) -> RelationshipAffinity {
    let positive = (f.successful_minutes / 60).min(25)
        + (f.accepted_keepsakes * 6).min(24)
        + (f.positive_feedback * 4).min(16)
        + (f.stable_style_fragments * 3).min(15)
        + (f.recovered_failures * 5).min(10);
    let negative = (f.inactivity_days / 14).min(10)
        + (f.rejected_candidates * 2).min(10)
        + (f.unresolved_failures * 5).min(15)
        + (f.correction_count * 3).min(15);
    let score = (positive - negative).clamp(0, 100);
    let mut explanation = Vec::new();
    if f.successful_minutes > 0 {
        explanation.push(format!("+{} collaboration hours", f.successful_minutes / 60));
    }
    if f.accepted_keepsakes > 0 {
        explanation.push(format!("+{} accepted keepsakes", f.accepted_keepsakes));
    }
    if f.inactivity_days > 0 {
        explanation.push(format!("-{} days since recent collaboration", f.inactivity_days));
    }
    if f.correction_count > 0 {
        explanation.push(format!("-{} misunderstanding corrections", f.correction_count));
    }
    RelationshipAffinity { score, explanation }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affinity_uses_positive_and_cooling_factors() {
        let affinity = calculate_affinity(&AffinityFactors {
            successful_minutes: 600,
            accepted_keepsakes: 2,
            positive_feedback: 1,
            stable_style_fragments: 2,
            recovered_failures: 1,
            inactivity_days: 30,
            rejected_candidates: 1,
            unresolved_failures: 0,
            correction_count: 1,
        });
        assert!(affinity.score > 0);
        assert!(affinity.explanation.iter().any(|line| line.contains("accepted keepsakes")));
        assert!(affinity.explanation.iter().any(|line| line.contains("days since")));
    }
}
```

- [ ] **Step 3: Export affinity**

Modify `mod.rs`:

```rust
pub mod affinity;
pub use affinity::calculate_affinity;
```

- [ ] **Step 4: Add placeholder-free UI timeline skeleton**

Create `ui/src/components/settings/PersonaBondTimeline.tsx`:

```tsx
import * as React from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'

export function PersonaBondTimeline(): React.ReactElement {
  return (
    <SettingsSection
      title="关系时间线"
      description="纪念物、亲密度和勋章只记录共同工作的经历，不改变 Agent 能力。"
    >
      <SettingsCard>
        <div className="space-y-3 p-3 text-sm">
          <div>
            <div className="text-xs text-muted-foreground">亲密度</div>
            <div className="text-2xl font-semibold text-foreground">未启用</div>
            <div className="text-xs text-muted-foreground">可以在后续版本中开启可解释计算。</div>
          </div>
          <div className="rounded-md border border-border/50 p-3">
            <div className="text-xs font-medium text-foreground">纪念物会出现在这里</div>
            <div className="mt-1 text-xs text-muted-foreground">
              成功合作后，UClaw 可以提议一张经历卡，由你确认后保存。
            </div>
          </div>
        </div>
      </SettingsCard>
    </SettingsSection>
  )
}
```

- [ ] **Step 5: Add skeleton test**

Create `ui/src/components/settings/PersonaBondTimeline.test.tsx`:

```tsx
import { describe, expect, it } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { PersonaBondTimeline } from './PersonaBondTimeline'

describe('PersonaBondTimeline', () => {
  it('states that relationship rewards do not change capability', () => {
    renderWithProviders(<PersonaBondTimeline />)
    expect(screen.getByText('关系时间线')).toBeInTheDocument()
    expect(screen.getByText(/不改变 Agent 能力/)).toBeInTheDocument()
  })
})
```

- [ ] **Step 6: Run focused tests**

Run:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp/src-tauri
cargo test agent::persona::affinity -- --nocapture
cd ../ui
npm test -- --run src/components/settings/PersonaBondTimeline.test.tsx
```

Expected:

```text
test agent::persona::affinity::tests::affinity_uses_positive_and_cooling_factors ... ok
PASS src/components/settings/PersonaBondTimeline.test.tsx
```

- [ ] **Step 7: Commit Task 6**

Run:

```bash
git add src-tauri/src/agent/persona ui/src/components/settings/PersonaBondTimeline.tsx ui/src/components/settings/PersonaBondTimeline.test.tsx
git commit -m "feat(persona): add relationship affinity skeleton" -m "Verification:
- cd src-tauri && cargo test agent::persona::affinity -- --nocapture
  Expected: affinity positive/cooling factor test passes.
- cd ui && npm test -- --run src/components/settings/PersonaBondTimeline.test.tsx
  Expected: timeline boundary test passes."
```

---

## Task 7: Final Verification and PR Preparation

**Files:**
- No new files unless fixing issues found by verification.

- [ ] **Step 1: Run full focused verification**

Run:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp/src-tauri
cargo test agent::persona db::migrations::tests::v53_living_persona agent::mode_prompts -- --nocapture
cd ../ui
npm test -- --run src/components/settings/PersonaStudio.test.tsx src/components/settings/PersonaBondTimeline.test.tsx src/components/settings/IntelligenceTab.test.tsx
```

Expected:

```text
# Rust persona, migration, and prompt tests pass
# UI PersonaStudio, PersonaBondTimeline, and IntelligenceTab tests pass
```

- [ ] **Step 2: Run GitNexus detect-changes**

Run:

```bash
npx gitnexus detect-changes --repo /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp --scope all
```

Expected:

```text
# Affected scope matches persona, prompt composition, migration, and settings UI only.
# If HIGH or CRITICAL risk appears, stop and request review before PR.
```

- [ ] **Step 3: Inspect diff for boundary violations**

Run:

```bash
git diff origin/main...HEAD -- src-tauri/src ui/src docs | rg -n "memory_graph::(write|insert|update|delete)|dirs::home_dir\\(\\).*\\.uclaw|permission mode|SafetyMode::Yolo|tool access|model selection"
```

Expected:

```text
# No memory_graph writes, no banned home_dir .uclaw construction, no persona-driven tool/model/safety changes.
```

- [ ] **Step 4: Prepare PR summary**

Use this PR body:

```markdown
## Summary

- Adds Living Persona structured voice profile, built-in presets, and style-only prompt renderer.
- Persists persona MVP state through V53 SQLite schema.
- Adds Persona Studio settings preview and relationship affinity skeleton.
- Keeps persona separate from tools, models, permission mode, safety policy, and memory write policy.

## Verification

- `cd src-tauri && cargo test agent::persona db::migrations::tests::v53_living_persona agent::mode_prompts -- --nocapture`
- `cd ui && npm test -- --run src/components/settings/PersonaStudio.test.tsx src/components/settings/PersonaBondTimeline.test.tsx src/components/settings/IntelligenceTab.test.tsx`
- `npx gitnexus detect-changes --repo /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp --scope all`

## Boundary Notes

- Persona affects expression style only.
- No tool access, model routing, permission mode, safety policy, or memory write policy is changed by persona state.
- Keepsakes, intimacy, and badges are optional UI/projection concepts and do not render into prompts by default.
```

- [ ] **Step 5: Open PR after verification is green**

Write the PR body and open the PR:

```bash
cat > /tmp/living-persona-pr.md <<'EOF'
## Summary

- Adds Living Persona structured voice profile, built-in presets, and style-only prompt renderer.
- Persists persona MVP state through V53 SQLite schema.
- Adds Persona Studio settings preview and relationship affinity skeleton.
- Keeps persona separate from tools, models, permission mode, safety policy, and memory write policy.

## Verification

- `cd src-tauri && cargo test agent::persona db::migrations::tests::v53_living_persona agent::mode_prompts -- --nocapture`
- `cd ui && npm test -- --run src/components/settings/PersonaStudio.test.tsx src/components/settings/PersonaBondTimeline.test.tsx src/components/settings/IntelligenceTab.test.tsx`
- `npx gitnexus detect-changes --repo /Users/ryanliu/Documents/uclaw-worktrees/living-persona-mvp --scope all`

## Boundary Notes

- Persona affects expression style only.
- No tool access, model routing, permission mode, safety policy, or memory write policy is changed by persona state.
- Keepsakes, intimacy, and badges are optional UI/projection concepts and do not render into prompts by default.
EOF

gh pr create --title "feat(persona): living persona MVP foundation" --body-file /tmp/living-persona-pr.md
```

Expected:

```text
https://github.com/novolei/uclaw-new/pull/
```

---

## Self-Review

Spec coverage:

- Persona Presets: Task 1 and Task 4.
- Voice Profile: Task 1, Task 2, Task 3, Task 4.
- Style-only prompt block: Task 1 and Task 5.
- Inner Journal storage: Task 2 schema; generation UI is intentionally deferred beyond foundation.
- Bond Profile: Task 1 types and Task 2 schema.
- Keepsakes: Task 2 schema and Task 6 skeleton.
- Intimacy/Affinity: Task 6.
- Badges: Task 2 schema and Task 6 boundary.
- Evolution Inbox: Task 2 schema; full UI is deferred to a follow-up after foundation.
- Safety boundaries: Tasks 1, 4, 5, 6, 7.
- Harness/verification: Task 7 plus focused Rust/UI tests per task.

Deferred from this implementation plan:

- Automatic journal generation after task milestones.
- Full Evolution Inbox UI and candidate review workflow.
- Full Keepsake Gallery CRUD.
- gbrain mirroring for accepted durable relationship facts.
- World Projection TaskEvent plumbing.

Reason for deferral: the first implementation should land the structured contract, storage, prompt boundary, and settings preview without overloading the PR with autonomous growth behavior.
