# Bundle 17-D — `summarize_to_fold` Resilience (multi-provider + heuristic fallback)

> **Context**: Follow-up to C1.1 PR-1 (Bundle 17-B `/compact` fold-delta path).
> Live E2E on 2026-05-22 showed `summarize_to_fold` is the brittle link
> in the M2-D wire-up — 2 of 3 `/compact` attempts on session
> `78c1d9fd` failed at the LLM tier, never reaching the delta-path
> code we shipped in PR #397.
>
> Queue position: **C1.5** (inserted between current C1.4 and the
> 50-turn benchmark). Without this, C1.5 bench results will be
> contaminated by LLM availability noise.

## 1. Background — observed failure modes

PR #397 ships `tauri_commands.rs::/compact intercept` with this flow:

```
1. mark messages compacted (sync, DB lock)
2. call summarize_to_fold(history) → StructuredFold   ← brittle
3. baseline::load + decide_placeholder + baseline::upsert
4. insert placeholder + bump message_count (sync, DB lock)
```

Step 2 is the single failure point. Today's behavior: any `Err` from
the LLM call (timeout, rejection, parse failure) falls through to the
**legacy placeholder** path — meaning the user still gets `/compact`
to complete, but loses the structured fold AND never seeds the
baseline AND can't exercise the delta path. The soft-fail is *correct*
but it makes the M2-D wire-up an unreliable win.

Production evidence from 2026-05-22 14:33–14:59 (session `78c1d9fd`):

| Attempt | Time | LLM outcome | Path taken |
|---|---|---|---|
| 1 | 06:49:05 → 06:49:06 (1.0s) | `OpenAI API error: rejected because high risk` | legacy placeholder, no baseline |
| 2 | 06:50:47 → 06:51:52 (65s) | success, 3 facts / 0 decisions / 1 next_action | **FullRewrite** path, baseline upserted ✓ |
| 3 | 06:56:18 → 06:59:43 (3m25s) | 2x timeout + retry → empty response → `EOF parsing JSON line 1 col 0` | legacy placeholder, baseline untouched |

Failure rate **2/3** on a single session in one hour. The "high risk"
rejection was triggered by personal-content messages ("我想我儿子了 / 他在天津");
the timeout was an upstream OpenAI / DeepSeek hiccup independent of
content.

## 2. Goal

Make `summarize_to_fold` resilient enough that the delta path actually
fires under realistic LLM-availability conditions. Specifically:

- After 17-D ships, **≥ 90% of `/compact` invocations should successfully
  produce a `StructuredFold`** (even if degraded), so the delta path
  exercises and seeds baselines reliably.
- **0% of failures should leave the user with a worse experience** than
  today's legacy placeholder fallback — we never regress the floor.

## 3. Design — two-tier resilience

### Tier 1: multi-provider fallback chain

`summarize_to_fold(history, llm_chain)` walks an ordered list of LLM
providers, returning on first success. On rejection-class or
timeout-class errors, it advances to the next provider; on truly fatal
errors (no providers configured, all rejected, malformed input) it
returns `Err` to preserve the current soft-fail floor.

**Provider chain config** lives in `memubot_config.rs`:

```rust
pub struct ContextConfig {
    // ... existing fields ...

    /// Bundle 17-D — ordered list of LLM provider IDs to try for
    /// `/compact` fold summarization. First success wins. Empty list
    /// or all-rejected → fall back to heuristic extractive (Tier 2).
    ///
    /// Default: `["anthropic", "deepseek", "kimi", "openai"]` — order
    /// chosen so the *most permissive content classifier* runs first,
    /// reducing the worst case (cross-provider double-reject) to one
    /// long retry chain rather than two.
    #[serde(default = "default_fold_summarizer_chain")]
    pub fold_summarizer_chain: Vec<String>,
}
```

**Provider resolution**: `crate::llm::providers::resolve_by_id(&id)`
already exists for the session model picker — reuse it. Provider IDs
that aren't configured (no API key, server unreachable) are skipped
silently with a `debug!` log.

**Error classification** — new helper in `agent/compact/summarize.rs`:

```rust
fn is_retryable_on_next_provider(err: &Error) -> bool {
    matches!(err,
      // Content moderation / safety rejection — same content will be
      // rejected again by the same provider. Try a different one.
      Error::ApiRejected { reason } if reason.contains("high risk")
                                     || reason.contains("safety")
                                     || reason.contains("policy") =>
        true,
      // Network / provider-side flakiness — fast-fail and move on.
      // Retrying same provider eats budget; trying next is cheaper.
      Error::Timeout | Error::Unavailable(_) => true,
      // Parse failures often mean the LLM didn't follow the structured-
      // output instruction. Different provider may comply better.
      SummarizeError::ParseFailed { .. } => true,
      _ => false,
    )
}
```

**Per-provider timeout**: each attempt gets a short timeout (10s)
rather than the default 30s, so the chain completes within a budget
the user is willing to wait for. Total chain budget ≈ 4 × 10s = 40s
worst case.

### Tier 2: heuristic extractive fallback

When every Tier-1 provider fails, instead of returning to the empty
legacy placeholder, produce a **minimal but real StructuredFold** via
zero-LLM extraction:

```rust
fn extract_fold_heuristically(history: &[ChatMessage]) -> StructuredFold {
    StructuredFold::default()
        .with_facts(extract_facts(history))
        .with_next_actions(extract_pending_actions(history))
        .with_evidence_refs(extract_artifact_refs(history))
}
```

Each extractor is a non-LLM regex/rule pass:

- **`extract_facts`**: pull lines like "X 是 Y" / "X is Y" / file paths
  the user mentioned / decisions the assistant confirmed ("好的我会 X").
  Cap at 5 facts.
- **`extract_pending_actions`**: look for assistant messages ending in
  "我会 X" / "next step: X" / unfinished tool_use without tool_result.
  Cap at 3.
- **`extract_artifact_refs`**: file paths (`/Users/.../foo.rs`), URLs
  (`https://...`), command outputs (`bash:N`). Cap at 10, deduplicate
  by stable_key.

Result is a StructuredFold with fewer axes populated but **still
diffable, still upsertable, still cache-friendly** on subsequent
compacts. The downstream delta-path machinery treats it identically.

Optional log line:

```
[/compact] heuristic-extractive fallback (LLM unavailable)  facts=N next_actions=M evidence=K
```

## 4. Out of scope (deferred)

- **Pre-flight content moderation** (Glean / enterprise pattern). Adds
  a separate API call before every `/compact`; cost overhead doesn't
  justify itself until we see Tier-1 fallback exhaust frequently.
- **Content scrubbing + retry** (Notion AI / Slack AI pattern). Tier-1
  swap to a different provider achieves the same result for most cases;
  scrub-and-retry is a follow-up for M5 Policy Hooks territory if we
  observe Tier-1 chain exhaust on > 5% of `/compact` calls.
- **Local small-model fallback** (Apple Intelligence / Ollama). Tier-2
  heuristic extraction is a cheaper floor that doesn't depend on shipping
  a local-model runtime.
- **Chunk + summarize-of-summaries** (LangChain `MapReduceChain`).
  uClaw `/compact` already caps history at `COMPACT_KEEP_TURNS = 10`
  remaining + the rest goes in. Chunking adds complexity without clear
  payoff at this size.

## 5. Concrete commit plan

```
Commit 1: chore(spec): Bundle 17-D resilience design (this doc)
Commit 2: feat(agent/compact): heuristic extractor module + 5 unit tests
Commit 3: feat(memubot_config): fold_summarizer_chain field + default
Commit 4: feat(agent/compact): rework summarize_to_fold to walk chain + Tier-2 fallback
Commit 5: test: stub multi-provider tests covering all-reject + Tier-2 path
```

## 6. Verification

After merge:

```bash
# 1. Cargo build clean
cd src-tauri && cargo build 2>&1 | grep -E '^error'

# 2. Cargo test clean
cargo test --lib agent::compact

# 3. Live: force all Tier-1 providers to fail
~/Documents/uclaw/scripts/e2e/17d-force-tier2.sh <session_id>
# (sets fold_summarizer_chain=["nonexistent"] then triggers /compact;
#  expects "heuristic-extractive fallback" log + baseline row written)

# 4. Bench: re-run 50-turn fixture under telemetry; expect
#    - delta_applied / total_compactions ≥ 70%
#    - tier2_fallback / total_compactions ≤ 5%
```

## 7. Industry references

- **Cursor**: multi-provider fallback for code completion (default
  GPT-4o + Claude Sonnet 4 + Haiku in their docs).
- **Aider**: explicit `--weak-model` flag for cheap-but-permissive
  summarization tasks; falls back to commit-msg heuristic when all
  providers fail.
- **LangChain `ConversationSummaryBufferMemory`**: token-budget-based
  threshold, but does NOT have a multi-provider fallback by default —
  this is a known gap they push to user config.
- **OpenAI moderation API + Anthropic safety classifier**: pre-flight
  pattern used by Glean / Notion AI / Slack AI. Out of scope for
  17-D's first cut per §4.

## 8. Closes / unblocks

- Unblocks C1.6 (50-turn benchmark) — without 17-D, bench results will
  show LLM availability noise dominating the actual cache-hit signal.
- Provides telemetry foundation for retuning `fold_delta_threshold`
  from data once Bundle 17-C `FoldDeltaStats` accumulates.
- Industry-standard resilience pattern that uClaw is missing today.

## 9. Estimated effort

- ~1.5 days
- 5 commits, ~400 lines net (Tier-1 ~150, Tier-2 ~180, tests ~70)
