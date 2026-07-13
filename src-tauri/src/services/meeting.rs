use std::sync::Arc;
use std::time::Duration;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Write, BufWriter};
use std::path::Path;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use log::{info, error, warn};
use crossbeam_channel::unbounded;

use crate::storage::Database;
use crate::audio::{start_capture, MicCapture, start_system_capture, SystemCapture, Vad};
use crate::stt::{WhisperEngine, ModelManager, SttEngine, ApiSttEngine};
use crate::diarization::{DiarizationEngine, SpeakerClustering};

#[derive(Debug, Clone, serde::Serialize)]
pub enum MeetingStatus {
    Idle,
    Recording,
    Paused,
    Processing,
}

#[derive(Clone)]
pub struct MeetingService {
    db: Database,
    model_manager: ModelManager,
    active_mic_capture: Arc<Mutex<Option<MicCapture>>>,
    active_sys_capture: Arc<Mutex<Option<SystemCapture>>>,
    engine_cache: Arc<Mutex<Option<(String, Arc<dyn SttEngine>)>>>,
    status: Arc<Mutex<MeetingStatus>>,
    active_meeting_id: Arc<Mutex<Option<String>>>,
    clustering: Arc<Mutex<Option<Arc<Mutex<SpeakerClustering>>>>>,
}

impl MeetingService {
    pub fn new(db: Database, model_manager: ModelManager) -> Self {
        Self {
            db,
            model_manager,
            active_mic_capture: Arc::new(Mutex::new(None)),
            active_sys_capture: Arc::new(Mutex::new(None)),
            engine_cache: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(MeetingStatus::Idle)),
            active_meeting_id: Arc::new(Mutex::new(None)),
            clustering: Arc::new(Mutex::new(None)),
        }
    }

    pub fn get_status(&self) -> MeetingStatus {
        self.status.lock().clone()
    }

    pub fn get_active_meeting_id(&self) -> Option<String> {
        self.active_meeting_id.lock().clone()
    }

    pub fn start(&self, app_handle: AppHandle, title: String) -> Result<String, String> {
        let mut status_guard = self.status.lock();
        if !matches!(*status_guard, MeetingStatus::Idle) {
            return Err("Meeting is already active".to_string());
        }

        // 1. Load settings
        let model_id = self.db.get_setting("meeting_stt_model")
            .unwrap_or(None)
            .or_else(|| self.db.get_setting("dictation_stt_model").unwrap_or(None))
            .unwrap_or_else(|| "whisper-tiny-q5".to_string());

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

        let mic_device_name = self.db.get_setting("active_audio_device").unwrap_or(None);

        // Whether to save the mixed recording to disk (enables playback + export).
        let save_audio = self.db.get_setting("save_audio")
            .unwrap_or(None)
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        // recordings dir = <app_data>/recordings  (models_dir is <app_data>/models)
        let recordings_dir = self
            .model_manager
            .models_dir()
            .parent()
            .map(|p| p.join("recordings"))
            .unwrap_or_else(|| std::path::PathBuf::from("recordings"));

        let diarization_threshold = self.db.get_setting("diarization_threshold")
            .unwrap_or(None)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.20);

        info!("Starting meeting \"{}\" with model: {}, VAD threshold: {}, Diarization threshold: {}", 
              title, model_id, threshold_rms, diarization_threshold);

        // 2. Load STT Engine (Lazy Loading)
        let engine = {
            let mut cache = self.engine_cache.lock();
            
            // Meeting transcription can use its own provider; fall back to the
            // shared dictation STT provider, then to embedded.
            let provider_id = self.db.get_setting("meeting_stt_provider")
                .unwrap_or(None)
                .or_else(|| self.db.get_setting("default_stt_provider").unwrap_or(None))
                .unwrap_or_else(|| "embedded".to_string());

            // Prefetch the provider so the cache key includes its model —
            // model changes must invalidate the cached engine.
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
                    // Per-task STT model (not the connection default_model,
                    // which may be an LLM used for summaries).
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
                loaded
            } else {
                cache.as_ref().unwrap().1.clone()
            }
        };

        // 3. Create Meeting in DB
        let meeting_id = uuid::Uuid::new_v4().to_string();
        self.db.create_meeting(&meeting_id, &title, "recording")
            .map_err(|e| format!("Failed to create meeting in database: {:?}", e))?;
        self.db.set_setting("active_meeting_id", &meeting_id)
            .map_err(|e| format!("Failed to update active meeting setting: {:?}", e))?;

        *self.active_meeting_id.lock() = Some(meeting_id.clone());

        // 4. Initialize Diarization components
        let diarizer = Arc::new(DiarizationEngine::new());
        let clustering_engine = Arc::new(Mutex::new(SpeakerClustering::new(self.db.clone(), diarization_threshold)));
        *self.clustering.lock() = Some(clustering_engine.clone());

        // 5. Update Status
        *status_guard = MeetingStatus::Recording;
        let _ = app_handle.emit("meeting-status", MeetingStatus::Recording);

        // 6. Audio capture channels
        let (mic_sender, mic_receiver) = unbounded::<Vec<f32>>();
        let (sys_sender, sys_receiver) = unbounded::<Vec<f32>>();

        // 7. Initialize VAD (neural Silero when available, else energy detector)
        let mut vad = Vad::new(threshold_rms, speech_ms, hangover_ms, self.model_manager.models_dir());

        // 8. Spawn Processing Thread
        let status_clone = self.status.clone();
        let app_handle_clone = app_handle.clone();
        let db_clone = self.db.clone();
        let meeting_id_clone = meeting_id.clone();
        let mic_capture_guard = self.active_mic_capture.clone();
        let sys_capture_guard = self.active_sys_capture.clone();
        let active_meeting_id_clone = self.active_meeting_id.clone();
        let save_audio_thread = save_audio;
        let recordings_dir_thread = recordings_dir.clone();

        std::thread::spawn(move || {
            let mut speech_buffer = Vec::new();
            let mut segment_start_time = 0.0;
            let mut elapsed_samples = 0;

            // Utterance buffering: capture the RAW contiguous audio for a
            // speech segment (not just VAD-passed frames) plus a short pre-roll,
            // so onset/boundary words are never clipped.
            const PREROLL_SAMPLES: usize = 4800; // ~0.3s @16kHz
            let mut in_speech = false;
            let mut preroll: VecDeque<f32> = VecDeque::new();

            // Full recording buffer — accumulated when saving audio to disk or
            // when the neural-diarization finalize pass needs the whole recording.
            let accumulate_audio = save_audio_thread || cfg!(feature = "neural-diarization");
            let mut full_audio: Vec<f32> = Vec::new();
            // Emit a transcription-failure notice at most once per meeting so a
            // dead remote provider (e.g. out of credits) doesn't spam the UI.
            let mut transcribe_error_notified = false;

            // Crash-safe audio: while recording we ALWAYS stream the mixed audio to
            // a raw f32 sidecar on disk (flushed ~1×/sec), regardless of the "save
            // recordings" setting. This is a safety net, not the saved artifact:
            //  - clean stop + save ON  → written to the permanent WAV, sidecar removed
            //  - clean stop + save OFF → sidecar removed (nothing persisted — the
            //                            user opted out of saving)
            //  - unclean stop (crash / force-quit / power loss, EITHER setting)
            //                          → sidecar survives and is recovered into a WAV
            //                            on next launch, so a recording is never lost.
            let part_path = recordings_dir_thread.join(format!("{}.f32.part", meeting_id_clone));
            let _ = std::fs::create_dir_all(&recordings_dir_thread);
            let mut audio_sink: Option<BufWriter<File>> = match File::create(&part_path) {
                Ok(f) => Some(BufWriter::new(f)),
                Err(e) => {
                    error!("Could not open crash-safe audio sidecar {:?}: {:?}", part_path, e);
                    None
                }
            };
            let mut samples_since_flush: usize = 0;

            info!("Meeting audio mixing & diarization thread started.");

            loop {
                // Check status
                let current_status = status_clone.lock().clone();
                if matches!(current_status, MeetingStatus::Idle) {
                    break;
                }

                if matches!(current_status, MeetingStatus::Paused) {
                    // Drain queues to avoid buildup while paused
                    while let Ok(_) = mic_receiver.try_recv() {}
                    while let Ok(_) = sys_receiver.try_recv() {}
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }

                // Block on mic_receiver
                match mic_receiver.recv() {
                    Ok(mic_samples) => {
                        let mut mixed_samples = mic_samples.clone();
                        // Non-blocking read system audio
                        if let Ok(sys_samples) = sys_receiver.try_recv() {
                            let len = mixed_samples.len().min(sys_samples.len());
                            for i in 0..len {
                                mixed_samples[i] = (mixed_samples[i] + sys_samples[i]) * 0.5;
                            }
                        }

                        let current_time = elapsed_samples as f64 / 16000.0;
                        elapsed_samples += mixed_samples.len();

                        if accumulate_audio {
                            full_audio.extend_from_slice(&mixed_samples);
                        }

                        // Durable append to the crash-safe sidecar (raw f32 LE).
                        if let Some(sink) = audio_sink.as_mut() {
                            let mut bytes = Vec::with_capacity(mixed_samples.len() * 4);
                            for &s in &mixed_samples {
                                bytes.extend_from_slice(&s.to_le_bytes());
                            }
                            if let Err(e) = sink.write_all(&bytes) {
                                error!("Failed writing crash-safe audio sidecar; disabling it (recording continues): {:?}", e);
                                audio_sink = None;
                            } else {
                                samples_since_flush += mixed_samples.len();
                                if samples_since_flush >= 16000 {
                                    let _ = sink.flush();
                                    samples_since_flush = 0;
                                }
                            }
                        }

                        let (vad_change, _speech_samples) = vad.process_samples(&mixed_samples);

                        // While speaking, accumulate raw contiguous audio; while
                        // silent, keep a rolling pre-roll for the next onset.
                        if in_speech {
                            speech_buffer.extend_from_slice(&mixed_samples);
                        } else {
                            preroll.extend(mixed_samples.iter().copied());
                            if preroll.len() > PREROLL_SAMPLES {
                                let drop = preroll.len() - PREROLL_SAMPLES;
                                preroll.drain(0..drop);
                            }
                        }

                        if let Some(speech_started) = vad_change {
                            if speech_started {
                                info!("Meeting VAD: Speech started");
                                in_speech = true;
                                // Seed with the pre-roll (already includes this chunk).
                                speech_buffer.clear();
                                speech_buffer.extend(preroll.iter().copied());
                                preroll.clear();
                                let buffered = speech_buffer.len() as f64 / 16000.0;
                                segment_start_time = (current_time - buffered).max(0.0);
                            } else {
                                info!("Meeting VAD: Speech ended");
                                in_speech = false;
                                if !speech_buffer.is_empty() {
                                    let segment_end_time = elapsed_samples as f64 / 16000.0;
                                    
                                    // Transcribe Segment (plain thread → use Tauri's runtime)
                                    let text = match tauri::async_runtime::block_on(engine.transcribe(&speech_buffer)) {
                                        Ok(res) => res.text.trim().to_string(),
                                        Err(e) => {
                                            error!("Meeting transcription error: {:?}", e);
                                            // Surface it once so the user knows why the
                                            // transcript went quiet (bad API key, quota /
                                            // credits exhausted, network down, …).
                                            if !transcribe_error_notified {
                                                transcribe_error_notified = true;
                                                let _ = app_handle_clone.emit("meeting-error", serde_json::json!({
                                                    "meeting_id": meeting_id_clone,
                                                    "message": format!("Transcription failed — recording continues but no new text will appear. ({})", e),
                                                }));
                                            }
                                            "".to_string()
                                        }
                                    };

                                    if !text.is_empty() {
                                        // Diarize Segment
                                        let embedding = diarizer.extract_embedding(&speech_buffer, 16000.0);
                                        let (speaker_id, speaker_name) = clustering_engine.lock().match_or_create_speaker(&embedding);

                                        // Persist Segment
                                        let segment_id = uuid::Uuid::new_v4().to_string();
                                        if let Err(e) = db_clone.add_segment(
                                            &segment_id,
                                            &meeting_id_clone,
                                            Some(&speaker_id),
                                            segment_start_time,
                                            segment_end_time,
                                            &text
                                        ) {
                                            error!("Failed to save meeting segment: {:?}", e);
                                        }

                                        // Emit Update Event
                                        let event_payload = serde_json::json!({
                                            "meeting_id": meeting_id_clone,
                                            "segment": {
                                                "id": segment_id,
                                                "meeting_id": meeting_id_clone.clone(),
                                                "speaker_id": Some(speaker_id.clone()),
                                                "speaker_name": speaker_name,
                                                "start_time": segment_start_time,
                                                "end_time": segment_end_time,
                                                "text": text,
                                                "created_at": chrono::Utc::now().to_rfc3339()
                                            }
                                        });
                                        let _ = app_handle_clone.emit("meeting-transcript-update", event_payload);
                                    }

                                    speech_buffer.clear();
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // The audio capture channel closed while we still thought we
                        // were recording — the mic/system-audio stream died (device
                        // unplugged, format change, OS revoked access, …). Don't stop
                        // silently: log it and tell the UI so it can notify the user
                        // instead of leaving a ghost timer running forever.
                        error!("Meeting audio capture channel closed unexpectedly — stopping recording.");
                        let _ = app_handle_clone.emit("meeting-error", serde_json::json!({
                            "meeting_id": meeting_id_clone,
                            "message": "Recording stopped: the audio capture device ended unexpectedly. Check your microphone / system-audio input and start again.",
                        }));
                        break;
                    }
                }
            }

            // Flush remaining speech buffer on finish
            if !speech_buffer.is_empty() {
                let segment_end_time = elapsed_samples as f64 / 16000.0;
                let text = match tauri::async_runtime::block_on(engine.transcribe(&speech_buffer)) {
                    Ok(res) => res.text.trim().to_string(),
                    Err(e) => {
                        error!("Meeting final flush transcription error: {:?}", e);
                        "".to_string()
                    }
                };

                if !text.is_empty() {
                    let embedding = diarizer.extract_embedding(&speech_buffer, 16000.0);
                    let (speaker_id, speaker_name) = clustering_engine.lock().match_or_create_speaker(&embedding);
                    let segment_id = uuid::Uuid::new_v4().to_string();
                    let _ = db_clone.add_segment(
                        &segment_id,
                        &meeting_id_clone,
                        Some(&speaker_id),
                        segment_start_time,
                        segment_end_time,
                        &text
                    );

                    let event_payload = serde_json::json!({
                        "meeting_id": meeting_id_clone,
                        "segment": {
                            "id": segment_id,
                            "meeting_id": meeting_id_clone.clone(),
                            "speaker_id": Some(speaker_id.clone()),
                            "speaker_name": speaker_name,
                            "start_time": segment_start_time,
                            "end_time": segment_end_time,
                            "text": text,
                            "created_at": chrono::Utc::now().to_rfc3339()
                        }
                    });
                    let _ = app_handle_clone.emit("meeting-transcript-update", event_payload);
                }
            }

            // Neural diarization finalize pass (pyannote via speakrs) — runs once
            // over the full recording and emits real speaker turns to the UI.
            #[cfg(feature = "neural-diarization")]
            {
                if !full_audio.is_empty() {
                    info!("Running neural diarization over {} samples...", full_audio.len());
                    let _ = app_handle_clone.emit("meeting-diarizing", serde_json::json!({ "meeting_id": meeting_id_clone, "status": "running" }));
                    match crate::diarization::NeuralDiarizer::new() {
                        Ok(mut nd) => match nd.diarize(&full_audio) {
                            Ok(turns) => {
                                info!("Neural diarization produced {} speaker turns", turns.len());

                                // Relabel each stored segment with the neural speaker
                                // whose turn overlaps it most, so the UI shows accurate
                                // speaker identity (overrides the streaming clustering).
                                if let Ok(spans) = db_clone.list_segment_spans(&meeting_id_clone) {
                                    use crate::diarization::NeuralDiarizer;
                                    let mut ensured = std::collections::HashSet::new();
                                    for (seg_id, start, end) in spans {
                                        if let Some(label) =
                                            NeuralDiarizer::speaker_for_span(&turns, start, end)
                                        {
                                            // Stable speaker id per meeting + label.
                                            let spk_id = format!("neural_{}_{}", meeting_id_clone, label);
                                            if ensured.insert(spk_id.clone()) {
                                                // "SPEAKER_00" -> "Speaker 1"
                                                let display = label
                                                    .rsplit('_')
                                                    .next()
                                                    .and_then(|n| n.parse::<u32>().ok())
                                                    .map(|n| format!("Speaker {}", n + 1))
                                                    .unwrap_or_else(|| label.clone());
                                                let _ = db_clone.upsert_speaker(&spk_id, &display);
                                            }
                                            let _ = db_clone.update_segment_speaker(&seg_id, &spk_id);
                                        }
                                    }
                                }

                                let payload = serde_json::json!({
                                    "meeting_id": meeting_id_clone,
                                    "turns": turns.iter().map(|t| serde_json::json!({
                                        "start": t.start,
                                        "end": t.end,
                                        "speaker": t.speaker,
                                    })).collect::<Vec<_>>(),
                                });
                                let _ = app_handle_clone.emit("meeting-diarization", payload);
                                // Nudge the UI to re-fetch the (now relabeled) transcript.
                                let _ = app_handle_clone.emit(
                                    "meeting-transcript-update",
                                    serde_json::json!({ "meeting_id": meeting_id_clone, "reload": true }),
                                );
                            }
                            Err(e) => error!("Neural diarization failed: {:?}", e),
                        },
                        Err(e) => error!("Neural diarizer load failed: {:?}", e),
                    }
                    let _ = app_handle_clone.emit("meeting-diarizing", serde_json::json!({ "meeting_id": meeting_id_clone, "status": "done" }));
                }
            }

            // Flush & close the crash-safe sidecar before we finalize.
            if let Some(mut sink) = audio_sink.take() {
                let _ = sink.flush();
            }

            // Save the mixed recording to disk (16kHz mono f32 WAV) if enabled,
            // and remember its path so the UI can offer playback + export.
            if save_audio_thread && !full_audio.is_empty() {
                let _ = std::fs::create_dir_all(&recordings_dir_thread);
                let path = recordings_dir_thread.join(format!("{}.wav", meeting_id_clone));
                let spec = hound::WavSpec {
                    channels: 1,
                    sample_rate: 16000,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                };
                match hound::WavWriter::create(&path, spec) {
                    Ok(mut writer) => {
                        for &s in &full_audio {
                            let _ = writer.write_sample(s);
                        }
                        if writer.finalize().is_ok() {
                            let _ = db_clone.set_setting(
                                &format!("audio_path::{}", meeting_id_clone),
                                &path.to_string_lossy(),
                            );
                            info!("Saved meeting audio to {:?}", path);
                        }
                    }
                    Err(e) => error!("Failed to create WAV writer: {:?}", e),
                }
            }
            // Clean stop: the WAV (or an empty recording) is now the source of
            // truth — drop the crash-recovery sidecar so it isn't recovered again.
            let _ = std::fs::remove_file(&part_path);

            // Update meeting duration on stop
            let duration_seconds = (elapsed_samples as f64 / 16000.0) as i32;
            if let Err(e) = db_clone.update_meeting_duration(&meeting_id_clone, duration_seconds) {
                error!("Failed to update meeting duration: {:?}", e);
            }

            // Cleanup active objects
            *mic_capture_guard.lock() = None;
            *sys_capture_guard.lock() = None;
            *active_meeting_id_clone.lock() = None;
            let _ = db_clone.set_setting("active_meeting_id", "");
            
            *status_clone.lock() = MeetingStatus::Idle;
            let _ = app_handle_clone.emit("meeting-status", MeetingStatus::Idle);
            info!("Meeting processing and capture completely stopped.");
        });

        // 9. Start capture streams
        let mic_capture = start_capture(
            mic_device_name.as_deref(),
            mic_sender,
            None
        ).map_err(|e| format!("Failed to start microphone capture: {:?}", e))?;

        let sys_capture = start_system_capture(
            sys_sender,
            None
        ).map_err(|e| format!("Failed to start system loopback capture: {:?}", e))?;

        *self.active_mic_capture.lock() = Some(mic_capture);
        *self.active_sys_capture.lock() = Some(sys_capture);

        Ok(meeting_id)
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut status = self.status.lock();
        match *status {
            MeetingStatus::Recording | MeetingStatus::Paused => {
                info!("Stopping meeting manually...");
                *status = MeetingStatus::Idle;
                Ok(())
            }
            _ => Err("Meeting is not active".to_string())
        }
    }

    /// Import an existing audio file (mp3/m4a/wav/flac): decode it, run it
    /// through the same VAD → STT → diarization pipeline as a live meeting, and
    /// store the result as a new meeting. Processing happens on a background
    /// thread; the meeting id is returned immediately.
    pub fn import_audio_file(&self, app_handle: AppHandle, path: String, title: String) -> Result<String, String> {
        let samples = crate::audio::decode::decode_to_16k_mono(std::path::Path::new(&path))
            .map_err(|e| format!("Failed to decode audio: {}", e))?;
        if samples.is_empty() {
            return Err("No audio could be decoded from that file.".to_string());
        }
        let meeting_id = uuid::Uuid::new_v4().to_string();
        self.db.create_meeting(&meeting_id, &title, "import").map_err(|e| format!("Failed to create meeting: {:?}", e))?;
        self.spawn_transcription_job(app_handle, meeting_id.clone(), samples)?;
        Ok(meeting_id)
    }

    /// Re-run the STT + diarization pipeline over a meeting's *saved* recording,
    /// replacing its existing transcript. This is the recovery path for a meeting
    /// whose live transcription failed (e.g. the STT provider ran out of credits
    /// or the network dropped): the audio was still captured and saved, so the
    /// transcript can be regenerated after the fact without losing anything.
    pub fn reprocess_meeting(&self, app_handle: AppHandle, meeting_id: String) -> Result<(), String> {
        // Never reprocess the meeting that is currently recording.
        if self.active_meeting_id.lock().as_deref() == Some(meeting_id.as_str())
            && !matches!(*self.status.lock(), MeetingStatus::Idle)
        {
            return Err("This meeting is still recording — stop it first.".to_string());
        }
        let key = format!("audio_path::{}", meeting_id);
        let path = self.db.get_setting(&key).map_err(|e| e.to_string())?
            .filter(|p| !p.is_empty() && std::path::Path::new(p).exists())
            .ok_or_else(|| "No saved recording exists for this meeting.".to_string())?;
        let samples = crate::audio::decode::decode_to_16k_mono(std::path::Path::new(&path))
            .map_err(|e| format!("Failed to decode saved recording: {}", e))?;
        if samples.is_empty() {
            return Err("The saved recording is empty.".to_string());
        }
        // Replace the old (failed / partial) transcript before regenerating.
        self.db.clear_segments(&meeting_id).map_err(|e| format!("Failed to clear old transcript: {:?}", e))?;
        self.spawn_transcription_job(app_handle, meeting_id, samples)?;
        Ok(())
    }

    /// Run VAD → STT → diarization over `samples` on a background thread, writing
    /// segments into an existing `meeting_id`. Shared by import and reprocess.
    fn spawn_transcription_job(&self, app_handle: AppHandle, meeting_id: String, samples: Vec<f32>) -> Result<(), String> {
        let engine = load_stt_engine_from_settings(&self.db, &self.model_manager)?;

        let threshold_rms = self.db.get_setting("vad_threshold").unwrap_or(None).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.015);
        let speech_ms = self.db.get_setting("vad_speech_ms").unwrap_or(None).and_then(|s| s.parse::<u32>().ok()).unwrap_or(150);
        let hangover_ms = self.db.get_setting("vad_hangover_ms").unwrap_or(None).and_then(|s| s.parse::<u32>().ok()).unwrap_or(800);
        let diarization_threshold = self.db.get_setting("diarization_threshold").unwrap_or(None).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.20);

        let diarizer = Arc::new(DiarizationEngine::new());
        let clustering = Arc::new(Mutex::new(SpeakerClustering::new(self.db.clone(), diarization_threshold)));

        let db = self.db.clone();
        let models_dir = self.model_manager.models_dir().clone();
        let app = app_handle.clone();
        let mid = meeting_id.clone();

        std::thread::spawn(move || {
            const SR: f64 = 16000.0;
            const CHUNK: usize = 1600; // 100ms
            const PREROLL_SAMPLES: usize = 4800;

            let mut vad = Vad::new(threshold_rms, speech_ms, hangover_ms, &models_dir);
            let total = samples.len();

            // Transcribe + diarize + persist + emit one segment.
            let process = |buf: &[f32], start: f64, end: f64| {
                let text = match tauri::async_runtime::block_on(engine.transcribe(buf)) {
                    Ok(res) => res.text.trim().to_string(),
                    Err(e) => { error!("Import transcription error: {:?}", e); String::new() }
                };
                if text.is_empty() { return; }
                let embedding = diarizer.extract_embedding(buf, 16000.0);
                let (speaker_id, speaker_name) = clustering.lock().match_or_create_speaker(&embedding);
                let segment_id = uuid::Uuid::new_v4().to_string();
                if let Err(e) = db.add_segment(&segment_id, &mid, Some(&speaker_id), start, end, &text) {
                    error!("Failed to save imported segment: {:?}", e);
                }
                let _ = app.emit("meeting-transcript-update", serde_json::json!({
                    "meeting_id": mid,
                    "reload": true,
                    "segment": {
                        "id": segment_id, "meeting_id": mid, "speaker_id": speaker_id,
                        "speaker_name": speaker_name, "start_time": start, "end_time": end, "text": text
                    }
                }));
            };

            let mut in_speech = false;
            let mut preroll: VecDeque<f32> = VecDeque::new();
            let mut speech_buffer: Vec<f32> = Vec::new();
            let mut segment_start_time = 0.0f64;
            let mut pos = 0usize;

            while pos < total {
                let end = (pos + CHUNK).min(total);
                let chunk = &samples[pos..end];
                let current_time = pos as f64 / SR;
                pos = end;

                let (vad_change, _sp) = vad.process_samples(chunk);
                if in_speech {
                    speech_buffer.extend_from_slice(chunk);
                } else {
                    preroll.extend(chunk.iter().copied());
                    if preroll.len() > PREROLL_SAMPLES {
                        let d = preroll.len() - PREROLL_SAMPLES;
                        preroll.drain(0..d);
                    }
                }

                if let Some(started) = vad_change {
                    if started {
                        in_speech = true;
                        speech_buffer.clear();
                        speech_buffer.extend(preroll.iter().copied());
                        preroll.clear();
                        let buffered = speech_buffer.len() as f64 / SR;
                        segment_start_time = (current_time - buffered).max(0.0);
                    } else {
                        in_speech = false;
                        if !speech_buffer.is_empty() {
                            process(&speech_buffer, segment_start_time, pos as f64 / SR);
                            speech_buffer.clear();
                        }
                    }
                }
            }
            // Flush trailing speech if the file ended mid-utterance.
            if !speech_buffer.is_empty() {
                process(&speech_buffer, segment_start_time, total as f64 / SR);
            }

            // Neural diarization finalize pass over the whole imported file.
            #[cfg(feature = "neural-diarization")]
            {
                info!("Running neural diarization over imported audio ({} samples)...", samples.len());
                let _ = app.emit("meeting-diarizing", serde_json::json!({ "meeting_id": mid, "status": "running" }));
                if let Ok(mut nd) = crate::diarization::NeuralDiarizer::new() {
                    if let Ok(turns) = nd.diarize(&samples) {
                        use crate::diarization::NeuralDiarizer;
                        if let Ok(spans) = db.list_segment_spans(&mid) {
                            let mut ensured = std::collections::HashSet::new();
                            for (seg_id, start, end) in spans {
                                if let Some(label) = NeuralDiarizer::speaker_for_span(&turns, start, end) {
                                    let spk_id = format!("neural_{}_{}", mid, label);
                                    if ensured.insert(spk_id.clone()) {
                                        let display = label
                                            .rsplit('_')
                                            .next()
                                            .and_then(|n| n.parse::<u32>().ok())
                                            .map(|n| format!("Speaker {}", n + 1))
                                            .unwrap_or_else(|| label.clone());
                                        let _ = db.upsert_speaker(&spk_id, &display);
                                    }
                                    let _ = db.update_segment_speaker(&seg_id, &spk_id);
                                }
                            }
                        }
                        let payload = serde_json::json!({
                            "meeting_id": mid,
                            "turns": turns.iter().map(|t| serde_json::json!({
                                "start": t.start, "end": t.end, "speaker": t.speaker,
                            })).collect::<Vec<_>>(),
                        });
                        let _ = app.emit("meeting-diarization", payload);
                    }
                }
                let _ = app.emit("meeting-diarizing", serde_json::json!({ "meeting_id": mid, "status": "done" }));
            }

            let dur = (total as f64 / SR) as i32;
            let _ = db.update_meeting_duration(&mid, dur);
            let _ = app.emit("meeting-transcript-update", serde_json::json!({ "meeting_id": mid, "reload": true }));
            info!("Transcription job finished for meeting {} ({} samples).", mid, total);
        });

        Ok(())
    }

    pub fn pause(&self) -> Result<(), String> {
        let mut status = self.status.lock();
        if matches!(*status, MeetingStatus::Recording) {
            *status = MeetingStatus::Paused;
            let _ = self.db.set_setting("meeting_paused", "true");
            info!("Meeting paused.");
            Ok(())
        } else {
            Err("Meeting is not recording, cannot pause".to_string())
        }
    }

    pub fn resume(&self, app_handle: AppHandle) -> Result<(), String> {
        let mut status = self.status.lock();
        if matches!(*status, MeetingStatus::Paused) {
            *status = MeetingStatus::Recording;
            let _ = self.db.set_setting("meeting_paused", "false");
            let _ = app_handle.emit("meeting-status", MeetingStatus::Recording);
            info!("Meeting resumed.");
            Ok(())
        } else {
            Err("Meeting is not paused, cannot resume".to_string())
        }
    }
}

/// Recover audio left behind by an unclean shutdown. During recording we stream
/// raw f32 to `<meeting_id>.f32.part`; a clean stop deletes it. Any `.part` files
/// still present on launch mean the app died mid-meeting — convert each into a
/// real WAV so the recording is preserved and can be reprocessed. Also clears the
/// stale `active_meeting_id` setting: a process restart can never continue a live
/// recording, and leaving it set would make the UI show a phantom "recording".
///
/// Call once at startup, before the UI queries meeting state.
pub fn recover_interrupted_recordings(db: &Database, recordings_dir: &Path) {
    // A restart can't resume an in-flight recording — never restore it as active.
    if let Ok(Some(active)) = db.get_setting("active_meeting_id") {
        if !active.is_empty() {
            let _ = db.set_setting("active_meeting_id", "");
        }
    }

    let entries = match std::fs::read_dir(recordings_dir) {
        Ok(e) => e,
        Err(_) => return, // no recordings dir yet
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let fname = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.ends_with(".f32.part") => n.to_string(),
            _ => continue,
        };
        let meeting_id = fname.trim_end_matches(".f32.part").to_string();

        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => { error!("recover: failed to read {:?}: {:?}", path, e); continue; }
        };
        let n = bytes.len() / 4;
        if n == 0 {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let mut samples = Vec::with_capacity(n);
        for c in bytes.chunks_exact(4) {
            samples.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
        }

        let wav_path = recordings_dir.join(format!("{}.wav", meeting_id));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        match hound::WavWriter::create(&wav_path, spec) {
            Ok(mut w) => {
                for &s in &samples { let _ = w.write_sample(s); }
                if w.finalize().is_ok() {
                    let _ = db.set_setting(&format!("audio_path::{}", meeting_id), &wav_path.to_string_lossy());
                    let _ = db.update_meeting_duration(&meeting_id, (n as f64 / 16000.0) as i32);
                    let _ = std::fs::remove_file(&path);
                    warn!(
                        "Recovered {:.1}s of audio for meeting {} after an unclean shutdown — reprocess it to regenerate the transcript.",
                        n as f64 / 16000.0, meeting_id
                    );
                }
            }
            Err(e) => error!("recover: failed to write WAV {:?}: {:?}", wav_path, e),
        }
    }
}

/// Build an STT engine from the meeting settings (embedded Whisper or a remote
/// API provider). Standalone (no cache) — used by the import path.
fn load_stt_engine_from_settings(
    db: &Database,
    model_manager: &ModelManager,
) -> Result<Arc<dyn SttEngine>, String> {
    let model_id = db
        .get_setting("meeting_stt_model")
        .unwrap_or(None)
        .or_else(|| db.get_setting("dictation_stt_model").unwrap_or(None))
        .unwrap_or_else(|| "whisper-tiny-q5".to_string());
    let provider_id = db
        .get_setting("meeting_stt_provider")
        .unwrap_or(None)
        .or_else(|| db.get_setting("default_stt_provider").unwrap_or(None))
        .unwrap_or_else(|| "embedded".to_string());

    if provider_id == "embedded" {
        #[cfg(feature = "audio8")]
        if model_id.contains("audio8") {
            let dir = crate::stt::Audio8Engine::model_dir_default(model_manager.models_dir());
            let eng = crate::stt::Audio8Engine::new(&dir)
                .map_err(|e| format!("Failed to load Audio8 engine: {:?}", e))?;
            return Ok(Arc::new(eng));
        }
        #[cfg(feature = "parakeet")]
        if model_id.contains("parakeet") {
            let dir = crate::stt::ParakeetEngine::model_dir_default(model_manager.models_dir());
            let eng = crate::stt::ParakeetEngine::new(&dir)
                .map_err(|e| format!("Failed to load Parakeet engine: {:?}", e))?;
            return Ok(Arc::new(eng));
        }
        let model_path = model_manager
            .get_model_path(&model_id)
            .ok_or_else(|| format!("Model not downloaded: {}. Please download it first.", model_id))?;
        let eng = WhisperEngine::new(model_path)
            .map_err(|e| format!("Failed to load Whisper engine: {:?}", e))?
            .with_language(crate::stt::stt_language(db));
        Ok(Arc::new(eng))
    } else {
        let registry = crate::providers::ProviderRegistry::new(db.clone());
        let provider = registry
            .get_provider(&provider_id)?
            .ok_or_else(|| format!("Provider not found: {}", provider_id))?;
        let api_url = provider.api_url.unwrap_or_default();
        let api_key = provider.api_key;
        let model = if model_id.is_empty() { "whisper-1".to_string() } else { model_id };
        Ok(Arc::new(ApiSttEngine::new(api_url, api_key, model, provider.provider_type)
            .with_language(crate::stt::stt_language(db))))
    }
}
