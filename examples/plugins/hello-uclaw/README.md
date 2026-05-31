# hello-uclaw example plugin

A minimal stdio MCP server that exposes one `hello` tool. Use it to verify the
plugin loader end-to-end without writing any Rust.

## Requirements

Node.js ≥ 18 (ESM support, `readline` built-in).

## Install into uClaw

uClaw loads plugins from `~/.uclaw/plugins/` (the `uclaw_home` data directory).

```sh
cp -r examples/plugins/hello-uclaw ~/.uclaw/plugins/hello-uclaw
```

Restart uClaw. The agent will gain the tool `mcp__hello-uclaw__hello`.

## Try the tool in the agent

```
Use the hello tool to greet Alice.
```

Expected result: `Hello, Alice!`

## Run the server manually

```sh
# Make sure it's executable (git tracks mode 755 already):
node examples/plugins/hello-uclaw/server.mjs

# Then type JSON-RPC on stdin, e.g.:
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | node server.mjs
```

## Executable approach

`server.mjs` carries a `#!/usr/bin/env node` shebang and is stored with git
mode `100755` (set via `git update-index --chmod=+x`). The manifest sets
`executable = "server.mjs"` so the registrar resolves it to
`~/.uclaw/plugins/hello-uclaw/server.mjs` (an absolute path) and spawns it
directly — no wrapper script needed.

If `node` is not on `PATH` when the system shell launches Tauri, set
`executable = "node"` and `args = ["server.mjs"]` as a fallback; both work
with the current registrar logic.
