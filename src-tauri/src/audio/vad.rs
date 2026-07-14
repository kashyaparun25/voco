pub struct EnergyVad {
    #[allow(dead_code)]
    sample_rate: u32,
    frame_size: usize,
    threshold_rms: f32,
    speech_frames_threshold: usize,
    hangover_frames_threshold: usize,

    /// Adaptive estimate of the ambient (non-speech) RMS. Real mic levels vary
    /// hugely with the system input volume and speaking distance — a fixed
    /// absolute threshold sits ABOVE quiet-but-real speech (missed words) or
    /// BELOW a loud room (false triggers). Frames are classified against
    /// `noise_floor * SPEECH_OVER_FLOOR` instead of `threshold_rms` alone.
    noise_floor: f32,

    is_speech_active: bool,
    consecutive_speech_frames: usize,
    consecutive_silent_frames: usize,
    buffer: Vec<f32>,
}

/// A frame counts as speech when its RMS exceeds the noise floor by this ratio.
const SPEECH_OVER_FLOOR: f32 = 2.0;
/// Noise-floor rise per silent frame (~20%/s at 30ms frames): tracks a room
/// that genuinely got louder without letting brief noise inflate it.
const FLOOR_RISE_SILENT: f32 = 1.006;
/// Noise-floor rise per speech frame (~1.7%/s): near-frozen during speech so
/// long utterances can't erode detection, but a permanently loud environment
/// (fan turned on) still re-converges within a minute.
const FLOOR_RISE_SPEECH: f32 = 1.0005;

impl EnergyVad {
    pub fn new(threshold_rms: f32, speech_ms: u32, hangover_ms: u32) -> Self {
        let sample_rate = 16000;
        let frame_size = 480; // 30ms at 16kHz
        let frame_duration_ms = 30;
        let speech_frames_threshold = (speech_ms / frame_duration_ms) as usize;
        let hangover_frames_threshold = (hangover_ms / frame_duration_ms) as usize;
        
        Self {
            sample_rate,
            frame_size,
            threshold_rms,
            speech_frames_threshold: speech_frames_threshold.max(1),
            hangover_frames_threshold: hangover_frames_threshold.max(1),
            // Seed at the configured threshold; the first quiet frames snap it
            // down to the room's real ambient level (the floor takes a min).
            noise_floor: threshold_rms,
            is_speech_active: false,
            consecutive_speech_frames: 0,
            consecutive_silent_frames: 0,
            buffer: Vec::new(),
        }
    }

    /// Process new samples, running the VAD state machine on complete frames.
    /// Returns a tuple `(state_changed, speech_samples_collected)`:
    /// - `state_changed`: `Some(true)` if speech just started, `Some(false)` if speech just ended (silence started), `None` otherwise.
    /// - `speech_samples_collected`: any new speech samples that should be accumulated for transcription.
    pub fn process_samples(&mut self, samples: &[f32]) -> (Option<bool>, Vec<f32>) {
        self.buffer.extend_from_slice(samples);
        let mut state_changed = None;
        let mut speech_samples = Vec::new();

        while self.buffer.len() >= self.frame_size {
            let frame: Vec<f32> = self.buffer.drain(0..self.frame_size).collect();
            
            // Calculate RMS
            let sum_sq: f32 = frame.iter().map(|&s| s * s).sum();
            let rms = (sum_sq / (frame.len() as f32)).sqrt();

            // Adaptive threshold: a multiple of the tracked ambient floor, with
            // an absolute minimum (derived from the configured threshold) so a
            // dead-silent room can't make breathing count as speech.
            let abs_min = (self.threshold_rms * 0.25).max(0.0025);
            let effective_threshold = (self.noise_floor * SPEECH_OVER_FLOOR).max(abs_min);
            let is_frame_speech = rms > effective_threshold;

            // Update the floor AFTER classifying: quieter frames pull it down
            // instantly; louder ones only nudge it up (slower during speech).
            if rms < self.noise_floor {
                self.noise_floor = rms.max(1e-6);
            } else {
                let rise = if is_frame_speech { FLOOR_RISE_SPEECH } else { FLOOR_RISE_SILENT };
                self.noise_floor = (self.noise_floor * rise).min(rms);
            }

            if is_frame_speech {
                self.consecutive_speech_frames += 1;
                self.consecutive_silent_frames = 0;
                
                if !self.is_speech_active && self.consecutive_speech_frames >= self.speech_frames_threshold {
                    self.is_speech_active = true;
                    state_changed = Some(true);
                }
            } else {
                self.consecutive_silent_frames += 1;
                self.consecutive_speech_frames = 0;
                
                if self.is_speech_active && self.consecutive_silent_frames >= self.hangover_frames_threshold {
                    self.is_speech_active = false;
                    state_changed = Some(false);
                }
            }

            if self.is_speech_active {
                speech_samples.extend_from_slice(&frame);
            }
        }

        (state_changed, speech_samples)
    }

    pub fn is_speech_active(&self) -> bool {
        self.is_speech_active
    }

    pub fn reset(&mut self) {
        self.is_speech_active = false;
        self.consecutive_speech_frames = 0;
        self.consecutive_silent_frames = 0;
        self.noise_floor = self.threshold_rms;
        self.buffer.clear();
    }
}
