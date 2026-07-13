use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub enum ProviderType {
    Embedded,
    LocalServer,
    CloudAPI,
}

#[derive(Debug, Clone, Serialize)]
pub struct WordTimestamp {
    pub word: String,
    pub start: f64,
    pub end: f64,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptSegment {
    pub id: uuid::Uuid,
    pub meeting_id: Option<uuid::Uuid>,
    pub text: String,
    pub start_time: f64,
    pub end_time: f64,
    pub speaker_id: Option<String>,
    pub confidence: f32,
    pub words: Vec<WordTimestamp>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub segments: Vec<TranscriptSegment>,
    pub language: Option<String>,
    pub processing_time_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PartialResult {
    pub text: String,
    pub is_final: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineInfo {
    pub name: String,
    pub provider_type: ProviderType,
    pub supports_streaming: bool,
    pub supports_timestamps: bool,
}

use async_trait::async_trait;

#[async_trait]
pub trait SttEngine: Send + Sync {
    /// Transcribe an audio buffer (16kHz mono f32)
    async fn transcribe(&self, audio: &[f32]) -> anyhow::Result<TranscriptionResult>;
    
    /// Transcribe with streaming partial results (some engines might not support streaming,
    /// in which case they just return the final result).
    async fn transcribe_streaming(
        &self,
        audio: &[f32],
        tx: tokio::sync::mpsc::Sender<PartialResult>,
    ) -> anyhow::Result<TranscriptionResult>;
    
    /// Get engine info
    fn info(&self) -> EngineInfo;
}
