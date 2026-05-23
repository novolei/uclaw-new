//! Provider management module.
//!
//! Provides built-in provider registry, multi-provider configuration
//! persistence, model discovery, and connection testing.
//!
//! Ported from if2Ai project with adaptations for uclaw's architecture.

pub mod types;
pub mod registry;
pub mod readiness;
pub mod store;
pub mod service;
