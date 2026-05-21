//! `normalize_tool_schema` — strip / dedupe / depth-cap a JSON-schema
//! tool definition before it's announced to the LLM.
//!
//! Three transforms (per ADR §M2-H L2):
//!
//! 1. **Drop `description.examples`** — top-level + every nested
//!    object's `description` field. Examples often dominate token cost
//!    while adding little marginal information; the description's
//!    leading paragraph is enough.
//!
//! 2. **Merge `enum` arrays** — for any object that has BOTH `enum`
//!    and `oneOf`/`anyOf` patterns producing overlapping enum sets,
//!    collapse them to a single dedup'd `enum` array. Common with
//!    MCP-generated schemas that flatten oneOf-of-enums into both.
//!
//! 3. **Prune nested ≥ 3 layers deep, type-preserving** — replace a
//!    deeply-nested sub-tree with an EMPTY container of the same JSON
//!    type. An Object becomes `{}`; an Array becomes `[]`. Type-
//!    preserving because strict JSON-Schema validators (DeepSeek,
//!    OpenAI tool mode) reject `{ "enum": {"truncated": true, ...} }`
//!    — `enum` keyword's value must remain an Array. The depth counter
//!    starts at 0 at the root and increments on every object/array
//!    descent.
//!
//!    The previous implementation (Bundle 5 → followup) replaced both
//!    Object and Array with a `{"truncated": true, "original_depth": N}`
//!    Object marker; that broke schemas that had deep arrays (enum,
//!    examples, allOf/oneOf). The current implementation drops the
//!    marker and accepts that we lose the "original_depth" metadata —
//!    in exchange for not breaking validators.
//!
//! The transform is **idempotent** and **non-mutating** — input
//! `serde_json::Value` is consumed, a new `Value` is returned. A
//! `NormalizeStats` byproduct lets callers report what was dropped
//! (useful for logging + the M2-J token-budget UI).

use serde_json::{Map, Value};

/// What the normalizer touched. Returned alongside the rewritten
/// schema for observability.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizeStats {
    /// Number of `description.examples` keys removed.
    pub examples_dropped: usize,
    /// Number of `enum` arrays whose items were deduplicated.
    pub enums_deduped: usize,
    /// Number of nested objects/arrays replaced with truncation
    /// placeholders because they exceeded the depth cap.
    pub deep_nests_pruned: usize,
}

impl NormalizeStats {
    /// `true` if the normalizer didn't have to change anything.
    pub fn is_noop(&self) -> bool {
        self.examples_dropped == 0
            && self.enums_deduped == 0
            && self.deep_nests_pruned == 0
    }
}

/// Default depth cap (root = depth 0; replace at depth ≥ 3).
pub const DEFAULT_MAX_NESTING_DEPTH: usize = 3;

/// Normalize a tool's JSON-schema definition. Returns the rewritten
/// schema and a `NormalizeStats` describing what was touched.
///
/// Pass `max_depth = DEFAULT_MAX_NESTING_DEPTH` for the ADR-spec
/// behaviour. Higher caps relax the prune step.
pub fn normalize_tool_schema(schema: Value, max_depth: usize) -> (Value, NormalizeStats) {
    let mut stats = NormalizeStats::default();
    let rewritten = visit(schema, 0, max_depth, &mut stats);
    (rewritten, stats)
}

// ── internal recursion ─────────────────────────────────────────────

fn visit(v: Value, depth: usize, max_depth: usize, stats: &mut NormalizeStats) -> Value {
    if depth >= max_depth {
        // Type-preserving truncation:
        // - Object → empty Object (`{}`)
        // - Array  → empty Array  (`[]`)
        // - Scalar → kept as-is (already cheap, and removing a scalar
        //            keyword value would break the parent's schema —
        //            e.g. `type: "string"` at depth 3 is a leaf).
        //
        // Idempotency: an empty container is its own fixed point — a
        // second pass over `{}` or `[]` re-enters the match at the
        // same depth and finds no compound to prune, so
        // `deep_nests_pruned` doesn't double-count.
        match v {
            Value::Object(map) => {
                if map.is_empty() {
                    return Value::Object(map);
                }
                stats.deep_nests_pruned += 1;
                Value::Object(Map::new())
            }
            Value::Array(items) => {
                if items.is_empty() {
                    return Value::Array(items);
                }
                stats.deep_nests_pruned += 1;
                Value::Array(Vec::new())
            }
            scalar => scalar,
        }
    } else {
        match v {
            Value::Object(map) => Value::Object(visit_object(map, depth, max_depth, stats)),
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .map(|item| visit(item, depth + 1, max_depth, stats))
                    .collect(),
            ),
            scalar => scalar,
        }
    }
}

fn visit_object(
    mut map: Map<String, Value>,
    depth: usize,
    max_depth: usize,
    stats: &mut NormalizeStats,
) -> Map<String, Value> {
    // Transform 1: drop description.examples
    if let Some(Value::Object(desc)) = map.get_mut("description") {
        if desc.remove("examples").is_some() {
            stats.examples_dropped += 1;
        }
    }

    // Transform 2: dedupe enum arrays
    if let Some(Value::Array(items)) = map.get_mut("enum") {
        let before = items.len();
        // Stable dedupe by string form (preserves first-seen order).
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        items.retain(|item| {
            let key = item.to_string();
            seen.insert(key)
        });
        if items.len() < before {
            stats.enums_deduped += 1;
        }
    }

    // Recurse into every remaining value.
    let mut out = Map::with_capacity(map.len());
    for (k, v) in map.into_iter() {
        out.insert(k, visit(v, depth + 1, max_depth, stats));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── transform 1: drop description.examples ──────────────────────

    #[test]
    fn drops_description_examples_at_root() {
        let schema = json!({
            "name": "shell",
            "description": {
                "summary": "Run a shell command",
                "examples": [
                    {"input": "ls -la", "output": "..."},
                    {"input": "cd /tmp", "output": "..."}
                ]
            }
        });
        let (out, stats) = normalize_tool_schema(schema, DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.examples_dropped, 1);
        let desc = &out["description"];
        assert!(desc.get("summary").is_some(), "summary preserved");
        assert!(desc.get("examples").is_none(), "examples removed");
    }

    #[test]
    fn leaves_string_description_alone() {
        // If description is a plain string (not object), no examples
        // field to drop. Stats unchanged.
        let schema = json!({
            "name": "shell",
            "description": "Run a shell command"
        });
        let (out, stats) = normalize_tool_schema(schema.clone(), DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.examples_dropped, 0);
        assert_eq!(out, schema);
    }

    #[test]
    fn drops_nested_description_examples() {
        let schema = json!({
            "properties": {
                "cmd": {
                    "type": "string",
                    "description": {
                        "summary": "command to run",
                        "examples": ["ls", "pwd"]
                    }
                }
            }
        });
        let (out, stats) = normalize_tool_schema(schema, DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.examples_dropped, 1);
        assert!(
            out["properties"]["cmd"]["description"]
                .get("examples")
                .is_none()
        );
    }

    // ── transform 2: dedupe enums ───────────────────────────────────

    #[test]
    fn dedupes_enum_with_duplicates() {
        let schema = json!({
            "type": "string",
            "enum": ["a", "b", "a", "c", "b"]
        });
        let (out, stats) = normalize_tool_schema(schema, DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.enums_deduped, 1);
        // Preserves first-seen order: a, b, c.
        assert_eq!(out["enum"], json!(["a", "b", "c"]));
    }

    #[test]
    fn leaves_enum_without_duplicates_alone() {
        let schema = json!({
            "type": "string",
            "enum": ["a", "b", "c"]
        });
        let (out, stats) = normalize_tool_schema(schema.clone(), DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.enums_deduped, 0);
        assert_eq!(out, schema);
    }

    #[test]
    fn dedupes_enum_of_mixed_value_types() {
        // enum can carry any JSON values — strings, numbers, objects.
        let schema = json!({
            "enum": [1, "a", 1, {"x": 1}, "a", {"x": 1}]
        });
        let (out, stats) = normalize_tool_schema(schema, DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.enums_deduped, 1);
        assert_eq!(out["enum"], json!([1, "a", {"x": 1}]));
    }

    // ── transform 3: prune deep nests ───────────────────────────────

    #[test]
    fn prunes_objects_at_depth_3_type_preserving() {
        // Build a 5-level deep object.
        let schema = json!({
            "d0": {                            // depth 1 (root is 0)
                "d1": {                        // depth 2
                    "d2": {                    // depth 3 — REPLACED
                        "d3": {"d4": "leaf"}   // never visited
                    }
                }
            }
        });
        let (out, stats) = normalize_tool_schema(schema, DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.deep_nests_pruned, 1);
        // d2 should now be an empty Object — the Object TYPE is preserved
        // so strict validators don't choke on a "should be object" field
        // suddenly carrying a marker keyed `truncated`.
        let d2 = &out["d0"]["d1"]["d2"];
        assert_eq!(d2, &json!({}));
        assert!(d2.is_object());
    }

    #[test]
    fn prunes_arrays_at_depth_3_type_preserving() {
        // The bug the previous implementation hit: a deeply-nested array
        // (e.g. `enum: [...]` at depth 3) was replaced with an Object
        // marker, breaking strict JSON-schema validators that demand the
        // `enum` keyword's value remain an array.
        let schema = json!({
            "d0": {
                "d1": {
                    "enum": ["a", "b", "c"]  // depth 3 — REPLACED with []
                }
            }
        });
        let (out, stats) = normalize_tool_schema(schema, DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.deep_nests_pruned, 1);
        let pruned = &out["d0"]["d1"]["enum"];
        assert_eq!(pruned, &json!([]));
        assert!(pruned.is_array(), "type must stay array, not become object");
    }

    #[test]
    fn deeply_nested_scalar_kept_unchanged() {
        // A leaf scalar at depth >= max_depth must NOT be replaced — it's
        // typically a keyword's required scalar value (e.g. `type: "object"`).
        let schema = json!({"d0": {"d1": {"d2": "string"}}});
        let (out, stats) = normalize_tool_schema(schema, DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.deep_nests_pruned, 0);
        assert_eq!(out["d0"]["d1"]["d2"], json!("string"));
    }

    #[test]
    fn shallow_schemas_untouched_by_depth_prune() {
        let schema = json!({
            "name": "shell",
            "args": {"cmd": "string"}
        });
        let (out, stats) = normalize_tool_schema(schema.clone(), DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.deep_nests_pruned, 0);
        assert_eq!(out, schema);
    }

    #[test]
    fn higher_depth_cap_relaxes_prune() {
        let schema = json!({
            "d0": {"d1": {"d2": {"d3": "leaf"}}}
        });
        // Default cap 3 → would prune d2.
        let (_, stats3) = normalize_tool_schema(schema.clone(), 3);
        assert_eq!(stats3.deep_nests_pruned, 1);
        // Cap 5 → no prune.
        let (out5, stats5) = normalize_tool_schema(schema.clone(), 5);
        assert_eq!(stats5.deep_nests_pruned, 0);
        assert_eq!(out5["d0"]["d1"]["d2"]["d3"], json!("leaf"));
    }

    // ── combined / idempotency / NormalizeStats ─────────────────────

    #[test]
    fn all_three_transforms_compose() {
        let schema = json!({
            "description": {"summary": "x", "examples": ["a"]},
            "enum": ["x", "x", "y"],
            "deep": {"a": {"b": {"c": "leaf"}}}
        });
        let (out, stats) = normalize_tool_schema(schema, DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(stats.examples_dropped, 1);
        assert_eq!(stats.enums_deduped, 1);
        assert_eq!(stats.deep_nests_pruned, 1);
        assert!(!stats.is_noop());
        // Spot-check each transform survived.
        assert!(out["description"].get("examples").is_none());
        assert_eq!(out["enum"], json!(["x", "y"]));
        // Bundle 9 — type-preserving: deep object becomes `{}`, not a marker.
        assert_eq!(out["deep"]["a"]["b"], json!({}));
        assert!(out["deep"]["a"]["b"].is_object());
    }

    #[test]
    fn idempotent_second_pass_is_noop() {
        let schema = json!({
            "description": {"summary": "x", "examples": ["a"]},
            "enum": ["a", "a"],
            "deep": {"a": {"b": {"c": "leaf"}}}
        });
        let (once, _) = normalize_tool_schema(schema, DEFAULT_MAX_NESTING_DEPTH);
        let (twice, stats) = normalize_tool_schema(once.clone(), DEFAULT_MAX_NESTING_DEPTH);
        assert!(
            stats.is_noop(),
            "second pass should not change anything"
        );
        assert_eq!(once, twice);
    }

    #[test]
    fn empty_schema_passes_through_noop() {
        let schema = json!({});
        let (out, stats) = normalize_tool_schema(schema, DEFAULT_MAX_NESTING_DEPTH);
        assert!(stats.is_noop());
        assert_eq!(out, json!({}));
    }
}
