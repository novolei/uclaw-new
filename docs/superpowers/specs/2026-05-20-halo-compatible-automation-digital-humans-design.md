# Halo-Compatible Automation Digital Humans Design

## Context

uClaw already has the right foundations for digital-human automation: `automation_specs` as the installed app truth table, the AppRuntimeService, activity/run-session logging, IM channel dispatch, browser tools, and the dedicated live-room moderator runtime. The current shape is powerful but fragmented. Halo's digital-human implementation is more productized: a built-in app bundle is installed from resources, skills are materialized into `.claude/skills`, the runtime builds a scoped MCP/tool surface for each run, the frontend presents "My Digital Humans", and IM sessions are isolated per app/channel/chat.

The goal is to replicate Halo's digital-human capability in uClaw without discarding uClaw's current runtime investments. The safest design is Halo-compatible, not Halo-replacing: uClaw should accept Halo-style built-in app specs and skills, expose a Halo-compatible `ai-browser` facade, and reshape automation UX/runtime semantics to match Halo where they define useful product behavior.

## Brainstorming Outcome

Three approaches were evaluated:

1. **Direct Halo port:** copy Halo AppManager, runtime, ai-browser, IM, and UI code nearly verbatim. This is fastest only on paper. It collides with uClaw's Rust/Tauri runtime, existing `automation_specs`, current IM channels, and browser agent architecture.
2. **Facade-first compatibility:** preserve uClaw truth sources and services, then add Halo-compatible adapters: built-in app bundle loader, `.claude/skills` materialization, `ai-browser` tool names, app-chat/IM session semantics, permission overrides, and Halo-like frontend screens. This gives the same user-visible capability while keeping migrations additive.
3. **Spec-only import:** only import Halo specs/skills and ask the existing automation runtime to run them. This is too shallow because Halo specs depend on `browser_run`, app memory, report tools, IM routing, login notices, and permissions.

The recommended approach is **Facade-first compatibility**. It is the least disruptive path that still lets uClaw run Halo-style digital humans successfully.

## Compatibility Boundaries

uClaw will become compatible with the following Halo concepts:

- Built-in automation bundles under a manifest, with `spec.yaml` plus bundled `skills/<skill>/index.js`.
- Skill materialization to a run workspace `.claude/skills/<skill>/`.
- A Halo-compatible `ai-browser` facade with tool names such as `browser_navigate`, `browser_wait_for`, `browser_snapshot`, `browser_evaluate`, `browser_run`, `browser_tab`, and `browser_download`.
- App statuses and user overrides equivalent to Halo's `active`, `paused`, `error`, `needs_login`, `waiting_user`, `uninstalled`, frequency, notification level, model source/model id, and dismissed login notice.
- IM sessions isolated by app, channel type, chat type or chat id.
- Permission resolution where explicit user deny wins, explicit grant wins, then spec-declared defaults apply.
- Frontend navigation equivalent to Halo's app list, header actions, Chat/Dynamic/Settings tabs, YAML view, login notices, runtime toggles, and message channel settings.

uClaw will keep these canonical truths:

- `automation_specs` remains the installed automation truth table. New compatibility fields are additive; no destructive table replacement is allowed.
- Existing `live_room` runtime remains the domain-specific engine for Douyin and future live platforms.
- Existing `channels` framework remains the IM transport layer.
- Existing `BrowserContextManager` and browser tab/session model remain the browser execution layer.
- Existing gbrain room/platform-scoped knowledge isolation remains mandatory for live-room specs.

## Architecture

The architecture has five layers:

1. **Bundle Layer:** loads built-in digital humans from `src-tauri/resources/builtin-automations/manifest.json`, copies bundled skills, installs or refreshes specs, and preserves user state.
2. **Compatibility Model Layer:** maps Halo app fields onto uClaw `automation_specs`, including status, overrides, permissions, login requirements, notification settings, and source metadata.
3. **Runtime Layer:** builds a scoped run context, resolves permissions, registers tools, materializes skills into the run workspace, executes scheduled/manual/IM/live-room triggers, and requires a final report.
4. **Browser Facade Layer:** exposes Halo-compatible `ai-browser` tools backed by uClaw's `BrowserContextManager`, including real page-context execution for `browser_run`.
5. **Frontend Layer:** presents automation as "digital humans" using Halo's user flow while preserving uClaw activity, run-session, marketplace, and live-room features.

## AI Browser Design

The Halo-compatible AI browser is an in-process facade, not an external MCP process at first. It should expose the same tool contract that Halo specs expect while delegating to uClaw browser primitives. A later MCP server can wrap the same facade if uClaw needs external MCP clients.

`browser_run` is the critical missing behavior. It must:

- Resolve script paths through the same allowlist policy used by `browser_run_script`.
- Allow `.claude/skills/<skill>/index.js`, built-in automation resources, and marketplace-installed skills copied into a safe workspace.
- Read the JavaScript file and execute it in the active browser page context with JSON params.
- Return structured JSON or a structured error.
- Use a per-run browser session id so concurrent specs cannot steal each other's active tabs.

This facade makes Halo-style Bilibili and future Douyin scripts portable while still using uClaw's browser identity, login state, and visual/browser-agent tools.

## Runtime Design

Every automation run receives a `RunContext` containing spec id, activity id, session id, trigger payload, workspace root, permissions, resolved model, channel handles, browser scope, and app handle. The runtime builds its tool registry from this context.

Scheduled/manual runs should behave like Halo proactive runs. IM-triggered runs should behave like Halo app-chat runs, using an identity key derived from app id, channel type, and chat id. Live-room runs keep their specialized loop but receive the same browser facade and permission resolver.

All runs must end with a report. Live-room specs also end automatically when the page indicates the live room has ended, or when the app user stops the spec. End reports include duration, trigger source, actions taken, moderation events, replies sent, knowledge items extracted, skipped comments, warnings, errors, and stop reason.

## IM Design

uClaw already has bidirectional IM channel instances, inbound dispatch, per-user session registry, permissions, and reply handles. The Halo-compatible layer should extend this instead of replacing it:

- Automation IM identity key becomes `app-chat:{specId}:{channelType}:{chatId}` for app-specific isolation.
- Trigger phrase routing remains supported for existing specs.
- Halo-style app chat is added for selected digital humans, with stop/clear commands and optional supplement buffering.
- `notify_channel` and `notify_bot` tools are added as explicit permissioned tools so agents can push summaries or alerts to configured channels.
- Guest policy remains strict: external IM users do not receive shell, destructive tools, or AI browser by default.

## Permission And Credential Design

Credential handling must follow Halo's pattern: browser login state lives in the browser session, credentials are not stored in automation specs, and login prompts are product UI state. Browser-login requirements are declared by specs through `browser_login`.

Permission resolution:

1. Explicit denied permission in the installed row returns false.
2. Explicit granted permission in the installed row returns true.
3. Spec-declared permission returns true.
4. High-risk tools default false unless the spec declares them and the user has enabled the app.

High-risk permissions include shell, filesystem write, AI browser, email send, IM notify/send, marketplace MCP, and destructive live-room moderation actions. Douyin room-manager actions may default to real execution only for the built-in live-room moderator spec because the user explicitly chose this behavior; the UI must still show the permission clearly.

## Frontend Design

The automation surface becomes a Halo-like digital-human product:

- Left sidebar: "我的数字人", running count, built-in apps, marketplace apps, create/import action.
- Header: icon, name, status, last run, workspace, run/pause/browser actions.
- Tabs: Chat, Dynamic, Settings.
- Settings subtabs: visual settings and YAML.
- Settings sections: schedule, model, AI browser, email, IM push, required login, notification importance, message channels, config schema, developer information, danger zone.
- Login notice: shown when `browser_login` is declared and not dismissed.
- Dynamic tab: activity feed plus run report details.
- Chat tab: local app chat and IM conversation thread list.

This is a product reshaping rather than a new landing page.

## Migration And保全 Principles

- No destructive migration is allowed for `automation_specs`.
- Existing Douyin live-room spec and room/platform-scoped gbrain isolation must keep working.
- Existing marketplace installs must remain listable, configurable, runnable, and uninstallable.
- Browser script execution must be behind path policy and permissions.
- IM-triggered runs must treat external messages as untrusted input.
- Built-in refresh must preserve user config, enabled status, permissions, uninstalled state, and last run history.
- Direct Halo code should be ported conceptually, not blindly copied across language/runtime boundaries.

## Risks

- **Browser execution risk:** `browser_run` changes a currently inert validation tool into real page execution. Mitigation: strict path policy, active tab scoping, timeout, structured audit result, and tests on local fixture pages.
- **Migration risk:** Halo's installed app model is richer than uClaw's current rows. Mitigation: additive columns or side tables only.
- **IM prompt-injection risk:** external users can send arbitrary text. Mitigation: current trust-boundary system prompt remains, high-risk permissions stay denied for guest chat, and app-specific permission checks are explicit.
- **Concurrent spec risk:** multiple specs can fight over browser active tabs. Mitigation: per-run browser session scope and explicit tab ids in facade results.
- **Live-room destructive-action risk:** real mute/remove actions can damage the room if classifiers are wrong. Mitigation: event log, thresholds, rate limits, room-scoped policy, UI-visible permission, and stop control.

## Success Criteria

- uClaw ships a built-in Halo-style Bilibili comment auto-reply digital human and a Douyin live-room moderator digital human.
- Halo's Bilibili spec shape can run in uClaw with `.claude/skills` scripts and `browser_run`.
- The Douyin live-room spec can declare platform and room, run concurrently with other specs, isolate gbrain knowledge by platform/room, stop on live-ended page signal, and produce a complete final report.
- IM-triggered automation uses app-specific sessions and respects permissions.
- The frontend lets a final user discover, configure, run, pause, chat with, inspect, and uninstall built-in digital humans without looking for YAML paths.
