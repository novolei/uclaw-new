# gbrain Sprint 2.2 followon #3 — `/v1/embeddings` endpoint + MCP timeout/zombie fixes

**Status:** ready to merge (assuming manual round-trip verify is green on Mac).
**Branch:** `worktree-gbrain-embeddings-endpoint`
**Base:** `main`
**Predecessor work:** PR #205 (init fix), PR #207 (launcher), PR #209 (hygiene polish), PR #212 (diagnostics — open).

## Why this PR exists

Live debugging surfaced THREE distinct production bugs all manifesting as
"gbrain MCP appears to hang / be down":

1. **`put_page` taking 60s+ per call.** Root cause: gbrain has no
   embedding API key configured (user has DeepSeek/Moonshot — both chat-only),
   so `gbrain config show` defaulted `embedding_model = text-embedding-3-large`
   and `put_page` tried to call OpenAI with no key → 401 retries → 60s+ hang.
   Manual repro: 90s probe still returned `"PGlite is closing"` because
   gbrain was still embedding when stdin EOF triggered shutdown.

2. **uClaw kills in-flight tool calls via health-ping reconnect.** The
   per-server health loop pings every 60s. `send_request` had a hardcoded
   60s timeout. When `put_page` is mid-flight (legitimately >30s), the
   60s health ping hits, gbrain (single-threaded MCP) can't respond
   because it's busy, ping times out, the health loop calls
   `reconnect_server_shared` which kills the connection — taking the
   `put_page` with it. Cascade: every slow tool call → ping collision
   → reconnect → another zombie gbrain child holding PGLite write lock.

3. **gbrain children leaked on connect timeout.** `StdioTransport::spawn`
   used `tokio::process::Command::spawn` without `kill_on_drop(true)`.
   When `McpConnection` drops (timeout, manager-side disconnect, etc.),
   the child process keeps running. For gbrain specifically, the orphan
   keeps holding PGLite's single-writer lock → next connect attempt's
   freshly-spawned gbrain hits `"Timed out waiting for PGLite lock"`
   → 60s timeout → another orphan. Compounds with #2.

## What changed

| File | Diff |
|---|---|
| `src-tauri/src/local_api/routes.rs` | +`POST /v1/embeddings` OpenAI-compatible handler → `MemUClient::embed_text` (FastEmbed bge-small-en-v1.5, 384 dim); +5 unit tests pinning input-validation paths; `ApiState` extended with `memu_client: Option<Arc<MemUClient>>` |
| `src-tauri/src/local_api/server.rs` | `LocalApiService::new` signature extended `(config, memu_client)`; threads through to `ApiState` |
| `src-tauri/src/main.rs` | Stage 3 call site updated to pass `memu_client.clone()` |
| `src-tauri/src/mcp.rs` | `kill_on_drop(true)` on `StdioTransport::spawn`'s `Command` builder (prevents orphan MCP children); `send_request`'s 60s hardcoded timeout split: `tools/call` gets 300s, everything else (initialize, ping, tools/list, notifications) keeps 60s |

Net: +369 / -14 across 4 files.

## How to use the embeddings endpoint (gbrain config)

After this PR ships and you've restarted uClaw (which spins up the
embeddings endpoint at `http://localhost:27270/v1/embeddings` per the
existing `LocalApiConfig.port`), point gbrain at it via three config
commands using the bundled launcher:

```bash
~/.uclaw/gbrain/run.sh config set embedding_model llama-server:bge-small-en-v1.5
~/.uclaw/gbrain/run.sh config set embedding_dimensions 384
~/.uclaw/gbrain/run.sh config set base_urls.llama-server http://localhost:27270/v1
```

After that, `put_page` calls gbrain → llama-server recipe → uClaw's
`/v1/embeddings` → memU's FastEmbed (already-loaded model, hot cache) →
~100ms per chunk instead of ~30-60s per call. No external API key needed.

## Alternatives (also documented in the handler's source comment)

- **Multilingual recall:** the bundled FastEmbed model
  (`BAAI/bge-small-en-v1.5`) is English-focused. For better Chinese
  recall, set `FASTEMBED_MODEL=bge-m3` in the memU bridge env (or
  whichever multilingual model FastEmbed supports) — both memU and the
  `/v1/embeddings` endpoint then use the same multilingual model.
- **Skip embedding entirely:** unset gbrain's `embedding_model` via
  `~/.uclaw/gbrain/run.sh config unset embedding_model` — `put_page`
  will then store pages without semantic vectors. Keyword search +
  graph navigation still work; `query`'s hybrid path degrades to
  keyword-only.
- **External provider:** any `openai-compatible` recipe (OpenAI proper,
  Voyage, ZeroEntropy, etc.) — `/v1/embeddings` here becomes unused.

## How `/v1/embeddings` is disabled

Two paths:

1. Disable the whole local API (`memubot_config.local_api.enabled = false`)
   — same surface that has been there since memubot infra landed.
2. Leave local API on but configure gbrain to use a different embedding
   provider (or no embedding) — the route stays registered but unused.

There's no third option to "expose local API but disable just the
embeddings sub-route" — that level of granularity isn't worth the
config surface area. If it becomes a real ask, add per-route flags.

## How to verify locally

```bash
# Build
cd ~/Documents/uclaw && cargo build --manifest-path src-tauri/Cargo.toml
cd src-tauri && cargo test --lib openai_embeddings_tests
# expect: 5 passed

# Manual round-trip (uClaw must be running with this build):
curl -s -X POST http://localhost:27270/v1/embeddings \
  -H 'Content-Type: application/json' \
  -d '{"input":["hello world","OpenAI 在 2025 年发布了 GPT-5"],"model":"bge-small-en-v1.5"}' \
  | python3 -c 'import sys,json; d=json.load(sys.stdin); print("data len:", len(d["data"])); print("dim:", len(d["data"][0]["embedding"])); print("usage:", d["usage"])'
# expect:
#   data len: 2
#   dim: 384
#   usage: {'prompt_tokens': N, 'total_tokens': N}

# Failure mode: memU not configured
# Should return 503 with body {"error":{"message":"...","type":"server_error","code":"memu_unavailable"}}
```

Then configure gbrain (see "How to use" above) and exercise a real
`put_page`:

```bash
~/.uclaw/gbrain/run.sh recall "GPT-5"  # should return your saved facts
```

`put_page` from the agent (uClaw chat) should complete in <5s for short
content instead of timing out at 60s.

## What's still out of scope

- **bun stdout buffering in MCP responses** — the original `script -q /dev/null`
  workaround in `~/.uclaw/mcp_servers.json` remains. The MCP SDK's
  `process.stdout.write(json)` in gbrain's stdio transport is
  block-buffered when piped (verified — response only flushed at stdin
  EOF). Real fix is to swap to `fs.writeSync(1, json)` in the SDK or
  in gbrain's wrapping. User has a fork at
  `https://github.com/novolei/gbrain.git` if/when this becomes worth
  fixing upstream. Current PTY workaround works fine for now.
- **UI for `LocalApiConfig.enabled` / `port`** — the schema is already
  in place (`memubot_config.local_api.{enabled, port}`); a settings
  panel surface isn't wired yet. Separate PR if/when the user wants
  GUI-driven control.
- **Eager memU spawn at boot** — already shipped via PR #212.

## Commits (bisectable)

| sha | summary |
|---|---|
| `<HEAD>` | feat(local_api): OpenAI-compatible /v1/embeddings + MCP kill_on_drop + tool-call timeout |

Single commit (changes are tightly coupled — endpoint needs memU
threading, MCP fixes prevent the failure mode that motivated the
endpoint).
