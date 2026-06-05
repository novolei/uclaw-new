---
name: plugin-author
version: "1.0.0"
description: Author and install a uClaw plugin (a stdio MCP server + plugin.toml) from a natural-language request.
author: uclaw
enabled: true
category: development
activation:
  keywords:
    - plugin
    - 插件
    - "make a plugin"
    - "create a plugin"
    - scaffold
    - "add a tool"
    - "add a command"
  patterns:
    - "(?i)\\b(make|create|build|write|scaffold)\\b.*\\bplugin\\b"
    - "(?i)(做|写|生成|创建).*插件"
  tags:
    - plugin
    - extension
    - mcp
  exclude_keywords: []
  max_context_tokens: 3000
parameters: []
---

# Authoring a uClaw plugin

When the user asks you to create a plugin / a new tool or slash command, author a
**self-contained** uClaw plugin and install it with the `install_plugin` tool.

## What a uClaw plugin is
A directory `<id>/` containing:
- `plugin.toml` — the manifest (the dir name MUST equal `id`)
- a stdio MCP server executable (e.g. `server.mjs`, Node)

A plugin contributes **tools** (agent-callable), **commands** (`/name`, user-typed), and/or
**skills**. Tools and commands both route to a `tools/call` on the server by name.

## plugin.toml format (example)
```toml
id = "weather"                 # kebab-case; MUST equal the directory name
version = "0.1.0"
display_name = "Weather"
description = "Look up weather for a city."

[author]
name = "uClaw user"

[runtime]
min_uclaw_version = "0.1.0"
kind = "subprocess"
executable = "server.mjs"

[permissions]                  # request the MINIMUM needed; the sandbox enforces these
run_subprocess = true
network = true                 # only if the server makes network calls
# filesystem_read = true / filesystem_write = true — only if needed

[contributes]
tools = ["weather"]            # agent-callable tools (names = tools/call names)
commands = ["weather"]         # /weather slash command (routes to the same tools/call)
mcp_servers = ["weather"]      # the server itself
```

## stdio MCP server contract (copy-paste template)
Line-delimited JSON-RPC 2.0 on stdin/stdout. Handle `initialize`, `tools/list`, `tools/call`;
ignore `notifications/*`. **Self-contained — Node builtins only, no `npm install`.**
```javascript
#!/usr/bin/env node
import readline from "readline";
const rl = readline.createInterface({ input: process.stdin, terminal: false });
const reply = (o) => process.stdout.write(JSON.stringify(o) + "\n");
rl.on("line", async (raw) => {
  const line = raw.trim(); if (!line) return;
  let req; try { req = JSON.parse(line); } catch { return reply({ jsonrpc:"2.0", id:null, error:{ code:-32700, message:"parse error" } }); }
  const { id, method, params } = req;
  if (typeof method === "string" && method.startsWith("notifications/")) return;
  switch (method) {
    case "initialize":
      return reply({ jsonrpc:"2.0", id, result:{ protocolVersion:"2024-11-05", capabilities:{ tools:{} }, serverInfo:{ name:"weather", version:"0.1.0" } } });
    case "tools/list":
      return reply({ jsonrpc:"2.0", id, result:{ tools:[ { name:"weather", description:"Get weather for a city.", inputSchema:{ type:"object", properties:{ city:{ type:"string" } }, required:[] } } ] } });
    case "tools/call": {
      const name = params?.name;
      // `/weather <city>` arrives as arguments.args; agent tool calls pass arguments.city
      const city = (params?.arguments?.city ?? params?.arguments?.args ?? "").trim() || "your area";
      if (name === "weather") {
        // example: a real impl would `fetch` a weather API (needs network permission)
        return reply({ jsonrpc:"2.0", id, result:{ content:[{ type:"text", text:`Weather for ${city}: (stub)` }] } });
      }
      return reply({ jsonrpc:"2.0", id, error:{ code:-32601, message:`tool not found: ${name}` } });
    }
    default:
      return reply({ jsonrpc:"2.0", id, error:{ code:-32601, message:"method not found" } });
  }
});
```

## Workflow (follow this ritual)
1. **Clarify** what the plugin should do — its tool(s)/command(s), inputs, and whether it needs network/filesystem.
2. **Pick a kebab-case `id`** (e.g. `weather`).
3. **Scaffold to a temp dir** `<tmpdir>/<id>/` (use the bash tool, e.g. under `/tmp`): write `plugin.toml` + the server (`server.mjs`). Keep it **self-contained** (Node builtins only).
4. **Self-check**: run the server with a probe initialize and confirm a valid reply:
   `printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | node <tmpdir>/<id>/server.mjs` → expect a JSON result with `serverInfo`.
5. **Summarize** the `plugin.toml` + the requested permissions to the user (so they know what they're approving).
6. **Install**: call the `install_plugin` tool with `{ "dir": "<tmpdir>/<id>" }`. The user will be asked to approve.
7. **Tell the user to restart uClaw** to activate the plugin's tools/commands (registration is boot-time).

## Rules
- Request the **minimum** permissions; the macOS sandbox enforces them (no network unless `network=true`, writes jailed unless `filesystem_write=true`).
- **Self-contained only** in v1 — do not rely on `npm install` / external packages.
- If `install_plugin` reports an error (e.g. id already installed, invalid plugin.toml), fix it and retry.
