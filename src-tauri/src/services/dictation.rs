use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
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
                            let dir = crate::stt::ParakeetEngine::model_dir_for(self.model_manager.models_dir(), &model_id);
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
                } else if model_id.contains("moss") {
                    #[cfg(feature = "moss")]
                    {
                        let mp = crate::stt::moss::MossEngine::model_path_default(self.model_manager.models_dir());
                        if !mp.exists() {
                            return Err("The MOSS Transcribe+Diarize model isn't downloaded yet — download it in Settings → AI Providers & Models first.".to_string());
                        }
                        info!("Loading local MOSS-Transcribe-Diarize model from: {:?}", mp);
                        let eng = crate::stt::moss::MossSttEngine::new(&mp, crate::stt::stt_language(&self.db))
                            .map_err(|e| format!("Failed to load MOSS engine: {:?}", e))?;
                        Arc::new(eng) as Arc<dyn SttEngine>
                    }
                    #[cfg(not(feature = "moss"))]
                    { return Err("MOSS support is not built into this build".to_string()); }
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

        // Live transcript preview (FluidVoice-style): fast engines re-transcribe
        // the growing session audio on a timer and stream the words to the pill.
        // Parakeet re-runs the whole prefix comfortably faster than real time;
        // Whisper/API/MOSS engines keep the waveform-only pill.
        let live_preview = model_id.contains("parakeet");
        crate::commands::window::set_pill_expanded(live_preview);

        // Show the dictation pill on the monitor the user is on. Done here (in
        // the service) so it appears for EVERY trigger — global hotkey, tray,
        // or the on-screen button — not just the button path.
        let _ = crate::commands::window::show_pill(&app_handle);

        // Capture the paste target now (off-thread so we never delay capture):
        // the PID owning the focused AX element — the paste is later posted
        // straight to that process — plus the frontmost app name, used for
        // per-app enhancement prompts AND the "Top Apps" stat.
        let target_pid: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
        {
            let db_app = self.db.clone();
            let pid_slot = target_pid.clone();
            std::thread::spawn(move || {
                *pid_slot.lock() = crate::services::focus::focused_app_pid();
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

        // Live-preview machinery: a parallel copy of the session audio for the
        // preview thread, a stop flag, and a gate that serializes ALL model
        // inference so a live pass never runs concurrently with the final pass
        // (FluidVoice serializes CoreML access the same way).
        let preview_buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let preview_stop = Arc::new(AtomicBool::new(false));
        let inference_gate: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
        if live_preview {
            let buf = preview_buf.clone();
            let stop = preview_stop.clone();
            let gate = inference_gate.clone();
            let eng = engine.clone();
            let app = app_handle.clone();
            std::thread::spawn(move || {
                // FluidVoice cadence for offline Parakeet: tick every 600ms,
                // first pass only after 1s of audio, re-transcribing the ENTIRE
                // buffer from t=0 each time (the model is far faster than
                // real time, so the growing prefix stays cheap).
                const TICK_MS: u64 = 600;
                const MIN_SAMPLES: usize = 16_000;
                let mut last_emitted = String::new();
                let mut skip_next = false;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    // Backpressure: if the previous pass overran the tick,
                    // give the machine one tick to breathe.
                    if skip_next {
                        skip_next = false;
                        continue;
                    }
                    // Rolling window: preview only the most recent ~30 s. The
                    // pill shows the tail anyway, and re-running the FULL
                    // growing prefix through a quadratic-memory encoder is the
                    // long-dictation OOM (37 GB observed). Constant window =
                    // constant per-tick cost regardless of session length.
                    const PREVIEW_WINDOW: usize = 30 * 16_000;
                    let snapshot = {
                        let b = buf.lock();
                        let start = b.len().saturating_sub(PREVIEW_WINDOW);
                        b[start..].to_vec()
                    };
                    if snapshot.len() < MIN_SAMPLES {
                        continue;
                    }
                    let started = std::time::Instant::now();
                    let text = {
                        let _serialized = gate.lock();
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        // Same preprocessing as the commits and the final tail
                        // — preview must predict the paste, not diverge from it.
                        match transcribe_normalized(&eng, &snapshot) {
                            Ok(t) => t.trim().to_string(),
                            Err(e) => {
                                log::warn!("Live preview pass failed: {:?}", e);
                                skip_next = true;
                                continue;
                            }
                        }
                    };
                    if started.elapsed().as_millis() as u64 > TICK_MS {
                        skip_next = true;
                    }
                    if text.is_empty() || stop.load(Ordering::Relaxed) {
                        continue;
                    }
                    if text != last_emitted {
                        last_emitted = text.clone();
                        let _ = app.emit("dictation-partial", text);
                    }
                }
                info!("Live preview thread finished.");
            });
        }

        // Rolling commit worker: long dictations are transcribed WHILE the
        // user is still speaking. The audio loop cuts a ~30s chunk at a quiet
        // point whenever enough uncommitted audio accumulates and sends it
        // here; on stop only the short tail remains, so stop→paste is
        // near-instant instead of paying the whole transcription at the end
        // (the FluidVoice architecture). Serialized on the inference gate
        // with the preview and the final tail pass.
        let (commit_tx, commit_rx) = unbounded::<Vec<f32>>();
        let committed_texts: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let committer_handle = {
            let engine = engine.clone();
            let gate = inference_gate.clone();
            let texts = committed_texts.clone();
            std::thread::spawn(move || {
                while let Ok(chunk) = commit_rx.recv() {
                    let _serialized = gate.lock();
                    match transcribe_normalized(&engine, &chunk) {
                        Ok(t) => texts.lock().push(t.trim().to_string()),
                        Err(e) => {
                            // Push a placeholder so ordering survives; the
                            // audio itself is still in the session buffer and
                            // saved to history, so nothing is lost.
                            log::warn!("Rolling commit failed (chunk kept in history audio): {:?}", e);
                            texts.lock().push(String::new());
                        }
                    }
                }
            })
        };

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
        let target_pid_thread = target_pid.clone();
        let preview_buf_thread = preview_buf.clone();
        let preview_stop_thread = preview_stop.clone();
        let inference_gate_thread = inference_gate.clone();

        std::thread::spawn(move || {
            // FluidVoice-style session capture: buffer the ENTIRE recording from
            // t0 and let the VAD only ANNOTATE where speech happened. The old
            // design used the VAD as a gatekeeper (keep audio only after its
            // first trigger, 0.75s pre-roll) — any late/missed trigger silently
            // dropped real words, which made quiet mics feel "random".
            let mut session_buffer: Vec<f32> = Vec::new();
            // Hard cap so a forgotten toggle can't grow unbounded (~46MB).
            const MAX_SESSION_SAMPLES: usize = 16000 * 60 * 12; // 12 minutes
            // Loudest chunk seen this session — reported when nothing was
            // detected, so a too-quiet mic is diagnosable instead of silent.
            let mut max_chunk_rms = 0f32;
            // VAD speech envelope, as offsets into session_buffer: position of
            // the first "speech started" and the last "speech ended" (the latter
            // already includes the ~800ms hangover of room tone past the last
            // word). Used to trim dead air at both ends before STT — long
            // non-speech stretches make Whisper/Audio8 hallucinate tokens.
            // VAD is annotation-only for dictation: it drives AutoStop and the
            // pill's speech events, but never gates or windows the audio.
            let mut first_speech_mark: Option<usize> = None;
            // Samples already handed to the rolling-commit worker.
            let mut committed_end: usize = 0;

            // Handle levels in a background channel
            let level_app_handle = app_handle_clone.clone();
            std::thread::spawn(move || {
                while let Ok(rms) = level_receiver.recv() {
                    let _ = level_app_handle.emit("dictation-audio-level", rms);
                }
            });

            // recv with a timeout, NOT a blocking recv: once `stop()` stamps the
            // session stop mark, the drain thread trims ALL later audio and may
            // never send another chunk — a blocking recv would then hang this
            // loop forever. The timeout lets us notice the status flip anyway.
            loop {
                let samples = match audio_receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(s) => Some(s),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                };
                let stopping = !matches!(*status_clone.lock(), DictationStatus::Recording);
                let Some(samples) = samples else {
                    if stopping {
                        break;
                    }
                    continue;
                };
                // Mark that audio is genuinely flowing (silence counts — the
                // resampler emits chunks even when quiet; only a dead stream
                // sends nothing). The watchdog relies on this.
                got_audio_thread.store(true, Ordering::Relaxed);

                if !samples.is_empty() {
                    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
                    max_chunk_rms = max_chunk_rms.max((sum_sq / samples.len() as f32).sqrt());
                }

                let (vad_change, _speech_samples) = vad.process_samples(&samples);

                if session_buffer.len() < MAX_SESSION_SAMPLES {
                    session_buffer.extend_from_slice(&samples);
                    if live_preview {
                        preview_buf_thread.lock().extend_from_slice(&samples);
                    }
                }

                // Rolling commit: enough uncommitted audio → cut at the
                // quietest point near 30s and hand it to the committer.
                const COMMIT_TRIGGER: usize = 32 * 16_000;
                const COMMIT_TARGET: usize = 30 * 16_000;
                if session_buffer.len() - committed_end >= COMMIT_TRIGGER {
                    let cut = chunk_cut_point(&session_buffer[committed_end..], COMMIT_TARGET);
                    let chunk = session_buffer[committed_end..committed_end + cut].to_vec();
                    committed_end += cut;
                    info!(
                        "Dictation: rolling commit of {:.1}s (committed through {:.1}s).",
                        chunk.len() as f64 / 16_000.0,
                        committed_end as f64 / 16_000.0
                    );
                    let _ = commit_tx.send(chunk);
                }

                if let Some(speech_started) = vad_change {
                    if speech_started {
                        info!("VAD: Speech started");
                        if first_speech_mark.is_none() {
                            first_speech_mark = Some(session_buffer.len());
                        }
                        let _ = app_handle_clone.emit("dictation-speech-started", ());
                    } else {
                        info!("VAD: Speech ended");
                        let _ = app_handle_clone.emit("dictation-speech-stopped", ());

                        // In AutoStop mode, automatically stop dictation when speech ends
                        if dictation_mode == "AutoStop" && first_speech_mark.is_some() {
                            info!("AutoStop: Silence detected after speech. Triggering transcription.");
                            break;
                        }
                    }
                }

                // Stop AFTER buffering the chunk that carried the stop signal, so
                // the words spoken right up to the hotkey release are kept.
                if stopping {
                    break;
                }
            }

            // The session is over: stop the live preview loop. The final pass
            // below serializes on the inference gate, so an in-flight live
            // pass finishes before the final transcription starts.
            preview_stop_thread.store(true, Ordering::Relaxed);

            // Processing phase
            let should_transcribe = {
                let mut status = status_clone.lock();
                if matches!(*status, DictationStatus::Recording) {
                    *status = DictationStatus::Processing;
                    true
                } else {
                    // Stopped from outside, check if we need to transcribe
                    matches!(*status, DictationStatus::Processing)
                }
            };
            if should_transcribe {
                // Emitted here (not only on the in-thread transition) because a
                // manual stop() flips the status without an AppHandle — the pill
                // relies on this event to switch to its processing spinner.
                let _ = app_handle_clone.emit("dictation-status", DictationStatus::Processing);
            }

            // Shut down capture BEFORE transcription so the mic indicator turns
            // off immediately: pause the warm mic (kept warm for next time) and
            // drop any on-demand capture used as a fallback. Both stamp the
            // session stop time (if `stop()` didn't already), drain the packet
            // ring trimmed to that mark, and flush the tail into the channel
            // before returning.
            if let Some(w) = warm_mic_clone.lock().as_ref() {
                w.stop();
            }
            {
                let mut cap = active_capture_clone.lock();
                *cap = None;
            }

            // Now drain the flushed tail so the final phoneme isn't clipped.
            // Session-scoped host-time trimming upstream guarantees this can
            // only ever contain audio from THIS session's window.
            while let Ok(samples) = audio_receiver.try_recv() {
                if session_buffer.len() >= MAX_SESSION_SAMPLES {
                    break;
                }
                session_buffer.extend_from_slice(&samples);
            }

            // FluidVoice-style: transcribe the WHOLE session, always. The VAD
            // no longer windows the audio — the old envelope trim silently
            // dropped quiet speech the detector missed (trailing words the
            // live preview had shown), and a cut mid-phoneme decodes as a
            // different word. The models handle leading/trailing silence fine,
            // and a truly empty result triggers the quiet-mic diagnostic below.
            let mut speech_buffer: Vec<f32> = session_buffer;

            if should_transcribe && speech_buffer.is_empty() {
                info!("Dictation: empty session — no audio was captured.");
                let _ = app_handle_clone.emit(
                    "dictation-error",
                    "No audio was captured — check the input device in Settings.",
                );
            }

            if should_transcribe && !speech_buffer.is_empty() {
                info!(
                    "Transcribing tail: {} of {} samples ({} already committed)...",
                    speech_buffer.len().saturating_sub(committed_end),
                    speech_buffer.len(),
                    committed_end
                );
                // Shut the committer down and wait for queued chunks BEFORE
                // taking the inference gate ourselves — the committer needs
                // the gate, so taking it first would deadlock the join.
                drop(commit_tx);
                if let Err(e) = committer_handle.join() {
                    error!("Rolling-commit worker panicked: {:?}", e);
                }
                // Only the tail is left to transcribe — the rolling commits
                // handled everything before committed_end while the user was
                // still speaking, which is what makes stop→paste fast.
                // Preprocessing (pad + normalize) is identical across
                // preview/commits/tail via transcribe_normalized.
                let transcription_res = (|| {
                    let _inference_serialized = inference_gate_thread.lock();
                    let t0 = std::time::Instant::now();
                    let tail_start = committed_end.min(speech_buffer.len());
                    let tail = transcribe_long(&engine, &speech_buffer[tail_start..])?;
                    let mut parts: Vec<String> = committed_texts
                        .lock()
                        .iter()
                        .filter(|p| !p.trim().is_empty())
                        .cloned()
                        .collect();
                    if !tail.text.trim().is_empty() {
                        parts.push(tail.text.trim().to_string());
                    }
                    Ok::<_, anyhow::Error>(crate::stt::TranscriptionResult {
                        text: parts.join(" "),
                        segments: Vec::new(),
                        language: None,
                        processing_time_ms: t0.elapsed().as_millis() as u64,
                    })
                })();

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
                                // Target the app that had focus at dictation
                                // start (paste posts straight to its PID).
                                let pid = *target_pid_thread.lock();
                                if let Err(e) = TextInjector::inject(&text, pid) {
                                    error!("Failed to inject text: {:?}", e);
                                    // Surface it — a silent paste failure looks
                                    // like the app ate the dictation (the text
                                    // is still in history + clipboard).
                                    let _ = app_handle_clone.emit(
                                        "dictation-error",
                                        format!("Auto-paste failed: {}", e),
                                    );
                                }
                            }
                        } else {
                            // Nothing recognized in the whole session. Most
                            // common cause: a mic signal too quiet (low input
                            // volume, wrong device, speaking away from it).
                            info!(
                                "Dictation: empty transcription (loudest chunk RMS {:.4}).",
                                max_chunk_rms
                            );
                            let _ = app_handle_clone.emit(
                                "dictation-error",
                                if max_chunk_rms < 0.02 {
                                    "No speech detected — the mic signal was very quiet. Check System Settings → Sound → Input (level and device)."
                                } else {
                                    "No speech detected in that recording."
                                },
                            );
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
        {
            let mut status = self.status.lock();
            match *status {
                DictationStatus::Recording => {
                    info!("Stopping dictation recording manually...");
                    *status = DictationStatus::Processing;
                    // Signal the thread to finish by updating the status.
                    // The thread will clean up the capture stream.
                }
                DictationStatus::Processing => {
                    return Err("Dictation is already processing".to_string());
                }
                DictationStatus::Idle => {
                    return Err("Dictation is not active".to_string());
                }
            }
        }
        // Stamp the session end NOW (hotkey release), not when the processing
        // thread gets around to tearing capture down — audio captured after
        // this instant is trimmed out by the drain thread.
        if let Some(w) = self.warm_mic.lock().as_ref() {
            w.mark_stop();
        }
        if let Some(c) = self.active_capture.lock().as_ref() {
            c.mark_stop();
        }
        Ok(())
    }
}

/// Transcribe audio of any length with bounded memory: encoder self-attention
/// grows quadratically with input length, so anything beyond ~30 s is split
/// into chunks — cut at the quietest 200 ms window near each boundary so words
/// aren't sliced mid-syllable — transcribed sequentially, and joined.
/// Pad + peak-normalize + transcribe one bounded piece of audio. ONE shared
/// preprocessing path for the live preview, the rolling commits, and the
/// final tail — if they diverge, the pasted text stops matching what the
/// pill showed (that bug shipped once).
fn transcribe_normalized(
    engine: &std::sync::Arc<dyn SttEngine>,
    audio: &[f32],
) -> anyhow::Result<String> {
    // whisper.cpp asserts on sub-1s buffers; short bursts get padded.
    const MIN_STT_SAMPLES: usize = 16_000;
    let mut buf = audio.to_vec();
    if buf.len() < MIN_STT_SAMPLES {
        buf.resize(MIN_STT_SAMPLES, 0.0);
    }
    // Consumer mic levels often sit far below full scale; quiet input degrades
    // every engine. Gain is capped so noise-only audio isn't amplified.
    let peak = buf.iter().fold(0f32, |m, &s| m.max(s.abs()));
    if peak > 0.0 && peak < 0.5 {
        let gain = (0.9 / peak).min(20.0);
        for s in buf.iter_mut() {
            *s *= gain;
        }
    }
    Ok(tauri::async_runtime::block_on(engine.transcribe(&buf))?.text)
}

fn transcribe_long(
    engine: &std::sync::Arc<dyn SttEngine>,
    audio: &[f32],
) -> anyhow::Result<crate::stt::TranscriptionResult> {
    // 30s chunks, cut at quiet points — the same scale the live preview uses.
    // Parakeet's accuracy audibly degrades on single passes much longer than
    // its ~30s training window (verified side-by-side: a 67.5s single pass
    // mangled words the 30s preview had gotten right), and encoder attention
    // memory grows quadratically on top.
    const MAX_CHUNK: usize = 30 * 16_000;

    let t0 = std::time::Instant::now();
    let mut text = String::new();
    let mut pos = 0usize;
    while pos < audio.len() {
        let remaining = &audio[pos..];
        let end = chunk_cut_point(remaining, MAX_CHUNK);
        let piece = transcribe_normalized(engine, &remaining[..end])?;
        let piece = piece.trim();
        if !piece.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(piece);
        }
        pos += end;
    }
    Ok(crate::stt::TranscriptionResult {
        text,
        segments: Vec::new(),
        language: None,
        processing_time_ms: t0.elapsed().as_millis() as u64,
    })
}

/// Where to cut the next chunk: the middle of the quietest 200 ms window in
/// the last 5 s before `max_len` (falls back to `max_len` for short inputs).
fn chunk_cut_point(audio: &[f32], max_len: usize) -> usize {
    const SEARCH: usize = 5 * 16_000;
    const WIN: usize = 3_200; // 200 ms @ 16 kHz

    if audio.len() <= max_len {
        return audio.len();
    }
    let lo = max_len.saturating_sub(SEARCH);
    let hi = max_len;
    let mut best_cut = hi;
    let mut best_energy = f32::MAX;
    let mut i = lo;
    while i + WIN <= hi {
        let energy: f32 = audio[i..i + WIN].iter().map(|s| s * s).sum();
        if energy < best_energy {
            best_energy = energy;
            best_cut = i + WIN / 2;
        }
        i += WIN / 2;
    }
    best_cut
}

#[cfg(test)]
mod chunking_tests {
    use super::chunk_cut_point;

    #[test]
    fn short_audio_is_one_chunk() {
        let audio = vec![0.1f32; 16_000];
        assert_eq!(chunk_cut_point(&audio, 30 * 16_000), audio.len());
    }

    #[test]
    fn cuts_at_quiet_point_near_boundary() {
        let max = 30 * 16_000;
        // Loud everywhere except a silent 200ms pocket at 27s.
        let mut audio = vec![0.5f32; max + 16_000];
        let quiet_at = 27 * 16_000;
        for s in audio[quiet_at..quiet_at + 3_200].iter_mut() {
            *s = 0.0;
        }
        let cut = chunk_cut_point(&audio, max);
        assert!(cut >= quiet_at && cut <= quiet_at + 3_200, "cut={cut}");
    }

    #[test]
    fn cut_never_exceeds_max() {
        let audio = vec![0.5f32; 40 * 16_000];
        assert!(chunk_cut_point(&audio, 30 * 16_000) <= 30 * 16_000);
    }
}
