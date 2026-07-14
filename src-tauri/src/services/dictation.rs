use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::collections::VecDeque;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use log::{info, error};
use crossbeam_channel::unbounded;

use crate::storage::Database;
use crate::audio::{start_capture, MicCapture, WarmMic, Vad};
use crate::stt::{WhisperEngine, ModelManager, SttEngine, ApiSttEngine};
use crate::services::text_injector::TextInjector;

#[derive(Debug, Clone, serde::Serialize)]
pub enum DictationStatus {
    Idle,
    Recording,
    Processing,
}

#[derive(Clone)]
pub struct DictationService {
    db: Database,
    model_manager: ModelManager,
    active_capture: Arc<Mutex<Option<MicCapture>>>,
    /// Pre-built, paused mic stream kept "warm" so a hotkey press starts
    /// recording near-instantly (no ~50ms build). No mic indicator while paused.
    warm_mic: Arc<Mutex<Option<WarmMic>>>,
    engine_cache: Arc<Mutex<Option<(String, Arc<dyn SttEngine>)>>>,
    status: Arc<Mutex<DictationStatus>>,
}

impl DictationService {
    pub fn new(db: Database, model_manager: ModelManager) -> Self {
        Self {
            db,
            model_manager,
            active_capture: Arc::new(Mutex::new(None)),
            warm_mic: Arc::new(Mutex::new(None)),
            engine_cache: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(DictationStatus::Idle)),
        }
    }

    /// Build the warm mic for the currently-configured device (or default),
    /// paused. Called at startup (off the hotkey path) and to rebuild on device
    /// change. Never fails the caller — dictation falls back to on-demand capture.
    pub fn prewarm_mic(&self) {
        let device_name = self.db.get_setting("active_audio_device").unwrap_or(None);
        match WarmMic::build(device_name.as_deref()) {
            Ok(w) => {
                *self.warm_mic.lock() = Some(w);
                info!("Dictation warm mic prepared.");
            }
            Err(e) => {
                *self.warm_mic.lock() = None;
                info!("Warm mic prepare skipped ({e}); will use on-demand capture.");
            }
        }
    }

    pub fn get_status(&self) -> DictationStatus {
        self.status.lock().clone()
    }

    /// Loads (or returns the cached) STT engine per current settings. Does NOT
    /// touch the status lock, so it can run at startup (preload) or mid-start
    /// without blocking `get_status`/`stop` callers.
    fn load_engine(&self) -> Result<Arc<dyn SttEngine>, String> {
        let model_id = self.db.get_setting("dictation_stt_model")
            .unwrap_or(None)
            .unwrap_or_else(|| "whisper-tiny-q5".to_string());

        let mut cache = self.engine_cache.lock();

        let provider_id = self.db.get_setting("default_stt_provider")
            .unwrap_or(None)
            .unwrap_or_else(|| "embedded".to_string());

        // Fetch the provider up front (API path) so the cache key can include
        // its model — otherwise changing the provider's model in Settings would
        // keep serving a stale cached engine with the old model.
        let api_provider = if provider_id == "embedded" {
            None
        } else {
            let registry = crate::providers::ProviderRegistry::new(self.db.clone());
            Some(
                registry
                    .get_provider(&provider_id)?
                    .ok_or_else(|| format!("Provider not found: {}", provider_id))?,
            )
        };

        let cache_key = match &api_provider {
            None => format!("embedded:{}", model_id),
            Some(p) => format!("api:{}:{}:{}", provider_id, model_id,
                p.api_url.as_deref().unwrap_or("")),
        };

        let load_new = match &*cache {
            Some((cached_key, _)) => cached_key != &cache_key,
            None => true,
        };

        if load_new {
            let loaded: Arc<dyn SttEngine> = if provider_id == "embedded" {
                // Parakeet TDT (ONNX) when selected and the feature is built in.
                let parakeet: Option<Arc<dyn SttEngine>> = {
                    #[cfg(feature = "parakeet")]
                    {
                        if model_id.contains("parakeet") {
                            let dir = crate::stt::ParakeetEngine::model_dir_default(self.model_manager.models_dir());
                            info!("Loading local Parakeet model from: {:?}", dir);
                            let eng = crate::stt::ParakeetEngine::new(&dir)
                                .map_err(|e| format!("Failed to load Parakeet engine: {:?}", e))?;
                            Some(Arc::new(eng) as Arc<dyn SttEngine>)
                        } else {
                            None
                        }
                    }
                    #[cfg(not(feature = "parakeet"))]
                    {
                        None::<Arc<dyn SttEngine>>
                    }
                };

                if let Some(eng) = parakeet {
                    eng
                } else if model_id.contains("audio8") {
                    #[cfg(feature = "audio8")]
                    {
                        let dir = crate::stt::Audio8Engine::model_dir_default(self.model_manager.models_dir());
                        info!("Loading local Audio8-ASR model from: {:?}", dir);
                        let eng = crate::stt::Audio8Engine::new(&dir)
                            .map_err(|e| format!("Failed to load Audio8 engine: {:?}", e))?;
                        Arc::new(eng) as Arc<dyn SttEngine>
                    }
                    #[cfg(not(feature = "audio8"))]
                    { return Err("Audio8 support is not built into this build".to_string()); }
                } else {
                    info!("Loading local Whisper model: {}", model_id);
                    let model_path = self.model_manager.get_model_path(&model_id)
                        .ok_or_else(|| format!("Model not downloaded: {}. Please download it first.", model_id))?;

                    let loaded = WhisperEngine::new(model_path)
                        .map_err(|e| format!("Failed to load Whisper engine: {:?}", e))?
                        .with_language(crate::stt::stt_language(&self.db));
                    Arc::new(loaded)
                }
            } else {
                info!("Loading API STT provider: {}", provider_id);
                let provider = api_provider.expect("api_provider set for non-embedded");

                let api_url = provider.api_url.unwrap_or_default();
                let api_key = provider.api_key;
                // Use the per-task STT model, never the connection's
                // default_model (which may be an LLM when the same provider is
                // also used for summaries).
                let model = if model_id.is_empty() { "whisper-1".to_string() } else { model_id.clone() };

                let loaded = ApiSttEngine::new(
                    api_url,
                    api_key,
                    model,
                    provider.provider_type,
                ).with_language(crate::stt::stt_language(&self.db));
                Arc::new(loaded)
            };
            *cache = Some((cache_key.clone(), loaded.clone()));
            Ok(loaded)
        } else {
            Ok(cache.as_ref().unwrap().1.clone())
        }
    }

    /// Warm the STT engine cache so the first hotkey press starts instantly.
    /// Call from a background thread at app startup.
    pub fn preload_engine(&self) {
        match self.load_engine() {
            Ok(_) => info!("Dictation engine preloaded and ready."),
            Err(e) => info!("Dictation engine preload skipped: {}", e),
        }
        // Prepare the warm mic too so the first hotkey press is instant.
        self.prewarm_mic();
    }

    pub fn start(&self, app_handle: AppHandle) -> Result<(), String> {
        // Claim the Idle→Recording transition IMMEDIATELY and release the lock,
        // so the pill/UI react instantly and `get_status`/`stop` never block
        // behind a slow engine load.
        {
            let mut status_guard = self.status.lock();
            if !matches!(*status_guard, DictationStatus::Idle) {
                return Err("Dictation is already active".to_string());
            }
            *status_guard = DictationStatus::Recording;
        }
        let _ = app_handle.emit("dictation-status", DictationStatus::Recording);

        // 1. Get settings
        let auto_paste = self.db.get_setting("auto_paste")
            .unwrap_or(None)
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(true);

        let dictation_mode = self.db.get_setting("dictation_mode")
            .unwrap_or(None)
            .unwrap_or_else(|| "Toggle".to_string());

        let threshold_rms = self.db.get_setting("vad_threshold")
            .unwrap_or(None)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.015);

        let speech_ms = self.db.get_setting("vad_speech_ms")
            .unwrap_or(None)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(150);

        let hangover_ms = self.db.get_setting("vad_hangover_ms")
            .unwrap_or(None)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(800);

        let device_name = self.db.get_setting("active_audio_device")
            .unwrap_or(None);

        let model_id = self.db.get_setting("dictation_stt_model")
            .unwrap_or(None)
            .unwrap_or_else(|| "whisper-tiny-q5".to_string());

        info!("Starting dictation with model: {}, mode: {}, auto_paste: {}", model_id, dictation_mode, auto_paste);

        // 2. Load STT Engine (cached after preload — normally instant).
        let engine = match self.load_engine() {
            Ok(e) => e,
            Err(e) => {
                // Roll back the claimed state on failure.
                *self.status.lock() = DictationStatus::Idle;
                let _ = app_handle.emit("dictation-status", DictationStatus::Idle);
                let _ = app_handle.emit("dictation-error", e.clone());
                return Err(e);
            }
        };

        // If the user toggled stop while the engine was loading, abort cleanly.
        if !matches!(*self.status.lock(), DictationStatus::Recording) {
            info!("Dictation was stopped during engine load; aborting start.");
            return Ok(());
        }

        // Pause any currently-playing media (opt-in). Runs off-thread — it shells
        // out to the MediaRemote perl bridge, which we must not let delay capture.
        // Resumed when we return to Idle (or if capture fails below).
        {
            let db = self.db.clone();
            std::thread::spawn(move || crate::services::media_control::pause_if_enabled(&db));
        }

        // Optional start cue.
        crate::services::sound::cue(&self.db, crate::services::sound::Cue::Start);

        // Show the dictation pill on the monitor the user is on. Done here (in
        // the service) so it appears for EVERY trigger — global hotkey, tray,
        // or the on-screen button — not just the button path.
        let _ = crate::commands::window::show_pill(&app_handle);

        // Capture the frontmost app now (off-thread so we never delay capture)
        // — used for per-app enhancement prompts AND the "Top Apps" stat.
        {
            let db_app = self.db.clone();
            std::thread::spawn(move || {
                crate::services::text_processing::capture_target_app(&db_app);
            });
        }

        // 3. Set up communication channels
        let (audio_sender, audio_receiver) = unbounded::<Vec<f32>>();
        let (level_sender, level_receiver) = unbounded::<f32>();

        // Liveness flag: flipped true once ANY audio frame reaches the processing
        // loop. A long-lived warm-mic CoreAudio input unit can silently stop
        // delivering after repeated pause/resume (no error, just no callbacks) —
        // the watchdog below uses this to detect that and self-heal.
        let got_audio = Arc::new(AtomicBool::new(false));
        // Sender clones held for a possible fresh-capture recovery (see watchdog).
        let wd_audio_sender = audio_sender.clone();
        let wd_level_sender = level_sender.clone();

        // 4. Start capture FIRST — before the (slower) VAD load. Prefer the
        // pre-built "warm" mic (near-instant play(); no build latency, no clipped
        // onset). Rebuild it if the device changed; fall back to on-demand capture
        // if no warm mic is available. Audio buffers in the unbounded channel
        // while the VAD initializes, and the processing loop drains it from the
        // first sample.
        let started_warm = {
            let mut wm = self.warm_mic.lock();
            let need_rebuild = wm
                .as_ref()
                .map(|w| w.device_name() != device_name.as_deref())
                .unwrap_or(true);
            if need_rebuild {
                match WarmMic::build(device_name.as_deref()) {
                    Ok(w) => *wm = Some(w),
                    Err(e) => {
                        error!("Warm mic build failed ({e}); using on-demand capture");
                        *wm = None;
                    }
                }
            }
            match wm.as_ref() {
                Some(w) => match w.start(audio_sender.clone(), Some(level_sender.clone())) {
                    Ok(()) => true,
                    Err(e) => {
                        error!("Warm mic start failed ({e}); using on-demand capture");
                        false
                    }
                },
                None => false,
            }
        };
        if !started_warm {
            let capture = match start_capture(device_name.as_deref(), audio_sender, Some(level_sender)) {
                Ok(c) => c,
                Err(e) => {
                    *self.status.lock() = DictationStatus::Idle;
                    let _ = app_handle.emit("dictation-status", DictationStatus::Idle);
                    crate::services::media_control::resume(&self.db);
                    let _ = crate::commands::window::hide_pill(&app_handle);
                    return Err(format!("Failed to start capture: {:?}", e));
                }
            };
            *self.active_capture.lock() = Some(capture);
        }

        // 5. Initialize VAD (neural Silero when available, else energy detector).
        let mut vad = Vad::new(threshold_rms, speech_ms, hangover_ms, self.model_manager.models_dir());

        // 5. Spawn Audio Processing Loop
        let active_capture_clone = self.active_capture.clone();
        let warm_mic_clone = self.warm_mic.clone();
        let status_clone = self.status.clone();
        let app_handle_clone = app_handle.clone();
        let got_audio_thread = got_audio.clone();
        let device_name_rebuild = device_name.clone();
        // History: persist each dictation (transcript + short audio clip).
        let db_hist = self.db.clone();
        let model_hist = model_id.clone();
        let recordings_dir = self
            .model_manager
            .models_dir()
            .parent()
            .map(|p| p.join("recordings"))
            .unwrap_or_else(|| std::path::PathBuf::from("recordings"));
        let save_dictation_audio = self.db.get_setting("save_dictation_audio")
            .unwrap_or(None)
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        std::thread::spawn(move || {
            let mut speech_buffer = Vec::new();
            let mut has_speech_started = false;
            // Buffer the RAW contiguous audio (not just VAD-passed frames) plus a
            // short pre-roll, so Whisper receives natural speech with pauses
            // intact — VAD-only concatenation was clipping/omitting words.
            const PREROLL_SAMPLES: usize = 12000; // ~0.75s @16kHz (onset insurance)
            let mut preroll: VecDeque<f32> = VecDeque::new();

            // Handle levels in a background channel
            let level_app_handle = app_handle_clone.clone();
            std::thread::spawn(move || {
                while let Ok(rms) = level_receiver.recv() {
                    let _ = level_app_handle.emit("dictation-audio-level", rms);
                }
            });

            while let Ok(samples) = audio_receiver.recv() {
                // Mark that audio is genuinely flowing (silence counts — the
                // resampler emits chunks even when quiet; only a dead stream
                // sends nothing). The watchdog relies on this.
                got_audio_thread.store(true, Ordering::Relaxed);
                // Check if we were stopped from outside
                if !matches!(*status_clone.lock(), DictationStatus::Recording) {
                    break;
                }

                let (vad_change, _speech_samples) = vad.process_samples(&samples);

                // Once speech has started, keep the full raw audio (incl. natural
                // pauses); before it, keep a rolling pre-roll for the onset.
                if has_speech_started {
                    speech_buffer.extend_from_slice(&samples);
                } else {
                    preroll.extend(samples.iter().copied());
                    if preroll.len() > PREROLL_SAMPLES {
                        let drop = preroll.len() - PREROLL_SAMPLES;
                        preroll.drain(0..drop);
                    }
                }

                if let Some(speech_started) = vad_change {
                    if speech_started {
                        info!("VAD: Speech started");
                        if !has_speech_started {
                            has_speech_started = true;
                            speech_buffer.extend(preroll.iter().copied());
                            preroll.clear();
                        }
                        let _ = app_handle_clone.emit("dictation-speech-started", ());
                    } else {
                        info!("VAD: Speech ended");
                        let _ = app_handle_clone.emit("dictation-speech-stopped", ());
                        
                        // In AutoStop mode, automatically stop dictation when speech ends
                        if dictation_mode == "AutoStop" && has_speech_started {
                            info!("AutoStop: Silence detected after speech. Triggering transcription.");
                            break;
                        }
                    }
                }
            }

            // Processing phase
            let should_transcribe = {
                let mut status = status_clone.lock();
                if matches!(*status, DictationStatus::Recording) {
                    *status = DictationStatus::Processing;
                    let _ = app_handle_clone.emit("dictation-status", DictationStatus::Processing);
                    true
                } else {
                    // Stopped from outside, check if we need to transcribe
                    matches!(*status, DictationStatus::Processing)
                }
            };

            // Shut down capture BEFORE transcription so the mic indicator turns
            // off immediately: pause the warm mic (kept warm for next time) and
            // drop any on-demand capture used as a fallback.
            if let Some(w) = warm_mic_clone.lock().as_ref() {
                w.stop();
            }
            let mut cap = active_capture_clone.lock();
            *cap = None;

            if should_transcribe && !speech_buffer.is_empty() {
                info!("Transcribing {} speech samples...", speech_buffer.len());
                // This runs on a plain OS thread (no ambient Tokio reactor), so
                // drive the async transcription on Tauri's managed runtime.
                let transcription_res = tauri::async_runtime::block_on(
                    engine.transcribe(&speech_buffer)
                );

                // Persist the raw clip (when saving is enabled) + a history row.
                // Shared by the success and failure paths so a recording is never
                // lost — even if STT failed (dead API, out of credits, HTTP 500…).
                let persist_clip = |text: &str, ai_on: bool| {
                    let id = uuid::Uuid::new_v4().to_string();
                    let duration_ms = (speech_buffer.len() as f64 / 16.0) as i64; // 16 samples/ms @16kHz
                    let mut audio_path: Option<String> = None;
                    if save_dictation_audio {
                        let _ = std::fs::create_dir_all(&recordings_dir);
                        let path = recordings_dir.join(format!("dictation_{}.wav", id));
                        let spec = hound::WavSpec {
                            channels: 1,
                            sample_rate: 16000,
                            bits_per_sample: 32,
                            sample_format: hound::SampleFormat::Float,
                        };
                        if let Ok(mut w) = hound::WavWriter::create(&path, spec) {
                            for &s in &speech_buffer { let _ = w.write_sample(s); }
                            if w.finalize().is_ok() {
                                audio_path = Some(path.to_string_lossy().to_string());
                            }
                        }
                    }
                    let app = db_hist.get_setting("__dictation_target_app").ok().flatten();
                    if let Err(e) = db_hist.add_dictation(
                        &id, text, duration_ms, Some(&model_hist), audio_path.as_deref(),
                        app.as_deref(), ai_on,
                    ) {
                        error!("Failed to save dictation history: {:?}", e);
                    }
                    let _ = app_handle_clone.emit("dictation-history-updated", ());
                    // Keep only the newest 100 audio clips on disk.
                    if let Ok(stale) = db_hist.prune_dictation_audio(100) {
                        for p in stale { let _ = std::fs::remove_file(p); }
                    }
                };

                match transcription_res {
                    Ok(result) => {
                        // Post-process: custom dictionary → punctuation → caps,
                        // then optional AI enhancement (all no-ops when disabled).
                        let mut text = crate::services::text_processing::process(&db_hist, &result.text);
                        text = tauri::async_runtime::block_on(
                            crate::services::text_processing::ai_enhance(&db_hist, text)
                        );
                        info!("Transcription finished: {}", text);
                        let _ = app_handle_clone.emit("dictation-final", text.clone());

                        // Persist to dictation history (skip empty results = silence).
                        if !text.trim().is_empty() {
                            let ai_on = crate::services::text_processing::ai_enhance_enabled(&db_hist);
                            persist_clip(&text, ai_on);

                            if auto_paste {
                                if let Err(e) = TextInjector::inject(&text) {
                                    error!("Failed to inject text: {:?}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Transcription failed: {:?}", e);
                        let _ = app_handle_clone.emit("dictation-error", format!("Transcription failed: {:?}", e));
                        // Durability: don't lose the recording to a transient STT
                        // failure. Keep the audio + a placeholder history row so it
                        // can be replayed / re-transcribed later.
                        if save_dictation_audio && !speech_buffer.is_empty() {
                            persist_clip("(transcription failed — audio saved)", false);
                        }
                    }
                }
            }

            // Resume any media we paused for this session + optional stop cue.
            crate::services::media_control::resume(&db_hist);
            crate::services::sound::cue(&db_hist, crate::services::sound::Cue::Stop);
            let _ = crate::commands::window::hide_pill(&app_handle_clone);

            // Return to Idle
            *status_clone.lock() = DictationStatus::Idle;
            let _ = app_handle_clone.emit("dictation-status", DictationStatus::Idle);
            info!("Dictation service is back to Idle.");

            // Rebuild the warm mic FRESH for the next session. Reusing one
            // long-lived CoreAudio input unit across many pause/resume cycles is
            // what eventually goes silent; a freshly built stream (played once,
            // like the meeting capture) stays reliable. Built here at idle — off
            // the hotkey path — so the next start is still instant. build() does
            // no hardware IO until play(), so no mic indicator appears.
            {
                let wm = warm_mic_clone.clone();
                let dev = device_name_rebuild.clone();
                std::thread::spawn(move || match WarmMic::build(dev.as_deref()) {
                    Ok(w) => { *wm.lock() = Some(w); info!("Warm mic rebuilt for next dictation."); }
                    Err(e) => { error!("Warm mic rebuild failed: {:?}", e); *wm.lock() = None; }
                });
            }
        });

        // Self-heal watchdog (warm path only — a fresh capture can't be stale).
        // If no audio reaches the processing loop shortly after start, the warm
        // CoreAudio unit resumed without actually delivering samples: stop it,
        // transparently switch to a freshly built capture feeding the same
        // channel, and discard the stale warm mic so it's rebuilt next time.
        if started_warm {
            let wd_status = self.status.clone();
            let wd_warm = self.warm_mic.clone();
            let wd_cap = self.active_capture.clone();
            let wd_dev = device_name.clone();
            let wd_got = got_audio.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(600));
                if !matches!(*wd_status.lock(), DictationStatus::Recording) {
                    return; // already stopped/processing — nothing to heal
                }
                if wd_got.load(Ordering::Relaxed) {
                    return; // audio is flowing — healthy
                }
                error!("Dictation: no mic audio after 600ms — warm stream is stale; recovering with a fresh capture.");
                if let Some(w) = wd_warm.lock().as_ref() {
                    w.stop(); // silence the dead stream so only the fresh one feeds
                }
                match start_capture(wd_dev.as_deref(), wd_audio_sender, Some(wd_level_sender)) {
                    Ok(cap) => {
                        *wd_cap.lock() = Some(cap);
                        info!("Dictation: recovered with a fresh capture stream.");
                    }
                    Err(e) => error!("Dictation: recovery capture failed: {:?}", e),
                }
                *wd_warm.lock() = None; // discard the stale warm mic (rebuilt next start)
            });
        }

        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut status = self.status.lock();
        match *status {
            DictationStatus::Recording => {
                info!("Stopping dictation recording manually...");
                *status = DictationStatus::Processing;
                // Signal the thread to finish by updating the status.
                // The thread will clean up the capture stream.
                Ok(())
            }
            DictationStatus::Processing => {
                Err("Dictation is already processing".to_string())
            }
            DictationStatus::Idle => {
                Err("Dictation is not active".to_string())
            }
        }
    }
}
