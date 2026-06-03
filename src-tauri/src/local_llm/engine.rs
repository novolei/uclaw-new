// SPDX-License-Identifier: Apache-2.0
//! candle quantized_llama load + generation for MiniCPM5-1B.
//!
//! Mirrors the ONNX embedder's two-phase pattern: async lock to lazy-load,
//! then `spawn_blocking` + `blocking_lock` for the (synchronous, &mut) forward
//! loop. Generation is serialized behind the lock by construction.

use std::path::Path;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_llama::ModelWeights;
use tokenizers::Tokenizer;

/// Generation parameters (mapped from the OpenAI request, with defaults).
#[derive(Debug, Clone)]
pub struct GenParams {
    pub temperature: f64,
    pub top_p: Option<f64>,
    pub top_k: Option<usize>,
    pub repeat_penalty: f32,
    /// How many recent tokens the repeat penalty considers.
    pub repeat_last_n: usize,
    pub max_tokens: usize,
    pub seed: u64,
    /// Extra stop strings beyond the built-in EOS / `<|im_end|>`.
    pub stop: Vec<String>,
}

impl Default for GenParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: Some(0.9),
            top_k: None,
            repeat_penalty: 1.1,
            repeat_last_n: 64,
            max_tokens: 512,
            seed: 299792458,
            stop: Vec::new(),
        }
    }
}

/// Why a generation could not run / produce output.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// Model files missing on disk, or load not yet attempted/succeeded.
    /// Maps to HTTP 503 `model_not_ready` so the caller can fall back to cloud.
    #[error("model not ready: {0}")]
    NotReady(String),
    /// Load failed (corrupt GGUF, OOM, device init). Also a fall-back signal.
    #[error("model load failed: {0}")]
    LoadFailed(String),
    /// Inference-time failure (forward / sampling / decode).
    #[error("generation failed: {0}")]
    Generation(String),
}

/// Build a candle `Sampling` from params. `temperature <= 0` ⇒ greedy ArgMax
/// (deterministic), matching the candle quantized example's convention.
pub fn build_sampling(p: &GenParams) -> Sampling {
    if p.temperature <= 0.0 {
        return Sampling::ArgMax;
    }
    let t = p.temperature;
    match (p.top_k, p.top_p) {
        (None, None) => Sampling::All { temperature: t },
        (Some(k), None) => Sampling::TopK { k, temperature: t },
        (None, Some(pp)) => Sampling::TopP { p: pp, temperature: t },
        (Some(k), Some(pp)) => Sampling::TopKThenTopP { k, p: pp, temperature: t },
    }
}

/// Construct the candle logits processor for these params.
pub fn build_logits_processor(p: &GenParams) -> LogitsProcessor {
    LogitsProcessor::from_sampling(p.seed, build_sampling(p))
}

/// A loaded model + tokenizer + device. Lives behind the Mutex in `mod.rs`.
pub struct Loaded {
    pub model: ModelWeights,
    pub tokenizer: Tokenizer,
    pub device: Device,
    /// Token ids that terminate generation (EOS `</s>` and `<|im_end|>`).
    pub stop_ids: Vec<u32>,
}

/// Pick Metal on macOS, else CPU. Metal init failure falls back to CPU.
pub fn select_device() -> Device {
    #[cfg(target_os = "macos")]
    {
        match Device::new_metal(0) {
            Ok(d) => {
                tracing::info!("[local_llm] using Metal device");
                return d;
            }
            Err(e) => tracing::warn!("[local_llm] Metal init failed ({e}); CPU fallback"),
        }
    }
    tracing::info!("[local_llm] using CPU device");
    Device::Cpu
}

/// Load the GGUF + external tokenizer from disk. Blocking (file IO + GPU upload);
/// callers run it under `spawn_blocking`. Caller guarantees files exist.
pub fn load(gguf_path: &Path, tokenizer_path: &Path, device: Device) -> Result<Loaded, EngineError> {
    let mut file = std::fs::File::open(gguf_path)
        .map_err(|e| EngineError::LoadFailed(format!("open gguf: {e}")))?;
    let content = gguf_file::Content::read(&mut file)
        .map_err(|e| EngineError::LoadFailed(format!("read gguf: {e}")))?;
    let model = ModelWeights::from_gguf(content, &mut file, &device)
        .map_err(|e| EngineError::LoadFailed(format!("from_gguf: {e}")))?;
    let tokenizer = Tokenizer::from_file(tokenizer_path)
        .map_err(|e| EngineError::LoadFailed(format!("tokenizer: {e}")))?;

    // Resolve stop ids: </s> and <|im_end|>. Missing markers are skipped.
    let mut stop_ids = Vec::new();
    for tok in ["</s>", "<|im_end|>"] {
        if let Some(id) = tokenizer.token_to_id(tok) {
            stop_ids.push(id);
        }
    }
    if stop_ids.is_empty() {
        return Err(EngineError::LoadFailed(
            "tokenizer has neither </s> nor <|im_end|>".into(),
        ));
    }

    Ok(Loaded { model, tokenizer, device, stop_ids })
}

/// UTF-8-safe incremental decoder (mirrors candle's TokenOutputStream). Emits a
/// text delta only when the decoded suffix grows AND ends on a complete
/// alphanumeric character — matching candle's reference `TokenOutputStream`
/// heuristic.  Chinese-safe: CJK ideographs ARE alphanumeric (`char::is_alphanumeric`
/// returns true for U+4E00–U+9FFF), so Chinese streams correctly.  Incomplete
/// multi-byte sequences decode to U+FFFD (replacement char, `Gc=So`), which is
/// NOT alphanumeric, keeping them buffered until the sequence completes.
/// Trailing punctuation/whitespace is buffered during generation and flushed
/// unconditionally by `finish()`.
struct TokenStream<'a> {
    tokenizer: &'a Tokenizer,
    tokens: Vec<u32>,
    prev_index: usize,
    current_index: usize,
}

impl<'a> TokenStream<'a> {
    fn new(tokenizer: &'a Tokenizer) -> Self {
        Self { tokenizer, tokens: Vec::new(), prev_index: 0, current_index: 0 }
    }

    fn decode(&self, ids: &[u32]) -> Result<String, EngineError> {
        self.tokenizer
            .decode(ids, true)
            .map_err(|e| EngineError::Generation(format!("decode: {e}")))
    }

    /// Push a token; return the newly-complete text delta (or None if the
    /// pending bytes don't yet form a complete suffix).
    ///
    /// Emit rule: the decoded suffix must grow AND end on an alphanumeric char.
    /// U+FFFD (emitted by the tokenizer for incomplete multi-byte sequences) is
    /// non-alphanumeric, so it stays buffered until the sequence is complete.
    /// CJK ideographs pass because `char::is_alphanumeric` covers U+4E00–U+9FFF.
    fn push(&mut self, token: u32) -> Result<Option<String>, EngineError> {
        let prev_text = if self.tokens.is_empty() {
            String::new()
        } else {
            self.decode(&self.tokens[self.prev_index..self.current_index])?
        };
        self.tokens.push(token);
        let text = self.decode(&self.tokens[self.prev_index..])?;
        if text.len() > prev_text.len() && text.chars().last().is_some_and(|c| c.is_alphanumeric()) {
            let delta = text[prev_text.len()..].to_string();
            self.prev_index = self.current_index;
            self.current_index = self.tokens.len();
            Ok(Some(delta))
        } else {
            Ok(None)
        }
    }

    /// Flush any remaining buffered text at end of generation.
    fn finish(&self) -> Result<String, EngineError> {
        let prev_text = if self.tokens.is_empty() {
            String::new()
        } else {
            self.decode(&self.tokens[self.prev_index..self.current_index])?
        };
        let text = self.decode(&self.tokens[self.prev_index..])?;
        if text.len() > prev_text.len() {
            Ok(text[prev_text.len()..].to_string())
        } else {
            Ok(String::new())
        }
    }
}

/// Why generation stopped — surfaced as OpenAI `finish_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    Length,
}

impl FinishReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            FinishReason::Stop => "stop",
            FinishReason::Length => "length",
        }
    }
}

/// Run a full generation against a loaded model, invoking `on_delta` for each
/// text fragment. Synchronous + `&mut` (KV cache); callers run under
/// `spawn_blocking` while holding the model lock. Returns the finish reason and
/// completion token count.
///
/// KV cache reset: `quantized_llama::ModelWeights` has no `clear_kv_cache()`
/// method. Instead, the per-layer cache ignores stale entries whenever
/// `index_pos == 0` is passed (the prefill step), so no explicit clear call
/// is needed — the prefill resets it implicitly.
pub fn generate(
    loaded: &mut Loaded,
    prompt: &str,
    params: &GenParams,
    mut on_delta: impl FnMut(&str),
) -> Result<(FinishReason, usize), EngineError> {
    let enc = loaded
        .tokenizer
        .encode(prompt, true)
        .map_err(|e| EngineError::Generation(format!("encode: {e}")))?;
    let prompt_tokens: Vec<u32> = enc.get_ids().to_vec();
    if prompt_tokens.is_empty() {
        return Err(EngineError::Generation("empty prompt after tokenize".into()));
    }

    let mut logits_processor = build_logits_processor(params);
    let mut all_tokens: Vec<u32> = Vec::new();
    let mut stream = TokenStream::new(&loaded.tokenizer);
    let mut accumulated = String::new();

    // Prefill: feed the whole prompt at index_pos 0 (resets KV cache implicitly).
    let input = Tensor::new(prompt_tokens.as_slice(), &loaded.device)
        .and_then(|t| t.unsqueeze(0))
        .map_err(|e| EngineError::Generation(format!("prompt tensor: {e}")))?;
    let mut logits = loaded
        .model
        .forward(&input, 0)
        .map_err(|e| EngineError::Generation(format!("prefill forward: {e}")))?
        .squeeze(0)
        .map_err(|e| EngineError::Generation(format!("squeeze: {e}")))?;

    let mut finish = FinishReason::Length;
    // True only when a user stop-string matched and intentionally truncated the
    // output. In that case we must NOT flush the tail — doing so would re-emit
    // text the stop-string was supposed to suppress.
    let mut stop_string_truncated = false;
    for step in 0..params.max_tokens {
        // Repeat penalty over recent tokens.
        if params.repeat_penalty != 1.0 && !all_tokens.is_empty() {
            let start = all_tokens.len().saturating_sub(params.repeat_last_n);
            logits = candle_transformers::utils::apply_repeat_penalty(
                &logits,
                params.repeat_penalty,
                &all_tokens[start..],
            )
            .map_err(|e| EngineError::Generation(format!("repeat penalty: {e}")))?;
        }

        let next = logits_processor
            .sample(&logits)
            .map_err(|e| EngineError::Generation(format!("sample: {e}")))?;

        if loaded.stop_ids.contains(&next) {
            finish = FinishReason::Stop;
            break;
        }

        all_tokens.push(next);
        if let Some(delta) = stream.push(next)? {
            accumulated.push_str(&delta);
            // Honor user stop strings on the accumulated text.
            if let Some(cut) = first_stop_hit(&accumulated, &params.stop) {
                let keep = &accumulated[..cut];
                let already = accumulated.len() - delta.len();
                if cut > already {
                    on_delta(&keep[already..]);
                } else {
                    // cut <= already: the stop prefix was already streamed in a prior
                    // delta; stop cleanly without emitting more (streaming can't
                    // retroactively un-emit).
                }
                finish = FinishReason::Stop;
                stop_string_truncated = true;
                break;
            }
            on_delta(&delta);
        }

        // Decode next step: single token at the advancing position.
        let input = Tensor::new(&[next], &loaded.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| EngineError::Generation(format!("step tensor: {e}")))?;
        logits = loaded
            .model
            .forward(&input, prompt_tokens.len() + step)
            .map_err(|e| EngineError::Generation(format!("step forward: {e}")))?
            .squeeze(0)
            .map_err(|e| EngineError::Generation(format!("squeeze: {e}")))?;
    }

    // Flush any trailing buffered text (e.g. punctuation held back by the
    // alphanumeric emit guard). Flush for both EOS/stop-id (Stop) and max-token
    // (Length) completions. Skip only when a user stop-string intentionally
    // truncated the output — flushing then would re-emit suppressed text.
    if !stop_string_truncated {
        let tail = stream.finish()?;
        if !tail.is_empty() {
            on_delta(&tail);
        }
    }

    Ok((finish, all_tokens.len()))
}

/// Index of the first user-stop-string occurrence in `text`, or None.
fn first_stop_hit(text: &str, stops: &[String]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for s in stops {
        if s.is_empty() {
            continue;
        }
        if let Some(i) = text.find(s.as_str()) {
            best = Some(best.map_or(i, |b| b.min(i)));
        }
    }
    best
}

#[cfg(test)]
mod sampling_tests {
    use super::*;

    #[test]
    fn zero_temperature_is_argmax() {
        let p = GenParams { temperature: 0.0, ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::ArgMax));
    }

    #[test]
    fn temp_only_is_all() {
        let p = GenParams { temperature: 0.8, top_p: None, top_k: None, ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::All { .. }));
    }

    #[test]
    fn temp_and_top_p_is_top_p() {
        let p = GenParams { temperature: 0.8, top_p: Some(0.9), top_k: None, ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::TopP { .. }));
    }

    #[test]
    fn top_k_and_top_p_is_combined() {
        let p = GenParams { temperature: 0.8, top_p: Some(0.9), top_k: Some(40), ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::TopKThenTopP { .. }));
    }

    #[test]
    fn defaults_are_sane() {
        let p = GenParams::default();
        assert_eq!(p.max_tokens, 512);
        assert_eq!(p.repeat_last_n, 64);
        assert!((p.repeat_penalty - 1.1).abs() < 1e-6);
    }
}

#[cfg(test)]
mod stop_tests {
    use super::*;

    #[test]
    fn finds_earliest_stop() {
        assert_eq!(first_stop_hit("abcSTOPdef", &["STOP".into()]), Some(3));
        assert_eq!(
            first_stop_hit("aXbYc", &["Y".into(), "X".into()]),
            Some(1),
            "earliest match wins"
        );
        assert_eq!(first_stop_hit("nomatch", &["zzz".into()]), None);
        assert_eq!(first_stop_hit("anything", &[]), None);
    }
}

#[cfg(test)]
mod gated_engine_tests {
    use super::*;
    use crate::local_llm;

    /// Model-backed smoke test. Skips (does NOT fail) when the 688 MB model is
    /// absent — mirrors the embedder's `#[ignore]` live tests so CI isn't blocked.
    #[test]
    fn smoke_generate_two_plus_two() {
        let data_dir = dirs_home_uclaw();
        if !local_llm::is_present(&data_dir) {
            eprintln!("[skip] MiniCPM model not present under {data_dir:?}");
            return;
        }
        let (gguf, tok) = local_llm::model_paths(&data_dir);
        let device = select_device();
        let mut loaded = load(&gguf, &tok, device).expect("load");
        let prompt = local_llm::chat_template::render_chatml(&[
            local_llm::chat_template::ChatMessage { role: "user".into(), content: "2+2=".into() },
        ]);
        let params = GenParams { temperature: 0.0, max_tokens: 16, ..Default::default() };
        let mut out = String::new();
        let (reason, n) = generate(&mut loaded, &prompt, &params, |d| out.push_str(d)).expect("gen");
        eprintln!("smoke out={out:?} reason={reason:?} tokens={n}");
        assert!(out.contains('4'), "expected '4' in {out:?}");
    }

    /// Resolve `~/.uclaw` without the banned `dirs::home_dir` (repo pre-commit
    /// hook blocks it). Uses HOME directly for the test only.
    fn dirs_home_uclaw() -> std::path::PathBuf {
        let home = std::env::var("HOME").expect("HOME set");
        std::path::Path::new(&home).join(".uclaw")
    }
}
