// SPDX-License-Identifier: Apache-2.0
//! Summariser factory. Production always builds the LLM summariser (it
//! resolves the provider lazily and degrades gracefully — erroring at
//! summarise time when no provider is configured). Tests inject
//! `InertSummariser` directly via the adapter constructor.

use std::sync::Arc;

use crate::providers::service::ProviderService;

use super::{LlmSummariser, Summariser};

/// Build the production summariser. The LLM summariser resolves the
/// ingestion provider per call; if none is configured it errors at
/// summarise time (the detached cascade logs + drops — best-effort).
pub fn build_summariser(provider_service: Arc<ProviderService>) -> Arc<dyn Summariser> {
    Arc::new(LlmSummariser::new(provider_service))
}
