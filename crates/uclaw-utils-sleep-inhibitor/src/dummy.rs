// SPDX-License-Identifier: Apache-2.0
// Derived from codex-rs/utils/sleep-inhibitor/src/dummy.rs (https://github.com/openai/codex).
// Copyright (c) OpenAI. Licensed under Apache License 2.0.
// See NOTICE in the repository root.

#[derive(Debug, Default)]
pub(crate) struct SleepInhibitor;

impl SleepInhibitor {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn acquire(&mut self) {}

    pub(crate) fn release(&mut self) {}
}
