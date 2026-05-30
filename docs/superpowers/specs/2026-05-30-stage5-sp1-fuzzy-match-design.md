# 阶段 5 SP1 — 9-Strategy Fuzzy-Match Chain Design Spec

**Status:** Approved (appetite = full 阶段 5; SP1 first). Proceeding to plan.
**Date:** 2026-05-30
**Position:** Sub-project 1 of 4 in 阶段 5 (hermes coding reliability). The gap-audit's explicit do-first, highest-ROI item (§1.9, §3.3, line 152). Precedes SP3 (checkpoint), SP2 (three-signal verify), SP4 (read contract).

---

## 1. Goal

Port hermes's 9-strategy fuzzy-match chain (`tools/fuzzy_match.py`) into uClaw's edit path so the builtin `edit` tool absorbs the whitespace / indentation / escape / unicode drift that LLM-produced `old_text` routinely contains — turning "old_text not found" hard-failures into successful edits. Today `edit.rs` does **exact-only** matching (`content.find(old_text)`), so any drift fails the edit. This is the user-visible "edits fail less" win (gap-audit's conservative-fallback pick, line 194).

**Scope:** a new pure `fuzzy_match` module (all 9 strategies + escape-drift guard + position-mapping) wired into `edit.rs`'s 3 find sites (execute / validate / apply). No behavior change when `old_text` matches exactly (strategy 1 = exact, tried first). No new deps beyond what's present (`regex`, `unicode-normalization` if needed).

**Out of scope (later SPs):** read-back byte-compare verify + lint + LSP delta (SP2); shadow checkpoint (SP3); read contract (SP4).

---

## 2. The chain (faithful port of `tools/fuzzy_match.py`)

`fuzzy_find_and_replace(content, old, new, replace_all) -> Result<FuzzyOutcome, String>` tries strategies **in order**; first one with matches wins:

1. **exact** — `content.find(pattern)`, all positions. (Fast path; identical behavior to today when old_text matches.)
2. **line_trimmed** — strip leading/trailing whitespace per line, normalized match, map spans back to original.
3. **whitespace_normalized** — collapse `[ \t]+` → single space, match in normalized, map back.
4. **indentation_flexible** — `lstrip` each line (ignore indentation entirely), normalized match.
5. **escape_normalized** — unescape `\n`/`\t`/`\r` in the pattern, then exact match.
6. **trimmed_boundary** — strip only the first/last pattern line, sliding-window over content lines.
7. **unicode_normalized** — NFC-normalize both sides, match, map back (handles smart-quote / combining-char drift).
8. **block_anchor** — match first+last lines, similarity for the middle (for large blocks with drifted interiors).
9. **context_aware** — final fuzzy fallback (per hermes — implementer ports verbatim).

**On the first matching strategy:**
- If `len(matches) > 1 && !replace_all` → `Err("Found N matches… provide more context or replace_all")` (ambiguity guard — same as today's uniqueness expectation).
- **Escape-drift guard** (only when `strategy != exact`): `_detect_escape_drift` — if `new` contains `\'`/`\"` sequences that are present in `old` (model "preserving context") but absent in the matched file region, that's transport serialization drift; writing `new` verbatim would corrupt the file → `Err(helpful message)` so the model re-reads + retries. **Critical safety guard — port faithfully.**
- Else `_apply_replacements(content, matches, new)` → `FuzzyOutcome { new_content, match_count, strategy_name }`.
- No strategy matches → `Err("Could not find a match for old_string")`.

Validation: empty `old` → Err; `old == new` → Err (no-op).

---

## 3. Components

### 3.1 `agent/tools/builtin/fuzzy_match.rs` (new)

```rust
pub struct FuzzyOutcome {
    pub new_content: String,
    pub match_count: usize,
    pub strategy: &'static str,  // "exact" | "line_trimmed" | ...
}

/// Port of hermes fuzzy_find_and_replace. Tries 9 strategies in order;
/// first match wins. Returns Err on ambiguity / escape-drift / no-match.
pub fn fuzzy_find_and_replace(
    content: &str,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Result<FuzzyOutcome, String>;

// private: strategy_exact, strategy_line_trimmed, ... strategy_context_aware
//   each: fn(content: &str, pattern: &str) -> Vec<(usize, usize)>  (BYTE spans)
// private: detect_escape_drift, apply_replacements, position-mapping helpers
```

- **Byte offsets, not char indices.** Python uses char indices; Rust slices by byte. All match spans are **byte ranges** into `content`, and the position-mapping helpers (norm↔orig) must produce byte-aligned, char-boundary-safe spans (especially for `unicode_normalized` where normalization changes byte length). `apply_replacements` splices by byte span (sorted descending so earlier splices don't shift later spans). **Never split a UTF-8 char boundary** — use `str::is_char_boundary` assertions / `char_indices` mapping.
- The norm↔orig position mapping (`_build_orig_to_norm_map` / `_map_positions_norm_to_orig` / `_find_normalized_matches` / `_map_normalized_positions`) is the fidelity-critical part — port carefully, test against multi-byte content.

### 3.2 `edit.rs` integration (3 sites)

`edit.rs` finds `old_text` at three points (verified): `execute_single_file` (~239), `validate_single_file` (~357), `apply_validated_single_file` (~414). Each currently does `content.find(old_text)` (exact). Replace each with `fuzzy_find_and_replace(&content, old_text, new_text, replace_all)`:
- **validate** site: call fuzzy, on `Err` return the same "not found / ambiguous" failure shape the tool already returns (so the tool's error contract is unchanged for callers); on `Ok` proceed.
- **apply** site: use `outcome.new_content` directly (no separate `content.replace` — the fuzzy fn already produced the spliced content). Record `outcome.strategy` + `match_count` for the diff/edit_count + a `tracing::info!(strategy = …)` so non-exact matches are observable.
- **execute** site (if it does find+replace in one): route through fuzzy.
- **`replace_all`**: map edit.rs's existing replace-all/edit_count semantics to the fuzzy fn's `replace_all` param. Verify how edit.rs currently expresses "replace all vs unique".

### 3.3 No tool-contract change

The `edit` tool's input schema (`old_text`/`new_text`/`replace_all`) + output (`Applied { diff, edit_count }` / failure) are unchanged. Fuzzy matching is purely the internal find step becoming tolerant. An exact match still goes through strategy 1 (identical result). The only observable differences: previously-failing drifted edits now succeed (logged with their strategy), and escape-drift corruption is blocked with a clear error.

---

## 4. Error handling

| Case | Behavior |
|---|---|
| `old` empty / `old == new` | `Err` (validation; same as a no-op today) |
| no strategy matches | `Err("Could not find a match…")` → tool's "not found" failure |
| >1 match, not replace_all | `Err("Found N matches… more context or replace_all")` → tool's ambiguity failure |
| escape-drift detected (non-exact strategy) | `Err(drift message)` → blocks corruption; model re-reads/retries |
| ok | spliced `new_content` + strategy + count |

The escape-drift guard is a **safety-critical** correctness gate (prevents writing `\'`/`\"` garbage into source) — it must be ported verbatim and tested.

---

## 5. Testing

Exhaustive pure-function tests (no I/O — `fuzzy_find_and_replace` is pure):

| Strategy / case | Tests |
|---|---|
| exact (single, multi, replace_all, ambiguity) | 3 |
| line_trimmed (leading/trailing ws drift) | 2 |
| whitespace_normalized (collapsed spaces/tabs) | 2 |
| indentation_flexible (re-indented block) | 2 |
| escape_normalized (`\n`/`\t` literal drift) | 2 |
| trimmed_boundary (boundary-line ws) | 1 |
| unicode_normalized (smart quotes / NFC drift) + **multi-byte byte-offset safety** | 3 |
| block_anchor (large block, drifted middle) | 1 |
| context_aware | 1 |
| escape-drift guard (`\'`/`\"` corruption blocked) | 2 |
| order/precedence (exact wins over fuzzy; first-match-wins) | 1 |
| no-match / empty-old / identical | 2 |
| **multi-byte splice never breaks a char boundary** (CJK / emoji content) | 1 |
| edit.rs integration (drifted old_text now applies; strategy logged; exact unchanged) | 2-3 |

~25 tests. The byte-offset/char-boundary tests on multi-byte content are the highest-value (port bugs corrupt files).

---

## 6. Scope boundaries

- **Only the find step becomes fuzzy** — no change to the edit tool's schema, diff generation, or file-write path beyond consuming `outcome.new_content`.
- **All 9 strategies** (gap-audit says "9 策略") — faithful port, not a subset.
- **No SP2/3/4** — no read-back verify, no lint/LSP, no checkpoint, no read contract.
- **No new heavy deps** — `regex` is present; if `unicode_normalize` needs `unicode-normalization`, verify it's in the tree (memory_bucket_seal or elsewhere) before adding; if absent, a minimal NFC via an existing dep or a documented skip of strategy 7's full NFC (fall back to a simpler unicode handling) — implementer flags.

---

## 7. File plan (preview)

| File | New/Mod | Purpose |
|---|---|---|
| `agent/tools/builtin/fuzzy_match.rs` | new | 9-strategy chain + escape-drift + mapping + ~25 tests |
| `agent/tools/builtin/edit.rs` | mod | route the 3 find sites through `fuzzy_find_and_replace`; log strategy |
| `agent/tools/builtin/mod.rs` (or wherever builtin mods are declared) | mod | `mod fuzzy_match;` |

Est. ~600 source + ~300 tests.

---

## 8. Open adaptation questions (resolved at impl)

1. **`unicode-normalization` crate availability** — does the workspace already pull a unicode NFC crate? If yes, use it for strategy 7; if no, either add it (small, widely-used) or port a reduced unicode strategy (the implementer flags + the spec accepts a documented reduced strategy 7 rather than a new heavy dep).
2. **edit.rs `replace_all` semantics** — how the current tool expresses unique-vs-all; map to the fuzzy fn's `replace_all`.
3. **Char vs byte spans in the mapping helpers** — port Python's char-index logic to Rust byte-safe spans; the implementer must verify multi-byte correctness (the spec's multi-byte tests gate this).
4. **The 3 find sites** — confirm execute/validate/apply all need routing (some may share a helper); a shared `find_and_apply` helper avoids triple-wiring.

---

## 9. Success criteria

- A drifted `old_text` (whitespace/indent/escape/unicode) that fails today now applies, with the matching strategy logged.
- Exact matches behave identically (strategy 1, no regression).
- Escape-drift `\'`/`\"` corruption is blocked with a clear retry message.
- Multi-byte content never corrupts on splice (char-boundary-safe).
- The edit tool's schema + error contract are unchanged.
- All existing edit.rs tests stay green; ~25 new fuzzy tests pass. CI hermetic.
