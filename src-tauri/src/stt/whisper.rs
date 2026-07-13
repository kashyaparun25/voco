use std::path::PathBuf;
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
use crate::stt::engine::{SttEngine, TranscriptionResult, TranscriptSegment, PartialResult, EngineInfo, ProviderType};

pub struct WhisperEngine {
    context: WhisperContext,
    model_name: String,
    /// Forced transcription language (e.g. "en"). `None` = auto-detect.
    language: Option<String>,
}

impl WhisperEngine {
    pub fn new(model_path: PathBuf) -> anyhow::Result<Self> {
        let model_path_str = model_path.to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid model path"))?;

        // Load the context
        let context = WhisperContext::new_with_params(
            model_path_str,
            WhisperContextParameters::default(),
        )?;

        let model_name = model_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Whisper Local")
            .to_string();

        // Default to English (predictable — no stray-script auto-detection).
        Ok(Self { context, model_name, language: Some("en".to_string()) })
    }

    /// Set the forced transcription language (`None` = auto-detect).
    pub fn with_language(mut self, language: Option<String>) -> Self {
        self.language = language;
        self
    }
}

use async_trait::async_trait;

#[async_trait]
impl SttEngine for WhisperEngine {
    async fn transcribe(&self, audio: &[f32]) -> anyhow::Result<TranscriptionResult> {
        let start_time = std::time::Instant::now();
        let mut state = self.context.create_state()?;
        
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(4);
        params.set_translate(false);
        params.set_language(self.language.as_deref());
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Run inference
        state.full(params, audio)?;

        let num_segments = state.full_n_segments()?;
        let mut segments = Vec::new();
        let mut full_text = String::new();

        for i in 0..num_segments {
            let text = state.full_get_segment_text(i)?;
            let start = state.full_get_segment_t0(i)?; // in 10ms units
            let end = state.full_get_segment_t1(i)?; // in 10ms units
            
            let start_sec = (start as f64) / 100.0;
            let end_sec = (end as f64) / 100.0;

            let segment = TranscriptSegment {
                id: uuid::Uuid::new_v4(),
                meeting_id: None,
                text: text.trim().to_string(),
                start_time: start_sec,
                end_time: end_sec,
                speaker_id: None,
                confidence: 1.0,
                words: Vec::new(),
            };
            
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                if !full_text.is_empty() {
                    full_text.push(' ');
                }
                full_text.push_str(trimmed);
            }
            segments.push(segment);
        }

        let processing_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(TranscriptionResult {
            text: full_text,
            segments,
            language: self.language.clone(),
            processing_time_ms,
        })
    }

    async fn transcribe_streaming(
        &self,
        audio: &[f32],
        tx: tokio::sync::mpsc::Sender<PartialResult>,
    ) -> anyhow::Result<TranscriptionResult> {
        let res = self.transcribe(audio).await?;
        let _ = tx.send(PartialResult {
            text: res.text.clone(),
            is_final: true,
        }).await;
        Ok(res)
    }

    fn info(&self) -> EngineInfo {
        EngineInfo {
            name: self.model_name.clone(),
            provider_type: ProviderType::Embedded,
            supports_streaming: true,
            supports_timestamps: true,
        }
    }
}
