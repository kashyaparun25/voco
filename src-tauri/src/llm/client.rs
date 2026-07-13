use async_trait::async_trait;
use futures_util::StreamExt;
use log::{info, warn};
use serde::Deserialize;
use std::time::Duration;
use reqwest_eventsource::{Event, EventSource};
use crate::llm::LlmEngine;

pub struct ApiClient {
    client: reqwest::Client,
    api_url: String,
    api_key: String,
    model: String,
}

impl ApiClient {
    pub fn new(api_url: String, api_key: String, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60)) // Remote APIs can take longer, give it 60s
            .build()
            .unwrap_or_default();
        Self { client, api_url, api_key, model }
    }

    fn completions_url(&self) -> String {
        if self.api_url.ends_with("/chat/completions") {
            self.api_url.clone()
        } else if self.api_url.ends_with("/v1") {
            format!("{}/chat/completions", self.api_url)
        } else if self.api_url.ends_with("/v1/") {
            format!("{}chat/completions", self.api_url)
        } else {
            format!("{}/chat/completions", self.api_url.trim_end_matches('/'))
        }
    }

    /// Streams a chat completion, invoking `on_token` for each token delta as it arrives.
    /// Returns the full concatenated text at the end. Uses SSE (`stream: true`).
    pub async fn generate_stream<F>(&self, prompt: &str, mut on_token: F) -> Result<String, String>
    where
        F: FnMut(&str),
    {
        let url = self.completions_url();
        info!("Sending streaming chat completion request to: {} (model: {})", url, self.model);

        let payload = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": "You are a helpful meeting assistant." },
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.3,
            "stream": true
        });

        let mut request = self.client.post(&url).json(&payload);
        if !self.api_key.trim().is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let mut es = EventSource::new(request)
            .map_err(|e| format!("Failed to open SSE stream: {}", e))?;

        let mut full = String::new();

        while let Some(event) = es.next().await {
            match event {
                Ok(Event::Open) => {}
                Ok(Event::Message(msg)) => {
                    if msg.data.trim() == "[DONE]" {
                        break;
                    }
                    match serde_json::from_str::<StreamChunk>(&msg.data) {
                        Ok(chunk) => {
                            if let Some(choice) = chunk.choices.first() {
                                if let Some(content) = &choice.delta.content {
                                    if !content.is_empty() {
                                        full.push_str(content);
                                        on_token(content);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse SSE chunk: {} (data: {})", e, msg.data);
                        }
                    }
                }
                Err(reqwest_eventsource::Error::StreamEnded) => break,
                Err(e) => {
                    es.close();
                    return Err(format!("SSE stream error: {}", e));
                }
            }
        }

        Ok(full)
    }
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

#[async_trait]
impl LlmEngine for ApiClient {
    async fn generate(&self, prompt: &str) -> Result<String, String> {
        let url = if self.api_url.ends_with("/chat/completions") {
            self.api_url.clone()
        } else if self.api_url.ends_with("/v1") {
            format!("{}/chat/completions", self.api_url)
        } else if self.api_url.ends_with("/v1/") {
            format!("{}chat/completions", self.api_url)
        } else {
            // general path cleanup
            format!("{}/chat/completions", self.api_url.trim_end_matches('/'))
        };

        info!("Sending chat completion request to: {} (model: {})", url, self.model);

        let mut request = self.client.post(&url);
        
        if !self.api_key.trim().is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let payload = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": "You are a helpful meeting assistant." },
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.3
        });

        let response = request.json(&payload)
            .send()
            .await
            .map_err(|e| format!("Failed to send API request: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("Remote API returned error (status {}): {}", status, error_text));
        }

        let body: ChatCompletionResponse = response.json().await
            .map_err(|e| format!("Failed to parse API response JSON: {}", e))?;

        if let Some(choice) = body.choices.first() {
            return Ok(choice.message.content.clone());
        }
        
        Err("API response did not contain any choices/completions.".to_string())
    }
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionMessage,
}

#[derive(Deserialize)]
struct ChatCompletionMessage {
    content: String,
}
