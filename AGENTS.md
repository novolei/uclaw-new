<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **uclaw-new** (28393 symbols, 47562 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/uclaw-new/context` | Codebase overview, check index freshness |
| `gitnexus://repo/uclaw-new/clusters` | All functional areas |
| `gitnexus://repo/uclaw-new/processes` | All execution flows |
| `gitnexus://repo/uclaw-new/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |
| Work in the Agent area (384 symbols) | `.claude/skills/generated/agent/SKILL.md` |
| Work in the Memory_graph area (371 symbols) | `.claude/skills/generated/memory-graph/SKILL.md` |
| Work in the Ui area (238 symbols) | `.claude/skills/generated/ui/SKILL.md` |
| Work in the Settings area (222 symbols) | `.claude/skills/generated/settings/SKILL.md` |
| Work in the Scenarios area (219 symbols) | `.claude/skills/generated/scenarios/SKILL.md` |
| Work in the Runtime area (175 symbols) | `.claude/skills/generated/runtime/SKILL.md` |
| Work in the Proactive area (171 symbols) | `.claude/skills/generated/proactive/SKILL.md` |
| Work in the Chat area (163 symbols) | `.claude/skills/generated/chat/SKILL.md` |
| Work in the Hooks area (141 symbols) | `.claude/skills/generated/hooks/SKILL.md` |
| Work in the Git area (139 symbols) | `.claude/skills/generated/git/SKILL.md` |
| Work in the Memory area (110 symbols) | `.claude/skills/generated/memory/SKILL.md` |
| Work in the Db area (105 symbols) | `.claude/skills/generated/db/SKILL.md` |
| Work in the Learning area (104 symbols) | `.claude/skills/generated/learning/SKILL.md` |
| Work in the Automation area (98 symbols) | `.claude/skills/generated/automation/SKILL.md` |
| Work in the Browser area (94 symbols) | `.claude/skills/generated/browser/SKILL.md` |
| Work in the Marketplace area (87 symbols) | `.claude/skills/generated/marketplace/SKILL.md` |
| Work in the Builtin area (80 symbols) | `.claude/skills/generated/builtin/SKILL.md` |
| Work in the Atoms area (75 symbols) | `.claude/skills/generated/atoms/SKILL.md` |
| Work in the Adapters area (67 symbols) | `.claude/skills/generated/adapters/SKILL.md` |
| Work in the Workspace area (62 symbols) | `.claude/skills/generated/workspace/SKILL.md` |

<!-- gitnexus:end -->
