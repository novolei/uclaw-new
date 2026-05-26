//! Live-mode bench (real provider). Bench-only (`feature = "bench"`).
//!
//! Sends the `task.json` refactor *prompt* (not a scripted tool sequence) to a
//! real provider K times, executing the emitted tool calls against a temp copy
//! of the fixture workspace. Harvests the provider's real `TokenUsage`
//! (input / output / cache_read / cost) and records whether the LLM actually
//! ADOPTS the Dirac borrows (multi-file batch edit / anchored edit /
//! `assume_hash` read) — the load-bearing assumption the replay path can't
//! prove on its own (spec §3.1).
//!
//! No API key → exits the process non-zero with `missing <PROVIDER>_API_KEY`.
//! It NEVER emits zeroed metrics (no-fabrication invariant, spec §12).

use serde::Serialize;
use std::path::Path;

use crate::agent::types::{calculate_cost, ContentBlock, RespondOutput};
use crate::llm::provider::CompletionConfig;
use crate::llm::LlmProvider;

use super::bench_tool_registry;

/// Averaged live-run telemetry + borrow-adoption flags.
#[derive(Debug, Serialize, Default)]
pub struct LiveReport {
    pub provider: String,
    pub model: String,
    pub runs: u32,
    pub avg_input_tokens: f64,
    pub avg_output_tokens: f64,
    pub avg_cache_read_tokens: f64,
    pub avg_cost_usd: f64,
    /// Whether any run hit the round-trip ceiling without terminating.
    pub avg_round_trips: f64,
    /// cache_read / input across all runs (the ≥0.50 DoD gate input).
    pub cache_hit_rate: f64,
    /// True iff at least one run returned token usage; when false the
    /// usage-derived metrics (input/cache/cost) are NOT measured and must be
    /// rendered as "N/A", never as a real 0.
    pub usage_returned: bool,
    /// Saw an `edit{files:[...]}` multi-file batch call (A2).
    pub adopted_batch_edit: bool,
    /// Saw an edit whose payload contains an `anchor` field (B1).
    pub adopted_anchored: bool,
    /// Saw a `read_file{...,assume_hash:...}` call (A3).
    pub adopted_assume_hash: bool,
    /// All runs ended with 0 `old_name` occurrences in the temp workspace.
    pub final_state_correct: bool,
}

/// Resolve `(model, base_url, api_key_env)` defaults for the in-scope
/// providers. Overridable via `--model` / env. deepseek + anthropic are the
/// only validated providers (spec §8.3).
fn provider_defaults(provider: &str) -> Option<(&'static str, Option<String>, &'static str)> {
    match provider {
        "deepseek" => Some(("deepseek-chat", Some("https://api.deepseek.com/v1".into()), "DEEPSEEK_API_KEY")),
        "anthropic" => Some(("claude-sonnet-4-20250514", None, "ANTHROPIC_API_KEY")),
        _ => None,
    }
}

/// Maximum tool round-trips per live run (safety ceiling; the post borrow
/// behaviour needs ~2, the legacy ~16).
const MAX_ROUND_TRIPS: usize = 30;

pub async fn live_run(fixture_dir: &Path, provider_name: &str, runs: u32) -> LiveReport {
    let Some((default_model, base_url, key_env)) = provider_defaults(provider_name) else {
        eprintln!("unknown --provider '{provider_name}' (expected deepseek|anthropic)");
        std::process::exit(2);
    };

    // No-fabrication: a missing key aborts with a clear message; we never
    // emit zeroed telemetry.
    let api_key = match std::env::var(key_env) {
        Ok(k) if !k.trim().is_empty() => k,
        _ => {
            eprintln!("missing {key_env}");
            std::process::exit(1);
        }
    };

    let model = std::env::var("BENCH_MODEL").unwrap_or_else(|_| default_model.to_string());

    // Load task prompt + rename target.
    let task: serde_json::Value = std::fs::read_to_string(fixture_dir.join("task.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| {
            eprintln!("could not read/parse {}", fixture_dir.join("task.json").display());
            std::process::exit(1);
        });
    let prompt = task["prompt"].as_str().unwrap_or("Rename old_name to new_name across the workspace.").to_string();
    let rename_from = task["rename"]["from"].as_str().unwrap_or("old_name").to_string();

    let config = CompletionConfig {
        model: model.clone(),
        max_tokens: 4096,
        temperature: 0.0,
        thinking_enabled: false,
    };

    let llm: std::sync::Arc<dyn LlmProvider> = crate::llm::create_provider(&crate::config::llm::LlmConfig {
        provider: provider_name.to_string(),
        model: model.clone(),
        api_key,
        base_url,
        max_tokens: Some(4096),
        temperature: Some(0.0),
    })
    .unwrap_or_else(|e| {
        eprintln!("create_provider failed: {e}");
        std::process::exit(1);
    });

    let mut report = LiveReport {
        provider: provider_name.to_string(),
        model,
        runs,
        final_state_correct: true,
        ..Default::default()
    };

    let (mut sum_in, mut sum_out, mut sum_cache, mut sum_cost, mut sum_rt) = (0u64, 0u64, 0u64, 0f64, 0usize);

    for run_idx in 0..runs {
        // Fresh temp copy of the fixture workspace per run.
        let tmp = std::env::temp_dir().join(format!("c1.5-live-{provider_name}-{run_idx}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        copy_dir(&fixture_dir.join("workspace"), &tmp).unwrap_or_else(|e| {
            eprintln!("copy fixture workspace failed: {e}");
            std::process::exit(1);
        });

        let registry = bench_tool_registry(&tmp);
        let tools = registry.list_definitions();
        let system_prompt = crate::agent::mode_prompts::compose_system_prompt(
            "",
            Some(tmp.as_path()),
            &crate::safety::SafetyMode::default(),
        );

        let mut history = vec![
            crate::agent::types::ChatMessage::system(&system_prompt),
            crate::agent::types::ChatMessage::user(&prompt),
        ];

        let mut round_trips = 0usize;
        loop {
            if round_trips >= MAX_ROUND_TRIPS {
                eprintln!("run {run_idx}: hit MAX_ROUND_TRIPS ceiling without terminating");
                break;
            }
            let out = llm.complete(history.clone(), tools.clone(), &config).await.unwrap_or_else(|e| {
                eprintln!("run {run_idx}: provider.complete failed: {e}");
                std::process::exit(1);
            });

            let (usage, tool_calls, text, thinking, thinking_sig) = match out {
                RespondOutput::Text { metadata, text, .. } => {
                    accumulate(&mut sum_in, &mut sum_out, &mut sum_cache, &mut sum_cost, &metadata.usage, &report.model);
                    // Terminal text — record the final assistant turn and stop.
                    history.push(crate::agent::types::ChatMessage::assistant(&text));
                    break;
                }
                RespondOutput::ToolCalls { metadata, tool_calls, text, thinking, thinking_signature } => {
                    accumulate(&mut sum_in, &mut sum_out, &mut sum_cache, &mut sum_cost, &metadata.usage, &report.model);
                    (metadata.usage, tool_calls, text, thinking, thinking_signature)
                }
            };
            let _ = (thinking, thinking_sig, usage);

            // Record the assistant turn (text preamble + each tool_use block).
            let mut assistant_blocks = Vec::new();
            if let Some(t) = &text {
                if !t.is_empty() {
                    assistant_blocks.push(ContentBlock::Text { text: t.clone() });
                }
            }
            for tc in &tool_calls {
                assistant_blocks.push(ContentBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                });
                detect_adoption(&mut report, &tc.name, &tc.arguments);
            }
            history.push(crate::agent::types::ChatMessage {
                role: crate::agent::types::MessageRole::Assistant,
                content: assistant_blocks,
                compacted: false,
            });

            // Execute each call against the temp workspace; feed results back.
            for tc in &tool_calls {
                round_trips += 1;
                let result_text = match registry.get(&tc.name) {
                    Some(tool) => match tool.execute(tc.arguments.clone()).await {
                        Ok(o) => o
                            .result
                            .get("content")
                            .and_then(|c| c.as_str())
                            .map(String::from)
                            .unwrap_or_else(|| o.result.to_string()),
                        Err(e) => format!("[error] {e:?}"),
                    },
                    None => format!("[error] unknown tool {}", tc.name),
                };
                history.push(crate::agent::types::ChatMessage::user_tool_result(&tc.id, &result_text, false));
            }
        }

        sum_rt += round_trips;

        // Final-state check: 0 `old_name` occurrences across the temp workspace.
        let remaining = count_occurrences(&tmp, &rename_from);
        if remaining != 0 {
            report.final_state_correct = false;
            eprintln!("run {run_idx}: {remaining} `{rename_from}` occurrences remain (expected 0)");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    let n = runs.max(1) as f64;
    report.avg_input_tokens = sum_in as f64 / n;
    report.avg_output_tokens = sum_out as f64 / n;
    report.avg_cache_read_tokens = sum_cache as f64 / n;
    report.avg_cost_usd = sum_cost / n;
    report.avg_round_trips = sum_rt as f64 / n;
    report.cache_hit_rate = if sum_in > 0 { sum_cache as f64 / sum_in as f64 } else { 0.0 };
    report.usage_returned = sum_in > 0;
    report
}

/// Accumulate a turn's usage into the running sums.
fn accumulate(
    sum_in: &mut u64,
    sum_out: &mut u64,
    sum_cache: &mut u64,
    sum_cost: &mut f64,
    usage: &Option<crate::agent::types::TokenUsage>,
    model: &str,
) {
    if let Some(u) = usage {
        *sum_in += u.input_tokens as u64;
        *sum_out += u.output_tokens as u64;
        *sum_cache += u.cache_read_tokens as u64;
        *sum_cost += calculate_cost(model, u.input_tokens, u.output_tokens);
    }
}

/// Inspect a tool call for Dirac-borrow adoption signals.
fn detect_adoption(report: &mut LiveReport, name: &str, args: &serde_json::Value) {
    if name == "edit" {
        if args.get("files").is_some() {
            report.adopted_batch_edit = true;
        }
        // anchored edits carry an `anchor` field, possibly nested under files[].edits[].
        if json_contains_key(args, "anchor") {
            report.adopted_anchored = true;
        }
    }
    if name == "read_file" && args.get("assume_hash").is_some() {
        report.adopted_assume_hash = true;
    }
}

/// Recursively check whether any object in `v` contains `key`.
fn json_contains_key(v: &serde_json::Value, key: &str) -> bool {
    match v {
        serde_json::Value::Object(map) => {
            map.contains_key(key) || map.values().any(|x| json_contains_key(x, key))
        }
        serde_json::Value::Array(arr) => arr.iter().any(|x| json_contains_key(x, key)),
        _ => false,
    }
}

/// Recursively copy a directory (fixture workspace → temp).
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Count total occurrences of `needle` across all files in `dir` (recursive).
fn count_occurrences(dir: &Path, needle: &str) -> usize {
    let mut count = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                count += count_occurrences(&p, needle);
            } else if let Ok(body) = std::fs::read_to_string(&p) {
                count += body.matches(needle).count();
            }
        }
    }
    count
}
