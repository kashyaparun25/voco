//! Embedded GGUF LLM inference via `llama-cpp-2` (llama.cpp), Metal-accelerated.
//!
//! Enabled by the `embedded-llm` cargo feature. The whole module is compiled
//! only when that feature is on, so the default build has no llama.cpp
//! dependency. Summaries are infrequent, so the model is loaded on demand per
//! call (keeps the type trivially `Send + Sync` and avoids self-referential
//! model/context lifetimes). The blocking llama.cpp work runs on a blocking
//! thread so the async runtime is never stalled.

// `token_to_str`/`Special` are marked deprecated upstream in favour of
// `token_to_piece`, which requires pulling in `encoding_rs` as a direct
// dependency. The simple API works correctly, so we opt out of the lint here.
#![allow(deprecated)]

use std::num::NonZeroU32;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use log::info;

/// The llama.cpp backend can only be initialised once per process.
fn backend() -> Result<&'static LlamaBackend> {
    use std::sync::OnceLock;
    static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();
    if let Some(b) = BACKEND.get() {
        return Ok(b);
    }
    let b = LlamaBackend::init().context("failed to init llama.cpp backend")?;
    // If another thread won the race, that's fine — use whichever is stored.
    let _ = BACKEND.set(b);
    BACKEND
        .get()
        .ok_or_else(|| anyhow!("llama backend unavailable"))
}

/// An embedded GGUF chat model runner.
pub struct EmbeddedLlm {
    model_path: PathBuf,
    n_ctx: u32,
    max_tokens: i32,
}

impl EmbeddedLlm {
    /// Create a runner for the GGUF file at `model_path` (not loaded until `generate`).
    pub fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            n_ctx: 4096,
            max_tokens: 1024,
        }
    }

    /// Whether the backing GGUF file exists on disk.
    pub fn model_exists(&self) -> bool {
        self.model_path.exists()
    }

    /// Run generation on a blocking thread (llama.cpp is synchronous).
    pub async fn generate(&self, prompt: &str) -> Result<String, String> {
        let path = self.model_path.clone();
        let prompt = prompt.to_string();
        let n_ctx = self.n_ctx;
        let max_tokens = self.max_tokens;

        tokio::task::spawn_blocking(move || Self::run_blocking(&path, &prompt, n_ctx, max_tokens))
            .await
            .map_err(|e| format!("embedded LLM task join error: {e}"))?
            .map_err(|e| format!("embedded LLM error: {e}"))
    }

    /// The actual blocking inference: load → tokenize → decode prompt → greedy-ish sample loop.
    fn run_blocking(path: &PathBuf, prompt: &str, n_ctx: u32, max_tokens: i32) -> Result<String> {
        if !path.exists() {
            return Err(anyhow!("GGUF model not found at {}", path.display()));
        }

        let backend = backend()?;

        // Offload all layers to Metal on Apple Silicon (no-op on a CPU build).
        let model_params = LlamaModelParams::default().with_n_gpu_layers(1000);
        let model = LlamaModel::load_from_file(backend, path, &model_params)
            .with_context(|| format!("failed to load GGUF model at {}", path.display()))?;

        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(n_ctx));
        let mut ctx = model
            .new_context(backend, ctx_params)
            .context("failed to create llama context")?;

        // Wrap the raw prompt in a generic chat template. Most instruct GGUFs
        // (Qwen/Gemma/Llama) tolerate a ChatML-style wrapper.
        let templated = format!(
            "<|im_start|>system\nYou are a concise meeting-notes assistant.<|im_end|>\n\
             <|im_start|>user\n{prompt}<|im_end|>\n<|im_start|>assistant\n"
        );

        let tokens = model
            .str_to_token(&templated, AddBos::Always)
            .context("failed to tokenize prompt")?;

        let n_ctx_usize = n_ctx as usize;
        if tokens.len() >= n_ctx_usize {
            return Err(anyhow!(
                "prompt ({} tokens) exceeds context window ({})",
                tokens.len(),
                n_ctx
            ));
        }

        let mut batch = LlamaBatch::new(n_ctx_usize.max(512), 1);
        let last_idx = tokens.len() as i32 - 1;
        for (i, tok) in tokens.iter().enumerate() {
            batch
                .add(*tok, i as i32, &[0], i as i32 == last_idx)
                .map_err(|e| anyhow!("batch add failed: {e}"))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| anyhow!("prompt decode failed: {e}"))?;

        // Low-temperature sampling for stable, factual summaries.
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::top_k(40),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::temp(0.3),
            LlamaSampler::dist(1234),
        ]);

        let mut out = String::new();
        let mut n_cur = batch.n_tokens();
        let n_limit = (tokens.len() as i32 + max_tokens).min(n_ctx as i32 - 1);

        while n_cur < n_limit {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if model.is_eog_token(token) {
                break;
            }

            match model.token_to_str(token, Special::Plaintext) {
                Ok(piece) => out.push_str(&piece),
                Err(e) => {
                    info!("embedded LLM: skipping undecodable token: {e}");
                }
            }

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| anyhow!("batch add (gen) failed: {e}"))?;
            n_cur += 1;
            ctx.decode(&mut batch)
                .map_err(|e| anyhow!("generation decode failed: {e}"))?;
        }

        Ok(out.trim().to_string())
    }
}
