pub struct EnergyVad {
    #[allow(dead_code)]
    sample_rate: u32,
    frame_size: usize,
    threshold_rms: f32,
    speech_frames_threshold: usize,
    hangover_frames_threshold: usize,
    
    is_speech_active: bool,
    consecutive_speech_frames: usize,
    consecutive_silent_frames: usize,
    buffer: Vec<f32>,
}

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
            
            let is_frame_speech = rms > self.threshold_rms;

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
        self.buffer.clear();
    }
}
