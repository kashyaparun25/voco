pub mod engine;
pub mod whisper;
pub mod manager;
pub mod api;

#[cfg(feature = "parakeet")]
pub mod parakeet;

#[cfg(feature = "audio8")]
pub mod audio8;

pub use engine::{SttEngine, TranscriptionResult, TranscriptSegment, WordTimestamp, PartialResult, EngineInfo, ProviderType};
#[cfg(feature = "parakeet")]
pub use parakeet::ParakeetEngine;
#[cfg(feature = "audio8")]
pub use audio8::Audio8Engine;
pub use whisper::WhisperEngine;
pub use manager::{ModelManager, ModelInfo, CustomModel};
pub use api::ApiSttEngine;

/// The configured forced transcription language. Default `Some("en")` (English —
/// predictable, avoids stray-script auto-detection); `"auto"`/empty → `None`
/// (let the engine auto-detect); any other value (e.g. `"es"`) forces it.
/// Note: this only applies to Whisper and OpenAI/Groq-style APIs — the embedded
/// Audio8 model auto-detects and cannot be forced.
pub fn stt_language(db: &crate::storage::Database) -> Option<String> {
    match db.get_setting("stt_language").unwrap_or(None).as_deref() {
        Some("auto") | Some("") => None,
        Some(l) => Some(l.to_string()),
        None => Some("en".to_string()),
    }
}
