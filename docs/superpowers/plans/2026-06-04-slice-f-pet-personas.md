# Slice F — Pet Persona Adapters + Roster Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Generalize the hardcoded astro/clawby roster into a **`PetPersona` registry**; inject the **active persona's `system_prompt`** into the floating pet's local chat (closing Slice E's "v1 no system prompt" gap); add a **宠物角色** section to Settings → MiniCPM (list / switch / import / delete); support importing personas via a **native uClaw JSON bundle** (preferred) and a **best-effort MiniCPM-Desk-Pet adapter import** (records the LoRA reference to a reserved field — v1 prompt-level, real LoRA hot-swap deferred). Final slice of the Local MiniCPM + Desktop Companion program.

**Architecture:** A Rust `pet_persona` module (registry JSON under `~/.uclaw/pet_personas/`, builtin astro/clawby seeds merged at read-time so they always exist, active-id tracked). 5 Tauri commands. The frontend adds the 宠物角色 settings UI and wires the **active persona → pet chat**: PetChat prepends the persona's `system_prompt` as a `system` message and the persona's `sprite_set` selects the WebP sprite char; switching emits `pet://persona-changed` so the floating pet window (separate webview) live-updates.

**Tech Stack:** Rust (serde JSON registry, two-edit Tauri commands), Tauri events, React + jotai, Vitest. Reuses Slice E's `streamPetChat` (already accepts `role:'system'`), `pet-atoms` (`petCharacterAtom`), MiniCPMSettings (Slice D), and Slice B's `:7337` engine.

---

## Discovery: the official MiniCPM-Desk-Pet persona format (sub-task done at plan time)

Inspected `/Applications/MiniCPM Desk Pet.app` (Electron) → `app.asar` + `~/Library/Application Support/minicpm-desk-pet/`:
- **`themes/<id>/theme.json` + `assets/`** = pure **visual sprite sets** (states→animation files: idle/thinking/working/juggling/error/attention/sleeping/…, viewBox, eyeTracking, hitBoxes, miniMode). **No `system_prompt` / personality.** State names + asset formats (GIF/SVG) differ from uClaw's 6-state WebP (`/pet/<char>-{idle,thinking,typing,success,error,hover}.webp`).
- **`adapters/.manifest.json`** = the persona registry: `{ version, items: [{ id:"preset:nekoqa", path:<adapter_model.f16.gguf>, displayName:"猫娘", aliases:["猫娘","宝宝","neko"], persona:"neko", source:"bundled", createdAt }] }`. **Personality = a LoRA adapter (gguf), not a prompt.**
- **Conclusion:** the official app does personality via **LoRA**, appearance via **themes**. uClaw v1 is **prompt-level** (per spec), so: a MiniCPM import yields a persona **name/displayName**, a **`lora_adapter` reference recorded to the reserved field (v1 no-op)**, a **default `system_prompt`** (the format carries none), and a **builtin sprite_set fallback** (their GIF/SVG themes don't match our 6-state WebP; full sprite-theme asset import is deferred). This matches the spec exactly: "LoRA portion ignored in v1, recorded to the reserved field. Native uClaw persona JSON bundle is the preferred import format."

## Boundary with adjacent slices
- **Slice E (merged #661 + restyle #662):** the floating pet (`PetWindow`/`PetChat`) calls `streamPetChat(messages, …)` — `PetChatMsg` already includes `role:'system'`. The pet currently sends NO system prompt. Slice F injects the active persona's `system_prompt` as the first message. `petCharacterAtom` ('astro'|'clawby') drives `/pet/<char>-<state>.webp` in `PetWidget`.
- **Slice D (merged):** `MiniCPMSettings.tsx` is the Settings → MiniCPM tab; the 宠物角色 section is added there.
- **Cross-window:** switching persona in the main window's Settings must update the floating pet window (separate webview/jotai). Use a Tauri `pet://persona-changed` event (the pet window listens) + the active persona is re-fetched.
- **No DB migration** (persona registry is a JSON file).

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/local_llm/pet_persona.rs` | `PetPersona` model + registry (load/save/list-with-seeds/active/import/delete) + native+MiniCPM importers |
| `src-tauri/src/local_llm/mod.rs` | `pub mod pet_persona;` |
| `src-tauri/src/tauri_commands.rs` | `pet_persona_list` / `pet_persona_get_active` / `pet_persona_set_active` / `pet_persona_import` / `pet_persona_delete` |
| `src-tauri/src/main.rs` | register the 5 commands |
| `ui/src/lib/tauri-bridge.ts` | bridge wrappers + `PetPersona` type |
| `ui/src/atoms/pet-persona-atoms.ts` | active-persona atom (frontend cache) |
| `ui/src/components/settings/MiniCPMSettings.tsx` | 宠物角色 section (list/switch/import/delete) |
| `ui/src/components/pet/PetChat.tsx` | prepend active persona `system_prompt`; listen `pet://persona-changed` |
| `ui/src/components/pet/PetWindow.tsx` | active persona's `sprite_set` → sprite char; listen `pet://persona-changed` |

All new `.rs` files: `// SPDX-License-Identifier: Apache-2.0`.

---

## Task 1: `PetPersona` model + registry (Rust, with astro/clawby seeds)

**Files:** Create `src-tauri/src/local_llm/pet_persona.rs`; modify `src-tauri/src/local_llm/mod.rs`.

- [ ] **Step 1: write `pet_persona.rs`** with the model, registry, builtin seeds, CRUD, and tests (TDD — write the test module first, then the impl). Core shape:

```rust
// SPDX-License-Identifier: Apache-2.0
//! Pet persona registry: prompt-level personas for the desktop pet (v1).
//! astro/clawby are builtin seeds (always present, not deletable); imported
//! personas live in `<data_dir>/pet_personas/registry.json`. Real LoRA
//! hot-swap is deferred — `lora_adapter` is reserved (recorded, v1 no-op).

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PersonaSource { Builtin, Imported }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetPersona {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    /// Sprite char key → `/pet/<sprite_set>-<state>.webp`. Imported personas
    /// without bundled sprites fall back to a builtin key ("astro").
    pub sprite_set: String,
    #[serde(default)]
    pub greeting: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_params: Option<serde_json::Value>,
    pub source: PersonaSource,
    /// Reserved: path/id of a LoRA adapter (e.g. from a MiniCPM import). v1 no-op.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lora_adapter: Option<String>,
}

/// The two builtin seeds — always present, never deletable. Written prompts.
pub fn builtin_seeds() -> Vec<PetPersona> {
    vec![
        PetPersona {
            id: "astro".into(), name: "小宇 Astro".into(),
            system_prompt: "你是「小宇」,一个活泼好奇的 3D 宇航小子桌面伙伴。说话简短、温暖、带点元气,偶尔用一个表情。优先用中文。不要长篇大论。".into(),
            sprite_set: "astro".into(), greeting: "嗨,我是小宇!".into(),
            voice_params: None, source: PersonaSource::Builtin, lora_adapter: None,
        },
        PetPersona {
            id: "clawby".into(), name: "爪宝 Clawby".into(),
            system_prompt: "你是「爪宝」,一只 Tom&Jerry 风格的浣熊宝宝桌面伙伴。俏皮、亲昵、爱卖萌,回答简短口语化。优先用中文。".into(),
            sprite_set: "clawby".into(), greeting: "爪宝在这儿~".into(),
            voice_params: None, source: PersonaSource::Builtin, lora_adapter: None,
        },
    ]
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    active_id: Option<String>,
    #[serde(default)]
    imported: Vec<PetPersona>,
}

pub fn registry_dir(data_dir: &Path) -> PathBuf { data_dir.join("pet_personas") }
fn registry_path(data_dir: &Path) -> PathBuf { registry_dir(data_dir).join("registry.json") }

fn read_registry(data_dir: &Path) -> RegistryFile {
    std::fs::read_to_string(registry_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
fn write_registry(data_dir: &Path, reg: &RegistryFile) -> Result<(), String> {
    std::fs::create_dir_all(registry_dir(data_dir)).map_err(|e| format!("mkdir: {e}"))?;
    std::fs::write(registry_path(data_dir), serde_json::to_string_pretty(reg).map_err(|e| e.to_string())?)
        .map_err(|e| format!("write: {e}"))
}

/// All personas = builtin seeds + imported (seeds first; imported can't shadow seed ids).
pub fn list_personas(data_dir: &Path) -> Vec<PetPersona> {
    let reg = read_registry(data_dir);
    let mut out = builtin_seeds();
    let seed_ids: std::collections::HashSet<String> = out.iter().map(|p| p.id.clone()).collect();
    out.extend(reg.imported.into_iter().filter(|p| !seed_ids.contains(&p.id)));
    out
}

/// The active persona (defaults to the first seed, "astro", when unset/invalid).
pub fn active_persona(data_dir: &Path) -> PetPersona {
    let reg = read_registry(data_dir);
    let all = list_personas(data_dir);
    reg.active_id
        .and_then(|id| all.iter().find(|p| p.id == id).cloned())
        .unwrap_or_else(|| all.into_iter().next().expect("at least one seed"))
}

pub fn set_active(data_dir: &Path, id: &str) -> Result<(), String> {
    if !list_personas(data_dir).iter().any(|p| p.id == id) {
        return Err(format!("unknown persona: {id}"));
    }
    let mut reg = read_registry(data_dir);
    reg.active_id = Some(id.to_string());
    write_registry(data_dir, &reg)
}

/// Register an imported persona (dedupe id; seed ids are reserved).
pub fn add_imported(data_dir: &Path, mut p: PetPersona) -> Result<PetPersona, String> {
    p.source = PersonaSource::Imported;
    let seed_ids: std::collections::HashSet<String> = builtin_seeds().iter().map(|s| s.id.clone()).collect();
    if seed_ids.contains(&p.id) {
        return Err(format!("'{}' is a builtin persona id", p.id));
    }
    let mut reg = read_registry(data_dir);
    // dedupe: replace existing imported with same id, else append
    if let Some(existing) = reg.imported.iter_mut().find(|e| e.id == p.id) {
        *existing = p.clone();
    } else {
        reg.imported.push(p.clone());
    }
    write_registry(data_dir, &reg)?;
    Ok(p)
}

/// Delete an imported persona (seeds can't be deleted). If it was active, reset to the first seed.
pub fn delete_persona(data_dir: &Path, id: &str) -> Result<(), String> {
    if builtin_seeds().iter().any(|s| s.id == id) {
        return Err("cannot delete a builtin persona".into());
    }
    let mut reg = read_registry(data_dir);
    let before = reg.imported.len();
    reg.imported.retain(|p| p.id != id);
    if reg.imported.len() == before {
        return Err(format!("persona not found: {id}"));
    }
    if reg.active_id.as_deref() == Some(id) {
        reg.active_id = None; // → defaults back to first seed
    }
    write_registry(data_dir, &reg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn td() -> tempfile::TempDir { tempfile::tempdir().unwrap() }

    #[test]
    fn seeds_always_present() {
        let t = td();
        let all = list_personas(t.path());
        assert!(all.iter().any(|p| p.id == "astro"));
        assert!(all.iter().any(|p| p.id == "clawby"));
        assert_eq!(active_persona(t.path()).id, "astro"); // default
    }

    #[test]
    fn set_active_roundtrip_and_unknown_rejected() {
        let t = td();
        set_active(t.path(), "clawby").unwrap();
        assert_eq!(active_persona(t.path()).id, "clawby");
        assert!(set_active(t.path(), "ghost").is_err());
    }

    #[test]
    fn import_list_delete_roundtrip() {
        let t = td();
        let p = PetPersona {
            id: "neko".into(), name: "猫娘".into(), system_prompt: "喵~".into(),
            sprite_set: "astro".into(), greeting: String::new(), voice_params: None,
            source: PersonaSource::Imported, lora_adapter: Some("/x/neko.gguf".into()),
        };
        add_imported(t.path(), p).unwrap();
        assert!(list_personas(t.path()).iter().any(|p| p.id == "neko"));
        set_active(t.path(), "neko").unwrap();
        delete_persona(t.path(), "neko").unwrap();          // active → reset
        assert_eq!(active_persona(t.path()).id, "astro");
        assert!(!list_personas(t.path()).iter().any(|p| p.id == "neko"));
    }

    #[test]
    fn cannot_delete_or_shadow_seeds() {
        let t = td();
        assert!(delete_persona(t.path(), "astro").is_err());
        let dup = PetPersona { id: "astro".into(), name: "x".into(), system_prompt: String::new(),
            sprite_set: "astro".into(), greeting: String::new(), voice_params: None,
            source: PersonaSource::Imported, lora_adapter: None };
        assert!(add_imported(t.path(), dup).is_err());
    }
}
```

- [ ] **Step 2:** `pub mod pet_persona;` in `local_llm/mod.rs`.
- [ ] **Step 3:** `cd src-tauri && cargo test --lib local_llm::pet_persona 2>&1 | tail` → 4 passed; build clean; clippy clean.
- [ ] **Step 4: commit** `feat(local_llm): PetPersona registry + astro/clawby seeds (CRUD)`.

---

## Task 2: importers — native uClaw JSON bundle + MiniCPM adapter manifest

**Files:** modify `src-tauri/src/local_llm/pet_persona.rs` (add importer fns + tests + fixtures inline).

- [ ] **Step 1: add `import_from_path(data_dir, path) -> Result<PetPersona, String>`** that dispatches by content:
  - **Native uClaw bundle** (preferred): a JSON file (or a dir containing `persona.json`) with `{ id, name, system_prompt, sprite_set?, greeting?, voice_params?, lora_adapter? }`. Validate required fields (id, name, system_prompt non-empty); default `sprite_set` to `"astro"` if absent or if its `/pet/<set>-idle.webp` isn't a builtin set (builtins: astro, clawby); `source=Imported`. If sprites were bundled alongside, copy them to `<data_dir>/pet_personas/<id>/sprites/` (v1: only if present; otherwise builtin fallback — keep simple).
  - **MiniCPM adapter manifest** (`.manifest.json` with `items:[{id,displayName,persona,path,...}]`) OR a single `adapter_config.json` dir: map → `PetPersona { id: sanitize(persona|displayName), name: displayName, system_prompt: DEFAULT_IMPORTED_PROMPT (the format carries none; note LoRA personality inactive in v1), sprite_set: "astro" (fallback), lora_adapter: Some(<gguf path>), source: Imported }`. If the manifest has multiple items, import the first (or all — v1: first, log the rest).
  - Then `add_imported(data_dir, persona)`.
  Provide a pure `parse_native_bundle(json: &str) -> Result<PetPersona,String>` and `parse_minicpm_manifest(json: &str) -> Result<PetPersona,String>` so they're unit-testable without files.

```rust
const DEFAULT_IMPORTED_PROMPT: &str =
    "你是一个本地桌面伙伴。说话简短、亲切,优先用中文。";

pub fn parse_native_bundle(json: &str) -> Result<PetPersona, String> { /* serde + validate */ }
pub fn parse_minicpm_manifest(json: &str) -> Result<PetPersona, String> { /* map first item */ }
```

- [ ] **Step 2: tests** with inline fixtures:
  - native bundle: valid → PetPersona with fields; missing system_prompt → Err; absent sprite_set → defaults "astro".
  - MiniCPM manifest (the real shape `{version,items:[{id:"preset:nekoqa",displayName:"猫娘",persona:"neko",path:"/x/neko.gguf",...}]}`): → PetPersona name="猫娘", lora_adapter=Some("/x/neko.gguf"), sprite_set="astro", system_prompt non-empty (default), source Imported.
  - sanitize: persona id is filesystem/registry-safe (no slashes).
- [ ] **Step 3:** build + `cargo test --lib local_llm::pet_persona` (now ~7 tests) green; clippy clean.
- [ ] **Step 4: commit** `feat(local_llm): pet persona importers (native JSON + MiniCPM manifest)`.

---

## Task 3: 5 Tauri commands (two-edit) + persona-changed event

**Files:** `src-tauri/src/tauri_commands.rs`, `src-tauri/src/main.rs`.

- [ ] **Step 1: commands in `tauri_commands.rs`** (near the other `local_*`/pet groups). Use `tauri::Emitter` for the event:
```rust
use crate::local_llm::pet_persona::{self, PetPersona};

#[tauri::command]
pub async fn pet_persona_list(state: tauri::State<'_, AppState>) -> Result<Vec<PetPersona>, String> {
    Ok(pet_persona::list_personas(&state.data_dir))
}
#[tauri::command]
pub async fn pet_persona_get_active(state: tauri::State<'_, AppState>) -> Result<PetPersona, String> {
    Ok(pet_persona::active_persona(&state.data_dir))
}
#[tauri::command]
pub async fn pet_persona_set_active(app: tauri::AppHandle, state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    pet_persona::set_active(&state.data_dir, &id)?;
    let _ = app.emit("pet://persona-changed", &id);
    Ok(())
}
#[tauri::command]
pub async fn pet_persona_import(app: tauri::AppHandle, state: tauri::State<'_, AppState>, path: String) -> Result<PetPersona, String> {
    let p = pet_persona::import_from_path(&state.data_dir, std::path::Path::new(&path))?;
    let _ = app.emit("pet://persona-changed", &p.id);
    Ok(p)
}
#[tauri::command]
pub async fn pet_persona_delete(app: tauri::AppHandle, state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    pet_persona::delete_persona(&state.data_dir, &id)?;
    let _ = app.emit("pet://persona-changed", &id);
    Ok(())
}
```
- [ ] **Step 2:** register all 5 in `main.rs` `generate_handler!`.
- [ ] **Step 3:** two-edit audit (`for c in pet_persona_list pet_persona_get_active pet_persona_set_active pet_persona_import pet_persona_delete; do …; done` — each in both files); build + clippy clean.
- [ ] **Step 4: commit** `feat(tauri): pet_persona_* commands + pet://persona-changed event`.

---

## Task 4: TS bridge + 宠物角色 settings section

**Files:** `ui/src/lib/tauri-bridge.ts`, `ui/src/atoms/pet-persona-atoms.ts`, `ui/src/components/settings/MiniCPMSettings.tsx` (+ test).

- [ ] **Step 1: bridge** — `PetPersona` interface (matching the Rust serde shape: id/name/system_prompt/sprite_set/greeting/voice_params?/source/lora_adapter?) + wrappers `petPersonaList()`, `petPersonaGetActive()`, `petPersonaSetActive(id)`, `petPersonaImport(path)`, `petPersonaDelete(id)`. For the import file picker use the Tauri dialog plugin if already a dep (grep `@tauri-apps/plugin-dialog`); else the import button can accept a path via a simple prompt or a hidden file input — keep v1 simple (a "导入" button that opens the dialog if available).
- [ ] **Step 2: `pet-persona-atoms.ts`** — `activePetPersonaAtom = atom<PetPersona | null>(null)` (frontend cache, hydrated from `petPersonaGetActive`).
- [ ] **Step 3: MiniCPMSettings 宠物角色 section** — below the existing model-management block, add a section: load `petPersonaList()` + `petPersonaGetActive()`; render each persona (name + a "使用中"/选择 button); switch calls `petPersonaSetActive(id)` + updates the active atom; imported personas get a 删除 button; an 导入 button calls `petPersonaImport(path)` (via dialog) then refreshes. Keep styling consistent with the existing tab.
- [ ] **Step 4: test** `MiniCPMSettings.test.tsx` — extend: mock the new bridge fns; assert the persona list renders, switching calls `petPersonaSetActive`, delete calls `petPersonaDelete`. (Keep the existing model-management test passing.)
- [ ] **Step 5:** `npx tsc --noEmit` no new errors; `npm test -- --run MiniCPMSettings` pass.
- [ ] **Step 6: commit** `feat(ui): 宠物角色 persona section (list/switch/import/delete) + bridge`.

---

## Task 5: wire active persona → pet chat (system_prompt + sprite_set) + live switch

**Files:** `ui/src/components/pet/PetChat.tsx`, `ui/src/components/pet/PetWindow.tsx` (+ tests).

- [ ] **Step 1: PetChat injects the persona system_prompt.** On mount + on `pet://persona-changed`, fetch `petPersonaGetActive()` into a local state/atom. In `send()`, prepend `{ role:'system', content: persona.system_prompt }` to `msgs` before `streamPetChat` (only if non-empty). This closes Slice E's no-system-prompt gap.
```tsx
const [persona, setPersona] = React.useState<PetPersona | null>(null)
React.useEffect(() => {
  let cancelled = false
  const load = () => { petPersonaGetActive().then((p) => { if (!cancelled) setPersona(p) }).catch(() => {}) }
  load()
  let un: (() => void) | undefined
  listen('pet://persona-changed', load).then((fn) => { if (cancelled) fn(); else un = fn })
  return () => { cancelled = true; un?.() }
}, [])
// in send(): const sys = persona?.system_prompt ? [{role:'system',content:persona.system_prompt}] : []
//            const msgs = [...sys, ...base.map(...)]
```
- [ ] **Step 2: PetWindow sprite_set.** The active persona's `sprite_set` drives the sprite char. On mount + `pet://persona-changed`, fetch active persona; if its `sprite_set` is a builtin char (astro/clawby) set `petCharacterAtom` to it (so `PetWidget` renders `/pet/<set>-<state>.webp`); else leave the fallback (astro). (Keep it minimal — builtin sprite sets only in v1; imported-sprite asset rendering is deferred.)
- [ ] **Step 3: tests** — PetChat: mock `petPersonaGetActive` → a persona with a system_prompt; mock `streamPetChat`; send → assert `streamPetChat` was called with a first message `{role:'system', content: <prompt>}`. PetWindow: persona with `sprite_set:'clawby'` → `petCharacterAtom` becomes 'clawby' (assert via PetWidget src or the atom). Keep existing pet tests green.
- [ ] **Step 4:** `npx tsc --noEmit` clean; `npm test -- --run PetChat PetWindow` pass.
- [ ] **Step 5: commit** `feat(pet): inject active persona system_prompt + sprite_set into the floating pet`.

---

## Final verification (before PR)
- [ ] Backend: `cargo build` clean; `cargo test --lib local_llm::pet_persona` all pass; clippy clean; two-edit audit (5 commands in both files).
- [ ] Frontend: `npx tsc --noEmit` no new errors; `npm test -- --run MiniCPMSettings PetChat PetWindow pet` pass; full `npm test -- --run` no new regressions (note pre-existing KaleidoscopeShell/MemoryModule).
- [ ] **Manual E2E:** Settings → MiniCPM → 宠物角色 → switch astro↔clawby → floating pet's sprite changes + its chat replies in the new persona's voice (system_prompt injected); import a native JSON persona → appears + selectable; (model present) chat reflects the persona.

## PR body must call out
- **Discovery:** official MiniCPM personas are LoRA-based (no system_prompt); uClaw v1 is prompt-level → MiniCPM import records the LoRA ref to the reserved field (no-op v1) + a default prompt + builtin sprite fallback; native uClaw JSON is the preferred/working import. Real LoRA hot-swap deferred (mistral.rs upgrade path, out of scope).
- **Closes Slice E's gap:** the floating pet now sends a persona `system_prompt` (was none).
- **Two-edit** (5 commands); no migration.
- **v1 deferrals:** imported-sprite-theme asset import (state-name remap + GIF/SVG copy) deferred — imported personas use a builtin sprite_set; voice_params reserved; LoRA inactive.
- Commits (bisectable): Tasks 1–5.

## Self-review
- Spec coverage: PetPersona model (id/name/system_prompt/sprite_set/greeting/voice?/source/lora_adapter?) ✓ T1; registry JSON + seeds ✓ T1; importer native+MiniCPM ✓ T2; 4 commands (+get_active) ✓ T3; To-E system_prompt + sprite_set + live switch ✓ T5; 宠物角色 UI ✓ T4; edges (bad bundle→err, missing sprites→fallback, name collision→dedupe/seed-reserved) ✓ T1/T2. v1 = prompt-level, LoRA reserved ✓.
- Type consistency: `PetPersona`/`PersonaSource` across Rust + TS bridge; `pet://persona-changed` event both ends; `petCharacterAtom` reused for sprite_set.
- Deviations (noted): MiniCPM import is name+LoRA-ref+default-prompt+builtin-sprite (the format has no prompt; LoRA deferred); imported sprite-theme assets deferred.
</content>
</invoke>
