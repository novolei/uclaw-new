//! Live-mode bench (real provider). Placeholder — implemented in Task 3.
//! Bench-only (`feature = "bench"`).

use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize, Default)]
pub struct LiveReport {}

#[allow(unused_variables)]
pub async fn live_run(fixture_dir: &Path, provider_name: &str, runs: u32) -> LiveReport {
    unimplemented!("live mode lands in Task 3")
}
