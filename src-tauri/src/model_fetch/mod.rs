// SPDX-License-Identifier: Apache-2.0
//! Manifest-driven model downloader, shared by the bge embedder and the
//! local MiniCPM engine. A `ModelManifest` lists files (each with its own
//! ordered candidate source URLs); a later task adds the streaming downloader
//! and a host-latency probe so the fastest reachable mirror wins.

pub mod manifest;
pub use manifest::{FileSource, Host, ManifestFile, ModelManifest};

pub mod probe;
pub use probe::{rank_hosts, HttpProbe, ProbeResult, SourceProbe};

pub mod download;
pub use download::{download_manifest, DownloadEvent, Progress};
