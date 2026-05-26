# Playwright MCP Workspace Root Hotfix

## Goal

Fix the built-in Playwright MCP server so it starts from the active uClaw workspace rather than inheriting the app/repo `src-tauri` cwd, and close the raw-tool bypass that lets the model call `mcp__playwright__*` directly instead of using Browser Runtime provider routing.

## Scope

- Inject a runtime working directory for the built-in `playwright` MCP server.
- Resolve that working directory from the active workspace, falling back to the default workground root.
- Restart Playwright MCP when the active workspace changes so the process root follows the user-visible workspace.
- Keep Playwright MCP tools available to the Browser Runtime adapter while hiding raw `mcp__playwright__*` tools from the agent ToolRegistry.
- Add focused Rust tests for cwd resolution and raw-tool hiding.

## Out Of Scope

- Redesigning provider UI.
- Adding full real browser execution for every unsupported CLI/MCP action.
- Changing gbrain or third-party MCP tool exposure.

## Verification

- `cargo test --lib mcp::tests::playwright`
- `cargo test --lib browser::provider_execution_tests::selected_cli_route_uses_official_adapter_without_runtime_pack`
- `cargo test --lib browser::tools::tests::direct_browser_tool_route_options_from_status_uses_config_backed_feature_flags`
