//! C1.5 50-turn refactor benchmark binary — **bench-only**
//! (`required-features = ["bench"]`, excluded from default/release builds).
//!
//! Usage:
//!   bench_50turn --fixture refactor-8-file --mode replay --golden post --out post.json
//!   bench_50turn --fixture refactor-8-file --mode replay --golden pre  --out pre.json
//!   bench_50turn --fixture refactor-8-file --mode live --provider deepseek --runs 3 --out live-ds.json
//!
//! Replay mode is deterministic + free (no network). Live mode hits a real
//! provider and harvests `TokenUsage`; it exits non-zero with a clear
//! "missing <PROVIDER>_API_KEY" message if the key is absent (never zeros).

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let fixture = arg(&args, "--fixture").unwrap_or_else(|| "refactor-8-file".into());
    let mode = arg(&args, "--mode").unwrap_or_else(|| "replay".into());
    let golden = arg(&args, "--golden").unwrap_or_else(|| "post".into());
    let out = arg(&args, "--out");

    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/c1.5-bench")
        .join(&fixture);

    if !fixture_dir.is_dir() {
        eprintln!("fixture not found: {}", fixture_dir.display());
        std::process::exit(2);
    }

    match mode.as_str() {
        "replay" => {
            if golden != "pre" && golden != "post" {
                eprintln!("--golden must be 'pre' or 'post', got '{golden}'");
                std::process::exit(2);
            }
            let report = uclaw_core::agent::bench::replay(&fixture_dir, &golden);
            emit(&report, out);
        }
        "live" => {
            let provider = arg(&args, "--provider").unwrap_or_else(|| {
                eprintln!("--provider required for live mode (deepseek|anthropic)");
                std::process::exit(2);
            });
            let runs: u32 = arg(&args, "--runs").and_then(|s| s.parse().ok()).unwrap_or(3);
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("failed to start tokio runtime: {e}");
                    std::process::exit(1);
                }
            };
            let report = rt.block_on(uclaw_core::agent::bench::live_run(&fixture_dir, &provider, runs));
            emit(&report, out);
        }
        other => {
            eprintln!("unknown --mode '{other}' (expected replay|live)");
            std::process::exit(2);
        }
    }
}

/// Serialize `report` to pretty JSON, writing to `--out` or stdout.
fn emit<T: serde::Serialize>(report: &T, out: Option<String>) {
    let json = serde_json::to_string_pretty(report).expect("serialize report");
    match out {
        Some(p) => std::fs::write(&p, json).unwrap_or_else(|e| {
            eprintln!("write {p}: {e}");
            std::process::exit(1);
        }),
        None => println!("{json}"),
    }
}

/// Tiny `--key value` arg parser (no clap dependency for a bench bin).
fn arg(a: &[String], k: &str) -> Option<String> {
    a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned()
}
