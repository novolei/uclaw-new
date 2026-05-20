# Memory + gbrain Eval Scorecard

Status: implemented for PR #249 as an app-native memory/gbrain eval adapter with an inventory command entrypoint, live write/recall probe, and recall-grounding score primitives.

The memory/gbrain harness converts `MemoryInventorySmokeReport` into reproducible harness episodes and `memory_gbrain_scorecard` artifacts. It is intentionally strict about service truth: reachable-empty is different from unavailable/error, and gbrain tool exposure is scored separately from page count.

It also defines a shared `MemoryGbrainEvalEvidence` contract for end-to-end write/recall probes. The app command now writes a namespaced harness fact into memU/gbrain when those services are connected, retrieves it through memU/gbrain recall paths, and scores write receipts, recall evidence, expected grounded facts, and forbidden hallucinated facts.

| Capability | Case | Primary Signals |
|---|---|---|
| memU inventory | `memory.memu.inventory` | memU health, reachable empty vs populated state |
| gbrain inventory | `memory.gbrain.inventory` | gbrain MCP reachability, reachable empty vs populated state |
| gbrain tooling | `memory.gbrain.tooling` | minimum exposed MCP tool count |
| dual health | `memory.dual_inventory.health` | combined memU + gbrain smoke truth without hallucinated contents |
| grounded recall | `memory.recall.grounded_fact` | write receipts, memU/gbrain recall evidence, expected facts, forbidden facts |
| hallucination guard | `memory.recall.no_hallucinated_fact` | expected user fact present, forbidden invented fact absent |
| gbrain page grounding | `memory.gbrain.page_grounding` | gbrain page/search evidence contains grounded page fact and not invented page fact |

App entrypoint:

- `run_memory_inventory_smoke`: returns the raw memU/gbrain inventory truth report.
- `run_memory_gbrain_eval_harness`: runs the inventory smoke, executes a namespaced live write/recall probe, records harness episodes, and writes scorecard/input artifacts under the app data harness directory.

Verification command:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory --lib
cargo test --manifest-path src-tauri/Cargo.toml memory_gbrain_eval_harness_command_tests --lib
cargo test --manifest-path src-tauri/Cargo.toml harness::memory_inventory --lib
cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw
git diff --check
```
