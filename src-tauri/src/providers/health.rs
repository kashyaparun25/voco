use crate::providers::config::ProviderConfig;
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use serde_json::json;

#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    pub is_reachable: bool,
    pub is_authenticated: bool,
    pub latency_ms: u64,
    pub error_message: Option<String>,
}

/// Checks the reachability, authentication, and latency of a provider
pub async fn check_provider_health(config: &ProviderConfig) -> HealthStatus {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return HealthStatus {
                is_reachable: false,
                is_authenticated: false,
                latency_ms: 0,
                error_message: Some(format!("Failed to build HTTP client: {}", e)),
            };
        }
    };

    // Normalize defensively: users paste full endpoints (e.g.
    // …/v1/audio/transcriptions); appending /models to that yields garbage.
    let normalized = config
        .api_url
        .as_deref()
        .map(crate::providers::config::normalize_base_url);
    let base_url = match &normalized {
        Some(url) => url.trim(),
        None => {
            return HealthStatus {
                is_reachable: false,
                is_authenticated: false,
                latency_ms: 0,
                error_message: Some("No API URL configured".to_string()),
            };
        }
    };

    if base_url.is_empty() {
        return HealthStatus {
            is_reachable: false,
            is_authenticated: false,
            latency_ms: 0,
            error_message: Some("API URL is empty".to_string()),
        };
    }

    let start = Instant::now();

    // Construct the check URL based on the provider type
    let request_builder = if config.provider_type.to_lowercase() == "ollama" {
        // Ollama tags endpoint
        let url = if base_url.ends_with("/api") {
            format!("{}/tags", base_url)
        } else if base_url.ends_with("/api/") {
            format!("{}tags", base_url)
        } else if base_url.ends_with('/') {
            format!("{}api/tags", base_url)
        } else {
            format!("{}/api/tags", base_url)
        };
        client.get(&url)
    } else {
        // OpenAI-compatible /v1/models endpoint
        let url = if base_url.ends_with('/') {
            format!("{}models", base_url)
        } else {
            format!("{}/models", base_url)
        };
        
        let mut builder = client.get(&url);
        if let Some(ref key) = config.api_key {
            if !key.trim().is_empty() {
                builder = builder.bearer_auth(key);
            }
        }
        builder
    };

    match request_builder.send().await {
        Ok(res) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            let status = res.status();

            if status.is_success() {
                HealthStatus {
                    is_reachable: true,
                    is_authenticated: true,
                    latency_ms,
                    error_message: None,
                }
            } else if status.as_u16() == 401 || status.as_u16() == 403 {
                HealthStatus {
                    is_reachable: true,
                    is_authenticated: false,
                    latency_ms,
                    error_message: Some(format!("Authentication failed: HTTP Status {}", status)),
                }
            } else {
                HealthStatus {
                    is_reachable: true,
                    is_authenticated: config.api_key.is_none() || config.api_key.as_ref().unwrap().trim().is_empty(),
                    latency_ms,
                    error_message: Some(format!("Server returned HTTP status {}", status)),
                }
            }
        }
        Err(e) => {
            HealthStatus {
                is_reachable: false,
                is_authenticated: false,
                latency_ms: 0,
                error_message: Some(format!("Network request failed: {}", e)),
            }
        }
    }
}

/// Fetches the list of available model ids from a provider's endpoint.
/// Supports OpenAI-compatible `/models` and Ollama `/api/tags`. On any failure
/// (no URL, network error, unexpected shape) returns an empty vec rather than erroring.
pub async fn fetch_provider_models(config: &ProviderConfig) -> Vec<String> {
    let base_url = match &config.api_url {
        Some(url) if !url.trim().is_empty() => url.trim().to_string(),
        _ => return Vec::new(),
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let is_ollama = config.provider_type.to_lowercase() == "ollama";

    let request_builder = if is_ollama {
        let url = if base_url.ends_with("/api") {
            format!("{}/tags", base_url)
        } else if base_url.ends_with("/api/") {
            format!("{}tags", base_url)
        } else if base_url.ends_with('/') {
            format!("{}api/tags", base_url)
        } else {
            format!("{}/api/tags", base_url)
        };
        client.get(&url)
    } else {
        let url = if base_url.ends_with('/') {
            format!("{}models", base_url)
        } else {
            format!("{}/models", base_url)
        };
        let mut builder = client.get(&url);
        if let Some(ref key) = config.api_key {
            if !key.trim().is_empty() {
                builder = builder.bearer_auth(key);
            }
        }
        builder
    };

    let body: serde_json::Value = match request_builder.send().await {
        Ok(res) if res.status().is_success() => match res.json().await {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        },
        _ => return Vec::new(),
    };

    if is_ollama {
        // Ollama: { "models": [ { "name": "..." }, ... ] }
        body.get("models")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        // OpenAI-compatible: { "data": [ { "id": "..." }, ... ] }
        body.get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Spawns a background task to periodically poll local servers (Ollama, LM Studio)
/// and emit their status to the frontend.
pub fn start_local_server_detection(app_handle: AppHandle) {
    // Use Tauri's managed async runtime — the setup hook runs on the main
    // thread where there is no ambient Tokio reactor, so `tokio::spawn` panics.
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_default();

        loop {
            let mut ollama_online = false;
            let mut lmstudio_online = false;

            // Poll Ollama (either root or /api/tags)
            if let Ok(res) = client.get("http://localhost:11434/api/tags").send().await {
                if res.status().is_success() {
                    ollama_online = true;
                }
            } else if let Ok(res) = client.get("http://localhost:11434/").send().await {
                if res.status().is_success() {
                    ollama_online = true;
                }
            }

            // Poll LM Studio /v1/models
            if let Ok(res) = client.get("http://localhost:1234/v1/models").send().await {
                if res.status().is_success() {
                    lmstudio_online = true;
                }
            }

            // Emit to frontend
            let _ = app_handle.emit("local-servers-status", json!({
                "ollama": ollama_online,
                "lmstudio": lmstudio_online,
            }));

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
}
