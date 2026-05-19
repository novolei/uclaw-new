# Browser Identity Broker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let browser tasks reuse real authorized user browser state through profile handles and Playwright-compatible storage state, without building a custom identity/session system.

**Architecture:** Add a `browser::identity` layer that imports, stores, validates, and attaches auth profiles to browser contexts. Secrets are stored through system keyring/Keychain handles; normal traces and chat records only see profile IDs and status.

**Tech Stack:** Rust/Tauri v2, Serde, Playwright `storageState` JSON format, system keyring via Rust `keyring` crate or existing secret storage if one is already available, existing `src-tauri/src/browser/*`.

---

## File Structure

Create:

- `src-tauri/src/browser/identity/mod.rs` — public identity broker API.
- `src-tauri/src/browser/identity/types.rs` — profile and storage state structs.
- `src-tauri/src/browser/identity/playwright_state.rs` — parse/validate Playwright storage state.
- `src-tauri/src/browser/identity/profile_store.rs` — metadata store and profile lookup.
- `src-tauri/src/browser/identity/keyring_store.rs` — secret handle abstraction.
- `ui/src/atoms/browser-identity-atoms.ts` — frontend state projection.
- `ui/src/components/browser/identity/BrowserIdentitySettings.tsx` — profile management surface.

Modify:

- `src-tauri/src/browser/mod.rs` — export `identity`.
- `src-tauri/src/browser/context_manager.rs` — accept optional identity profile handle when creating contexts.
- `src-tauri/src/browser/agent_loop.rs` — select a matching authorized profile before task execution.
- `src-tauri/src/tauri_commands.rs` — add list/import/delete/verify profile commands.

---

## Task 1: Playwright Storage State Types

- [ ] **Step 1: Write JSON parsing tests**

Use a fixture with:

```json
{
  "cookies": [
    {
      "name": "sid",
      "value": "abc",
      "domain": ".example.com",
      "path": "/",
      "expires": 1893456000,
      "httpOnly": true,
      "secure": true,
      "sameSite": "Lax"
    }
  ],
  "origins": [
    {
      "origin": "https://example.com",
      "localStorage": [{ "name": "theme", "value": "dark" }]
    }
  ]
}
```

- [ ] **Step 2: Implement storage state structs**

Serde structs must round-trip the fixture and preserve unknown-safe optional fields.

- [ ] **Step 3: Verify**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::identity::playwright_state --lib
```

---

## Task 2: Profile Store and Secret Handles

- [ ] **Step 1: Write profile metadata tests**

Test create/list/delete and origin pattern matching.

- [ ] **Step 2: Implement metadata store**

Store non-secret metadata under app data. Store auth state through secret handle only.

- [ ] **Step 3: Verify no secret leakage**

Profile list responses must include `secretHandle` or redacted status, never raw cookie values or tokens.

---

## Task 3: Browser Context Restore

- [ ] **Step 1: Add context-manager test seam**

Create a test that passes a profile ID and asserts the resolved context config includes a storage state ref.

- [ ] **Step 2: Attach profile to browser context**

Use existing Chromium/CDP context creation path. Do not fork a second browser runtime.

- [ ] **Step 3: Verify stale auth boundary**

When validation fails, emit `auth_profile_stale` and checkpoint instead of silently retrying login.

---

## Task 4: Frontend Management Surface

- [ ] **Step 1: Add atoms**

Represent profile metadata, verification status, and import/delete pending state.

- [ ] **Step 2: Add settings component**

The UI should show profile label, origin, scope, last verified time, and stale/live status.

- [ ] **Step 3: Verify**

Run:

```bash
npm run test -- browser-identity
npm run build
```

---

## Out of Scope

- CAPTCHA solving.
- OCR/VLM perception.
- Browser harness suites.
- Automatic import from locked OS Chrome profile without explicit user action.

Those should be separate PRs after the core identity broker is in place.
