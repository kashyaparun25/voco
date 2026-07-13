//! Real neural speaker diarization backed by the [`speakrs`] crate (pyannote
//! `community-1` pipeline).
//!
//! This module is only compiled when the `neural-diarization` Cargo feature is
//! enabled. It wraps `speakrs`'s [`OwnedDiarizationPipeline`] and exposes a small,
//! `anyhow`-based API that turns raw 16 kHz mono `f32` audio into time-ordered
//! speaker turns.
//!
//! The heavy pyannote segmentation + embedding models are loaded (and, on first
//! run, downloaded from HuggingFace) exactly once when a [`NeuralDiarizer`] is
//! constructed via [`NeuralDiarizer::new`] or [`NeuralDiarizer::from_dir`]. The
//! same pipeline instance is then reused for every [`NeuralDiarizer::diarize`]
//! call.
//!
//! On Apple Silicon the pipeline is built with [`ExecutionMode::CoreMl`]; if that
//! fails to construct (e.g. CoreML unavailable) it transparently falls back to
//! [`ExecutionMode::Cpu`].

use std::path::Path;

use anyhow::Result;
use speakrs::pipeline::{FRAME_DURATION_SECONDS, FRAME_STEP_SECONDS};
use speakrs::{ExecutionMode, OwnedDiarizationPipeline};

/// A single diarized speaker turn, with times in seconds relative to the start
/// of the recording.
#[derive(Debug, Clone, PartialEq)]
pub struct DiarSegment {
    /// Start time in seconds.
    pub start: f64,
    /// End time in seconds.
    pub end: f64,
    /// Speaker label, e.g. `"SPEAKER_00"`.
    pub speaker: String,
}

/// Neural speaker diarizer wrapping a loaded `speakrs` pipeline.
///
/// Construction loads (and possibly downloads) the pyannote models, which is
/// expensive; keep a single instance alive and call [`diarize`](Self::diarize)
/// repeatedly.
pub struct NeuralDiarizer {
    pipeline: OwnedDiarizationPipeline,
    mode: ExecutionMode,
}

impl NeuralDiarizer {
    /// Load/download the pyannote models and build the pipeline.
    ///
    /// Models are resolved via the HuggingFace cache (downloaded on first use).
    /// On macOS this tries [`ExecutionMode::CoreMl`] first and falls back to
    /// [`ExecutionMode::Cpu`] if the CoreML pipeline fails to construct.
    pub fn new() -> Result<Self> {
        Self::build(|mode| OwnedDiarizationPipeline::from_pretrained(mode))
    }

    /// Load the pyannote models from an explicit directory instead of the
    /// HuggingFace cache.
    ///
    /// The directory is expected to contain the standard speakrs model files
    /// (e.g. `segmentation-3.0.onnx`, `wespeaker-voxceleb-resnet34.onnx`, and the
    /// PLDA parameters). Same CoreML-then-CPU fallback as [`new`](Self::new).
    pub fn from_dir(models_dir: &Path) -> Result<Self> {
        Self::build(|mode| OwnedDiarizationPipeline::from_dir(models_dir, mode))
    }

    /// Shared constructor logic: try the preferred execution mode, then fall
    /// back to CPU. `build_fn` maps an [`ExecutionMode`] to a pipeline.
    fn build<F>(build_fn: F) -> Result<Self>
    where
        F: Fn(ExecutionMode) -> Result<OwnedDiarizationPipeline, speakrs::PipelineError>,
    {
        // On Apple Silicon / macOS, prefer CoreML; elsewhere go straight to CPU.
        // `VOCO_DIARIZER_MODE=cpu|coreml|coreml-fast` overrides the default.
        let preferred = match std::env::var("VOCO_DIARIZER_MODE").ok().as_deref() {
            Some("cpu") => ExecutionMode::Cpu,
            Some("coreml") => ExecutionMode::CoreMl,
            Some("coreml-fast") => ExecutionMode::CoreMlFast,
            _ if cfg!(target_os = "macos") => ExecutionMode::CoreMl,
            _ => ExecutionMode::Cpu,
        };

        log::info!("neural-diarization: loading pyannote pipeline (mode={preferred:?})");
        match build_fn(preferred) {
            Ok(pipeline) => {
                log::info!("neural-diarization: pipeline ready (mode={preferred:?})");
                Ok(Self {
                    pipeline,
                    mode: preferred,
                })
            }
            Err(err) if preferred != ExecutionMode::Cpu => {
                log::warn!(
                    "neural-diarization: {preferred:?} pipeline failed ({err}); \
                     falling back to CPU"
                );
                let pipeline = build_fn(ExecutionMode::Cpu)
                    .map_err(|e| anyhow::anyhow!("speakrs: {e}"))?;
                log::info!("neural-diarization: pipeline ready (mode=Cpu)");
                Ok(Self {
                    pipeline,
                    mode: ExecutionMode::Cpu,
                })
            }
            Err(err) => Err(anyhow::anyhow!("speakrs: {err}")),
        }
    }

    /// The execution mode the loaded pipeline is running in.
    pub fn mode(&self) -> ExecutionMode {
        self.mode
    }

    /// Diarize a full 16 kHz mono recording and return time-ordered speaker
    /// turns.
    ///
    /// Takes `&mut self` because the underlying `speakrs` pipeline runs
    /// segmentation/embedding inference with `&mut` model access.
    pub fn diarize(&mut self, audio: &[f32]) -> Result<Vec<DiarSegment>> {
        if audio.is_empty() {
            return Ok(Vec::new());
        }

        log::info!(
            "neural-diarization: running on {} samples ({:.2}s @16kHz)",
            audio.len(),
            audio.len() as f64 / 16_000.0
        );

        let result = self
            .pipeline
            .run(audio)
            .map_err(|e| anyhow::anyhow!("speakrs: {e}"))?;

        // Diagnostics: shape + peak activation of the frame-level matrix.
        let (frames, speakers) = result.discrete_diarization.dim();
        let peak = result
            .discrete_diarization
            .iter()
            .cloned()
            .fold(0.0_f32, f32::max);
        log::info!(
            "neural-diarization: activation matrix {frames} frames x {speakers} speakers, peak={peak:.3}"
        );

        // The canonical path (see speakrs `print_turns` / `speaker_airtime`
        // examples) turns the frame-level `discrete_diarization` activation
        // matrix into labeled `Segment`s via `to_segments`. `DiscreteDiarization`
        // exposes this directly as a convenience method.
        let segments = result
            .discrete_diarization
            .to_segments(FRAME_STEP_SECONDS, FRAME_DURATION_SECONDS);

        let turns: Vec<DiarSegment> = segments
            .into_iter()
            .map(|s| DiarSegment {
                start: s.start,
                end: s.end,
                speaker: s.speaker,
            })
            .collect();

        log::info!(
            "neural-diarization: produced {} speaker turns",
            turns.len()
        );

        Ok(turns)
    }

    /// Given the diarization turns, return the label of the speaker whose turns
    /// have the greatest temporal overlap with `[start, end]`.
    ///
    /// Returns `None` if no turn overlaps the span. This lets a caller label its
    /// own STT segments by time overlap against the diarization result.
    pub fn speaker_for_span(turns: &[DiarSegment], start: f64, end: f64) -> Option<String> {
        let mut best_speaker: Option<&str> = None;
        let mut best_overlap = 0.0_f64;

        for turn in turns {
            let overlap = (turn.end.min(end) - turn.start.max(start)).max(0.0);
            if overlap > best_overlap {
                best_overlap = overlap;
                best_speaker = Some(turn.speaker.as_str());
            }
        }

        best_speaker.map(|s| s.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, speaker: &str) -> DiarSegment {
        DiarSegment {
            start,
            end,
            speaker: speaker.to_owned(),
        }
    }

    #[test]
    fn speaker_for_span_picks_dominant_overlap() {
        let turns = vec![
            seg(0.0, 2.0, "SPEAKER_00"),
            seg(2.0, 5.0, "SPEAKER_01"),
        ];
        // Span [1.5, 4.0]: 0.5s overlaps SPEAKER_00, 2.0s overlaps SPEAKER_01.
        assert_eq!(
            NeuralDiarizer::speaker_for_span(&turns, 1.5, 4.0),
            Some("SPEAKER_01".to_owned())
        );
    }

    #[test]
    fn speaker_for_span_no_overlap_is_none() {
        let turns = vec![seg(0.0, 1.0, "SPEAKER_00")];
        assert_eq!(NeuralDiarizer::speaker_for_span(&turns, 5.0, 6.0), None);
    }

    #[test]
    fn speaker_for_span_empty_is_none() {
        assert_eq!(NeuralDiarizer::speaker_for_span(&[], 0.0, 1.0), None);
    }
}
