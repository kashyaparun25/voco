//! Neural voice-activity detection backed by the Silero VAD v5 ONNX model.
//!
//! This module is only compiled when the `silero-vad` cargo feature is enabled
//! (it is declared under `#[cfg(feature = "silero-vad")]` in `mod.rs`), so the
//! whole file transitively depends on `ort` + `ndarray` without needing inner
//! `cfg` attributes.
//!
//! [`SileroVad`] is a drop-in alternative to [`crate::audio::vad::EnergyVad`]:
//! it exposes the same `new` / `process_samples` / `is_speech_active` / `reset`
//! surface plus a couple of model-management helpers. Instead of a naive RMS
//! energy threshold it runs the Silero recurrent network per 512-sample window
//! (32 ms at 16 kHz) and applies the same min-speech / min-silence hangover
//! state machine on top of the model's speech probability.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ort::{session::Session, value::Tensor};

/// Sample rate the Silero v5 model expects. The exported ONNX graph only
/// supports 16 kHz (and 8 kHz); we always run at 16 kHz.
const SAMPLE_RATE: i64 = 16_000;

/// Window size in samples for 16 kHz. Silero v5 requires exactly 512 samples
/// per inference call at 16 kHz (32 ms).
const WINDOW_SIZE: usize = 512;

/// Duration of one window in milliseconds (`WINDOW_SIZE / 16000 * 1000`).
const WINDOW_MS: u32 = 32;

/// Shape of the recurrent state tensor: `[2, 1, 128]`.
const STATE_SHAPE: [i64; 3] = [2, 1, 128];
const STATE_LEN: usize = 2 * 1 * 128;

/// URL of the canonical Silero v5 ONNX model.
const MODEL_URL: &str =
    "https://raw.githubusercontent.com/snakers4/silero-vad/master/src/silero_vad/data/silero_vad.onnx";

/// Neural VAD wrapping an `ort` [`Session`] running the Silero v5 model.
///
/// The `ort` [`Session`] is `Send`, and all other fields are plain owned data,
/// so `SileroVad` is `Send` and can be moved into the audio worker thread just
/// like [`EnergyVad`](crate::audio::vad::EnergyVad).
pub struct SileroVad {
    session: Session,

    /// Speech-probability threshold in `[0, 1]`; windows above this count as
    /// speech (Silero's recommended default is ~0.5).
    threshold: f32,

    /// Number of consecutive speech windows required to *enter* the speech
    /// state (derived from `min_speech_ms`).
    speech_windows_threshold: usize,
    /// Number of consecutive silence windows required to *leave* the speech
    /// state (derived from `min_silence_ms`).
    silence_windows_threshold: usize,

    is_speech_active: bool,
    consecutive_speech_windows: usize,
    consecutive_silence_windows: usize,

    /// Partial-window remainder, mirroring `EnergyVad`'s frame buffer.
    buffer: Vec<f32>,

    /// Recurrent state `[2, 1, 128]` carried between windows and fed back into
    /// the model on every call. Reset to zeros in [`SileroVad::reset`].
    state: Vec<f32>,
}

impl SileroVad {
    /// Load the Silero model from `model_path` and build the VAD state machine.
    ///
    /// `threshold` is the speech probability cutoff (typically ~0.5).
    /// `min_speech_ms` / `min_silence_ms` are converted into a count of
    /// 32 ms windows for the hangover / min-duration logic (each rounded up to
    /// at least one window).
    pub fn new(
        model_path: &Path,
        threshold: f32,
        min_speech_ms: u32,
        min_silence_ms: u32,
    ) -> Result<Self> {
        let session = Session::builder()
            .context("failed to create ONNX Runtime session builder")?
            .commit_from_file(model_path)
            .with_context(|| {
                format!("failed to load Silero VAD model from {}", model_path.display())
            })?;

        // Round up ms -> windows so a sub-window request still needs >= 1 window.
        let speech_windows_threshold = ms_to_windows(min_speech_ms);
        let silence_windows_threshold = ms_to_windows(min_silence_ms);

        Ok(Self {
            session,
            threshold,
            speech_windows_threshold,
            silence_windows_threshold,
            is_speech_active: false,
            consecutive_speech_windows: 0,
            consecutive_silence_windows: 0,
            buffer: Vec::new(),
            state: vec![0.0f32; STATE_LEN],
        })
    }

    /// Process new samples, running the model on every complete 512-sample
    /// window and driving the speech/silence state machine.
    ///
    /// Returns `(state_changed, speech_samples_collected)`, matching
    /// [`EnergyVad::process_samples`](crate::audio::vad::EnergyVad::process_samples):
    /// - `state_changed`: `Some(true)` when speech just started, `Some(false)`
    ///   when it just ended, `None` otherwise.
    /// - `speech_samples_collected`: new samples captured while speech is active.
    ///
    /// If a model inference fails, the offending window is treated as silence
    /// and the error is logged rather than panicking, so a transient failure
    /// never crashes the audio thread.
    pub fn process_samples(&mut self, samples: &[f32]) -> (Option<bool>, Vec<f32>) {
        self.buffer.extend_from_slice(samples);

        let mut state_changed = None;
        let mut speech_samples = Vec::new();

        // Reusable scratch for the current window to keep allocations out of
        // the hot loop; the tensor create still copies, but we avoid a fresh
        // Vec per window.
        let mut window = vec![0.0f32; WINDOW_SIZE];

        while self.buffer.len() >= WINDOW_SIZE {
            window.copy_from_slice(&self.buffer[..WINDOW_SIZE]);
            self.buffer.drain(0..WINDOW_SIZE);

            let is_window_speech = match self.infer_window(&window) {
                Ok(prob) => prob >= self.threshold,
                Err(e) => {
                    log::warn!("Silero VAD inference failed, treating window as silence: {e:#}");
                    false
                }
            };

            if is_window_speech {
                self.consecutive_speech_windows += 1;
                self.consecutive_silence_windows = 0;

                if !self.is_speech_active
                    && self.consecutive_speech_windows >= self.speech_windows_threshold
                {
                    self.is_speech_active = true;
                    state_changed = Some(true);
                }
            } else {
                self.consecutive_silence_windows += 1;
                self.consecutive_speech_windows = 0;

                if self.is_speech_active
                    && self.consecutive_silence_windows >= self.silence_windows_threshold
                {
                    self.is_speech_active = false;
                    state_changed = Some(false);
                }
            }

            if self.is_speech_active {
                speech_samples.extend_from_slice(&window);
            }
        }

        (state_changed, speech_samples)
    }

    /// Run the model on a single 512-sample window, updating the recurrent
    /// state, and return the speech probability in `[0, 1]`.
    fn infer_window(&mut self, window: &[f32]) -> Result<f32> {
        debug_assert_eq!(window.len(), WINDOW_SIZE);

        let input = Tensor::from_array(([1i64, WINDOW_SIZE as i64], window.to_vec()))
            .context("failed to build Silero `input` tensor")?;
        let state = Tensor::from_array((STATE_SHAPE, self.state.clone()))
            .context("failed to build Silero `state` tensor")?;
        let sr = Tensor::from_array(([1i64], vec![SAMPLE_RATE]))
            .context("failed to build Silero `sr` tensor")?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input" => input,
                "state" => state,
                "sr" => sr,
            ])
            .context("Silero VAD session run failed")?;

        // Update the recurrent state from `stateN` before reading probability.
        let (_, new_state) = outputs["stateN"]
            .try_extract_tensor::<f32>()
            .context("failed to extract Silero `stateN` output")?;
        if new_state.len() == STATE_LEN {
            self.state.copy_from_slice(new_state);
        } else {
            // Shape drift would corrupt subsequent calls; surface it instead.
            anyhow::bail!(
                "unexpected Silero `stateN` length: got {}, expected {}",
                new_state.len(),
                STATE_LEN
            );
        }

        let (_, output) = outputs["output"]
            .try_extract_tensor::<f32>()
            .context("failed to extract Silero `output` tensor")?;
        let prob = output
            .first()
            .copied()
            .context("Silero `output` tensor was empty")?;

        Ok(prob)
    }

    /// Whether the VAD currently considers speech active.
    pub fn is_speech_active(&self) -> bool {
        self.is_speech_active
    }

    /// Reset the state machine, sample buffer, and recurrent model state.
    /// The loaded session is kept.
    pub fn reset(&mut self) {
        self.is_speech_active = false;
        self.consecutive_speech_windows = 0;
        self.consecutive_silence_windows = 0;
        self.buffer.clear();
        // Zero the recurrent state in place (no realloc).
        for v in self.state.iter_mut() {
            *v = 0.0;
        }
    }

    /// Conventional on-disk path for the model inside `models_dir`.
    pub fn default_model_path(models_dir: &Path) -> PathBuf {
        models_dir.join("silero_vad.onnx")
    }

    /// Ensure the Silero v5 ONNX model exists under `models_dir`, downloading it
    /// if missing, and return its path.
    ///
    /// Uses async `reqwest` driven by `tauri::async_runtime::block_on` because
    /// the project's `reqwest` build does not enable the blocking feature. This
    /// only blocks the calling thread, never unrelated threads.
    pub fn download_model(models_dir: &Path) -> Result<PathBuf> {
        let dest = Self::default_model_path(models_dir);
        if dest.exists() {
            return Ok(dest);
        }

        std::fs::create_dir_all(models_dir).with_context(|| {
            format!("failed to create models directory {}", models_dir.display())
        })?;

        let bytes = tauri::async_runtime::block_on(async {
            let resp = reqwest::get(MODEL_URL)
                .await
                .context("failed to request Silero VAD model")?;
            let resp = resp
                .error_for_status()
                .context("Silero VAD model download returned an error status")?;
            resp.bytes()
                .await
                .context("failed to read Silero VAD model response body")
        })?;

        // Write to a temp file then rename so a partial download can't be
        // mistaken for a valid model.
        let tmp = dest.with_extension("onnx.part");
        std::fs::write(&tmp, &bytes)
            .with_context(|| format!("failed to write model to {}", tmp.display()))?;
        std::fs::rename(&tmp, &dest).with_context(|| {
            format!("failed to move downloaded model into place at {}", dest.display())
        })?;

        Ok(dest)
    }
}

/// Convert a duration in milliseconds into a count of 32 ms windows, rounding
/// up and clamping to at least one window.
fn ms_to_windows(ms: u32) -> usize {
    (ms.div_ceil(WINDOW_MS)).max(1) as usize
}
