pub mod prompt;
pub mod local_runner;
pub mod client;
#[cfg(feature = "embedded-llm")]
pub mod embedded;

use async_trait::async_trait;
use crate::storage::Database;
use crate::providers::ProviderRegistry;
use log::{info, warn};

#[async_trait]
pub trait LlmEngine {
    async fn generate(&self, prompt: &str) -> Result<String, String>;
}

pub fn get_llm_engine(db: &Database) -> Result<Box<dyn LlmEngine + Send + Sync>, String> {
    // 1. Get default_llm_provider
    let provider_id = db
        .get_setting("default_llm_provider")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "embedded".to_string());

    info!("Initializing LLM engine for provider: {}", provider_id);

    if provider_id == "embedded" {
        #[allow(unused_mut)]
        let mut runner = local_runner::LocalRunner::new();
        // If an embedded GGUF model has been downloaded, prefer real in-app
        // inference over the local-API/simulated fallbacks.
        #[cfg(feature = "embedded-llm")]
        {
            let mut gguf: Option<std::path::PathBuf> = db
                .get_setting("embedded_llm_path")
                .ok()
                .flatten()
                .map(std::path::PathBuf::from)
                .filter(|p| p.exists());

            // Fall back to auto-discovering any downloaded GGUF in the models dir.
            if gguf.is_none() {
                if let Ok(Some(dir)) = db.get_setting("models_dir") {
                    if let Ok(entries) = std::fs::read_dir(&dir) {
                        gguf = entries
                            .filter_map(|e| e.ok().map(|e| e.path()))
                            .find(|p| p.extension().map(|x| x == "gguf").unwrap_or(false));
                    }
                }
            }

            if let Some(path) = gguf {
                info!("Embedded LLM using GGUF: {:?}", path);
                runner = runner.with_embedded_model(path);
            }
        }
        return Ok(Box::new(runner));
    }

    // 2. Fetch config from registry
    let registry = ProviderRegistry::new(db.clone());
    let config_opt = registry.get_provider(&provider_id)?;

    match config_opt {
        Some(config) => {
            let api_url = config.api_url.unwrap_or_default();
            let api_key = config.api_key.unwrap_or_default();
            // Prefer the per-task summary model so the same connection can be
            // used for STT (Whisper) and summaries (an LLM) at once.
            let model = db
                .get_setting("summary_llm_model")
                .ok()
                .flatten()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| if config.default_model.is_empty() { "default".to_string() } else { config.default_model });

            info!("Creating Remote LLM client for provider type: {}, model: {}", config.provider_type, model);
            
            Ok(Box::new(client::ApiClient::new(
                api_url,
                api_key,
                model,
            )))
        }
        None => {
            warn!("LLM Provider '{}' not found in registry. Falling back to Local/Embedded Runner.", provider_id);
            Ok(Box::new(local_runner::LocalRunner::new()))
        }
    }
}
