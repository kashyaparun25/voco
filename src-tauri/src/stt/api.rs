use crate::stt::engine::{
    EngineInfo, PartialResult, ProviderType, SttEngine, TranscriptionResult, TranscriptSegment, WordTimestamp,
};
use serde::Deserialize;
use std::time::Instant;

pub struct ApiSttEngine {
    pub api_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub provider_type: String,
    /// Forced transcription language (ISO code, e.g. "en"). `None` = auto-detect.
    pub language: Option<String>,
}

impl ApiSttEngine {
    pub fn new(api_url: String, api_key: Option<String>, model: String, provider_type: String) -> Self {
        Self {
            api_url,
            api_key,
            model,
            provider_type,
            language: None,
        }
    }

    /// Set the forced transcription language (`None` = auto-detect).
    pub fn with_language(mut self, language: Option<String>) -> Self {
        self.language = language;
        self
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIWord {
    word: String,
    start: f64,
    end: f64,
}

#[derive(Debug, Deserialize)]
struct OpenAISegment {
    id: Option<i64>,
    start: f64,
    end: f64,
    text: String,
    words: Option<Vec<OpenAIWord>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIVerboseJson {
    text: String,
    segments: Option<Vec<OpenAISegment>>,
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAISimpleJson {
    text: String,
}

use async_trait::async_trait;

#[async_trait]
impl SttEngine for ApiSttEngine {
    async fn transcribe(&self, audio: &[f32]) -> anyhow::Result<TranscriptionResult> {
        let start_time = Instant::now();
        let client = reqwest::Client::new();

        // Convert PCM f32 audio to WAV bytes
        let wav_bytes = pcm_to_wav(audio, 16000);

        // Determine API endpoint
        let endpoint = if self.api_url.ends_with("/audio/transcriptions") {
            self.api_url.clone()
        } else if self.api_url.ends_with('/') {
            format!("{}audio/transcriptions", self.api_url)
        } else {
            format!("{}/audio/transcriptions", self.api_url)
        };

        // Build multipart request
        let file_part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone());

        // For OpenAI, Groq, NVIDIA compatible APIs, verbose_json includes word and segment timestamps
        if self.provider_type != "ollama" {
            form = form.text("response_format", "verbose_json");
            // Force the transcription language when configured (empty/None = auto).
            if let Some(lang) = self.language.as_deref().filter(|l| !l.is_empty()) {
                form = form.text("language", lang.to_string());
            }
        }

        let mut req = client.post(&endpoint).multipart(form);

        if let Some(ref key) = self.api_key {
            if !key.trim().is_empty() {
                req = req.bearer_auth(key);
            }
        }

        let res = req.send().await?;
        let status = res.status();
        
        if !status.is_success() {
            let error_text = res.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "API request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let response_text = res.text().await?;
        let processing_time_ms = start_time.elapsed().as_millis() as u64;

        // Try to parse verbose_json
        if let Ok(verbose) = serde_json::from_str::<OpenAIVerboseJson>(&response_text) {
            let text = verbose.text.clone();
            let mut segments = Vec::new();

            if let Some(resp_segs) = verbose.segments {
                for seg in resp_segs {
                    let id = uuid::Uuid::new_v4();
                    let words = seg
                        .words
                        .unwrap_or_default()
                        .into_iter()
                        .map(|w| WordTimestamp {
                            word: w.word,
                            start: w.start,
                            end: w.end,
                            confidence: 1.0,
                        })
                        .collect();

                    segments.push(TranscriptSegment {
                        id,
                        meeting_id: None,
                        text: seg.text,
                        start_time: seg.start,
                        end_time: seg.end,
                        speaker_id: None,
                        confidence: 1.0,
                        words,
                    });
                }
            } else {
                // Fallback if segments not present in verbose response
                segments.push(TranscriptSegment {
                    id: uuid::Uuid::new_v4(),
                    meeting_id: None,
                    text: text.clone(),
                    start_time: 0.0,
                    end_time: audio.len() as f64 / 16000.0,
                    speaker_id: None,
                    confidence: 1.0,
                    words: Vec::new(),
                });
            }

            return Ok(TranscriptionResult {
                text,
                segments,
                language: verbose.language,
                processing_time_ms,
            });
        }

        // Fallback to simple JSON
        if let Ok(simple) = serde_json::from_str::<OpenAISimpleJson>(&response_text) {
            let text = simple.text;
            let segments = vec![TranscriptSegment {
                id: uuid::Uuid::new_v4(),
                meeting_id: None,
                text: text.clone(),
                start_time: 0.0,
                end_time: audio.len() as f64 / 16000.0,
                speaker_id: None,
                confidence: 1.0,
                words: Vec::new(),
            }];

            return Ok(TranscriptionResult {
                text,
                segments,
                language: None,
                processing_time_ms,
            });
        }

        // Return raw text if JSON parsing failed
        Ok(TranscriptionResult {
            text: response_text.clone(),
            segments: vec![TranscriptSegment {
                id: uuid::Uuid::new_v4(),
                meeting_id: None,
                text: response_text,
                start_time: 0.0,
                end_time: audio.len() as f64 / 16000.0,
                speaker_id: None,
                confidence: 1.0,
                words: Vec::new(),
            }],
            language: None,
            processing_time_ms,
        })
    }

    async fn transcribe_streaming(
        &self,
        audio: &[f32],
        tx: tokio::sync::mpsc::Sender<PartialResult>,
    ) -> anyhow::Result<TranscriptionResult> {
        let result = self.transcribe(audio).await?;
        let _ = tx
            .send(PartialResult {
                text: result.text.clone(),
                is_final: true,
            })
            .await;
        Ok(result)
    }

    fn info(&self) -> EngineInfo {
        EngineInfo {
            name: format!("API: {} ({})", self.model, self.provider_type),
            provider_type: ProviderType::CloudAPI,
            supports_streaming: false,
            supports_timestamps: self.provider_type != "ollama",
        }
    }
}

/// Helper function to convert raw f32 samples to standard 16-bit WAV file bytes
fn pcm_to_wav(pcm: &[f32], sample_rate: u32) -> Vec<u8> {
    let mut wav = Vec::new();
    let num_samples = pcm.len();
    let num_channels = 1;
    let bits_per_sample = 16;
    let byte_rate = sample_rate * num_channels * (bits_per_sample / 8);
    let block_align = num_channels * (bits_per_sample / 8);
    let data_size = num_samples * (bits_per_sample as usize / 8);
    let file_size = 36 + data_size;

    // RIFF chunk descriptor
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(file_size as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // "fmt " sub-chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&(16u32).to_le_bytes()); // subchunk1_size (16 for PCM)
    wav.extend_from_slice(&(1u16).to_le_bytes());  // audio_format (1 for PCM)
    wav.extend_from_slice(&(num_channels as u16).to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&(block_align as u16).to_le_bytes());
    wav.extend_from_slice(&(bits_per_sample as u16).to_le_bytes());

    // "data" sub-chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_size as u32).to_le_bytes());

    // Convert samples to i16 and write
    for &sample in pcm {
        let clamped = sample.clamp(-1.0, 1.0);
        let sample_i16 = (clamped * i16::MAX as f32) as i16;
        wav.extend_from_slice(&sample_i16.to_le_bytes());
    }

    wav
}
