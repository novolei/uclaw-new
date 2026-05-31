use std::path::PathBuf;

use uclaw_core::eval::run_eval_evidence_gate_files;

#[derive(Debug, Default)]
struct Args {
    manifest: Option<PathBuf>,
    episode: Option<PathBuf>,
    report: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--manifest" => parsed.manifest = args.next().map(PathBuf::from),
            "--episode" => parsed.episode = args.next().map(PathBuf::from),
            "--report" => parsed.report = args.next().map(PathBuf::from),
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown argument: {other}\n{}", usage())),
        }
    }
    if parsed.manifest.is_none() || parsed.episode.is_none() {
        return Err(usage());
    }
    Ok(parsed)
}

fn usage() -> String {
    "usage: eval-evidence-gate --manifest <manifest.json> --episode <episode.json> [--report <report.json>]".to_string()
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let outcome = match run_eval_evidence_gate_files(
        args.manifest.as_deref().expect("validated manifest path"),
        args.episode.as_deref().expect("validated episode path"),
        args.report.as_deref(),
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&outcome.report).expect("report serializes")
    );
    std::process::exit(outcome.exit_code);
}
