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
#[allow(dead_code)] // used by pet_persona_* commands (Task 3)
pub fn active_persona(data_dir: &Path) -> PetPersona {
    let reg = read_registry(data_dir);
    let all = list_personas(data_dir);
    reg.active_id
        .and_then(|id| all.iter().find(|p| p.id == id).cloned())
        .unwrap_or_else(|| all.into_iter().next().expect("at least one seed"))
}

#[allow(dead_code)] // used by pet_persona_* commands (Task 3)
pub fn set_active(data_dir: &Path, id: &str) -> Result<(), String> {
    if !list_personas(data_dir).iter().any(|p| p.id == id) {
        return Err(format!("unknown persona: {id}"));
    }
    let mut reg = read_registry(data_dir);
    reg.active_id = Some(id.to_string());
    write_registry(data_dir, &reg)
}

/// Register an imported persona (dedupe id; seed ids are reserved).
#[allow(dead_code)] // used by pet_persona_* commands (Task 3)
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
#[allow(dead_code)] // used by pet_persona_* commands (Task 3)
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
