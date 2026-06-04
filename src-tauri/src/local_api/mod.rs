mod server;
mod routes;

pub use server::LocalApiService;

/// The localhost port uClaw's in-process LocalApiService binds for the
/// OpenAI-compatible local endpoints (`/v1/chat/completions`, `/v1/models`,
/// `/v1/embeddings`). uClaw-new uses 7437 to avoid colliding with the
/// parallel uclaw-pi project (which uses 7337). Single source of truth — all
/// port/base_url references derive from this.
pub const LOCAL_API_PORT: u16 = 7437;

/// The OpenAI-compatible base URL for the in-process local API
/// (`http://localhost:<LOCAL_API_PORT>/v1`). Used as the default `base_url`
/// for the local provider + embedding endpoint, and as the in-process
/// routing sentinel.
pub fn local_api_base_url() -> String {
    format!("http://localhost:{}/v1", LOCAL_API_PORT)
}
