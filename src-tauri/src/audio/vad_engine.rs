//! Unified voice-activity detector.
//!
//! Wraps either the always-available energy detector or the neural Silero VAD
//! (when the `silero-vad` feature is enabled and its ONNX model is present),
//! behind one interface so the dictation and meeting services don't care which
//! backend is active. Prefers Silero when available; falls back gracefully.

use std::path::Path;

use crate::audio::vad::EnergyVad;
#[cfg(feature = "silero-vad")]
use crate::audio::silero_vad::SileroVad;

/// A VAD backend chosen at runtime.
pub enum Vad {
    Energy(EnergyVad),
    #[cfg(feature = "silero-vad")]
    Silero(SileroVad),
}

impl Vad {
    /// Build the best available VAD. With the `silero-vad` feature on and the
    /// Silero model downloaded into `models_dir`, uses the neural detector;
    /// otherwise uses the energy detector. `threshold_rms` applies to the
    /// energy detector; Silero uses a fixed probability threshold.
    pub fn new(threshold_rms: f32, speech_ms: u32, hangover_ms: u32, models_dir: &Path) -> Self {
        #[cfg(feature = "silero-vad")]
        {
            let model_path = SileroVad::default_model_path(models_dir);
            if model_path.exists() {
                match SileroVad::new(&model_path, 0.5, speech_ms, hangover_ms) {
                    Ok(v) => {
                        log::info!("VAD: using Silero neural detector");
                        return Vad::Silero(v);
                    }
                    Err(e) => log::warn!("VAD: Silero load failed ({e}); using energy detector"),
                }
            }
        }
        let _ = models_dir; // unused without silero-vad
        Vad::Energy(EnergyVad::new(threshold_rms, speech_ms, hangover_ms))
    }

    /// See `EnergyVad::process_samples` for the return contract.
    pub fn process_samples(&mut self, samples: &[f32]) -> (Option<bool>, Vec<f32>) {
        match self {
            Vad::Energy(v) => v.process_samples(samples),
            #[cfg(feature = "silero-vad")]
            Vad::Silero(v) => v.process_samples(samples),
        }
    }

    #[allow(dead_code)]
    pub fn is_speech_active(&self) -> bool {
        match self {
            Vad::Energy(v) => v.is_speech_active(),
            #[cfg(feature = "silero-vad")]
            Vad::Silero(v) => v.is_speech_active(),
        }
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        match self {
            Vad::Energy(v) => v.reset(),
            #[cfg(feature = "silero-vad")]
            Vad::Silero(v) => v.reset(),
        }
    }
}
