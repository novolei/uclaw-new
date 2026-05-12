//! GitHub-CLI (`gh`) integration.
//!
//! Pure subprocess wrappers around `gh pr create / view` and the URL-
//! parsing helpers needed to surface the resulting URL to the user.
//! IPC layer consumes these directly; if `gh` is missing the frontend
//! branches on [`is_gh_available`] and falls back to draft text (PR B).

pub(crate) mod pr;

use super::command::{command_exists, GH_BIN};

/// Returns `true` if the `gh` binary is reachable on `PATH`.
#[must_use]
pub(crate) fn is_gh_available() -> bool {
    command_exists(GH_BIN)
}
