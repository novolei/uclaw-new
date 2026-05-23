# jcode Comparison Report Index

Status: analysis package, no implementation changes.
Date: 2026-05-23
Scope: five-agent comparison of `/Users/ryanliu/Documents/jcode` and `/Users/ryanliu/Documents/uclaw`, followed by ADR alignment review.

## Core Conclusion

jcode is a strong Rust coding-agent runtime reference. uClaw should absorb its backend engineering discipline at the module, trait, protocol, streaming, tool, provider, session, and performance-harness levels.

uClaw should not become a literal jcode clone. The safer target is:

> uClaw Agent OS v2 with jcode-grade Rust modularity and runtime discipline.

The migration should preserve uClaw's Tauri desktop shell, Agent OS v2 contracts, gbrain-primary memory, safety/path policy, browser runtime, automation domain, harness discipline, Capability Mesh, and World Projection direction.

## Reports

| Report | Agent Role | Focus |
|---|---|---|
| `01_framework_design.md` | Architecture Analyst Agent | Rust framework design, runtime identity, AppState/control-plane comparison, uClaw upgrade points |
| `02_workspace_and_modules.md` | Architecture Designer Agent | Cargo/workspace structure, crate boundaries, module split plan, dependency direction |
| `03_performance_optimization.md` | Performance Agent | Hot paths, streaming, search, persistence, allocator/runtime observations, perf harness plan |
| `04_backend_reconstruction_blueprint.md` | Backend Staff Agent | Feasibility of jcode-grade backend reconstruction, 1:1 module matrix, phased PR plan, granular risks |
| `05_frontend_integration.md` | Frontend Senior Designer Agent | Frontend event/projection architecture needed to consume a reconstructed backend |
| `06_adr_gap_audit_and_reference_addenda.md` | ADR Review Agent | Second-pass ADR alignment review, design gaps, tool/browser/ambient/harness addenda |

## Recommended Migration Order

1. PR-0: legal/provenance and implementation boundary audit.
2. PR-1: extract pure type crates for messages, tools, protocol, and runtime contracts.
3. PR-2: introduce `ToolContext` and a tool-core adapter without behavior changes.
4. PR-3: extract provider-core with split prompt, model capability, route, cost, and failover metadata.
5. PR-4: add soft interrupts and background tool safe points.
6. PR-5: add session projection journal and startup stub without replacing SQLite truth.
7. PR-6: add performance scorecards and repeatable benchmarks.
8. PR-7: harden subagent/team runtime around `WorkerSpec`, `TeamSpec`, reviewer gates, capability profiles, and TaskEvent emissions.
9. PR-8: normalize jcode-inspired tool families into Capability Mesh cards, starting with search/read/write/patch/shell/background/session-search.
10. PR-9: formalize BrowserProvider around uClaw Browser Agent v2 while borrowing jcode-style readiness/setup/status probes.
11. PR-10: map ambient/scheduled/background work to uClaw automation runtime and heartbeat automations, not to a parallel ambient loop.
12. PR-11: extend uClaw harness with jcode-style tool smoke harnesses and performance campaigns.
13. PR-12: normalize frontend runtime events into a single session projection reducer.
14. PR-13: converge Agent, Chat, Browser, Automation, Symphony, and Team views on `TaskEvent -> WorldProjection`.

## Non-Negotiable Guardrails

- Do not write new `memory_graph` flows.
- Do not bypass uClaw's safety/path policy.
- Do not introduce a second backend control plane.
- Do not replace Agent OS v2 contracts with jcode daemon/session semantics.
- Do not copy jcode UI/TUI literally into the React app.
- Do not move behavior before type boundaries are extracted and tested.
- Do not import jcode ambient as a second scheduler; translate it into uClaw Automation + TaskEvent + human boundary policy.
- Do not import jcode browser as the main browser stack; uClaw Browser Agent v2 is closer to the ADR target.

## Verification Performed

- Refreshed the GitNexus index for `uclaw-new` with `npx gitnexus analyze`.
- Compared Cargo/workspace topology for both repositories.
- Inspected key Rust backend, protocol, tool, provider, safety, session, runtime, and frontend bridge files.
- Rewrote the five original report files as docs-only analysis.
- Added the second-pass ADR gap audit and reference addenda.

No Rust or frontend implementation was changed by this analysis package.
