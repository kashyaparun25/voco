# Voco Rust Backend Audit Report

**Scope:** All files under `/Volumes/Extreme SSD/voco/src-tauri/src/` compared to the implementation plan in `/Volumes/Extreme SSD/voco/implementation_plan.md`.

**Audit Date:** 2025-01-21

**Overall Build Status:** `cargo check` succeeds for both the library and binary targets with only warnings (no compilation errors). The code is structurally coherent and will run, but many subsystems are placeholders or simplified relative to the plan.

---

## Phase 1: Foundation (App Shell, Audio Capture, Database)

### P1.1 / P1.3 — Tauri 2 + Rust Project Scaffold

- **Files:** `main.rs`, `lib.rs`, `Cargo.toml`
- **Status:** Mostly real / functional
- **Notes:**
  - `main.rs` is a minimal entry point that calls `tauri_app_lib::run()`.
  - `lib.rs` wires the Tauri runtime, plugins, system tray, global shortcut, state management, and command registry.
  - `Cargo.toml` defines `tauri-app` with dependencies: `tauri` 2, `cpal`, `ringbuf`, `rubato`, `rusqlite`, `whisper-rs` with `metal`, `tokio`, `reqwest`, `async-trait`, etc.
  - **Gaps:** The global shortcut handler only logs the trigger; it does not start dictation or meeting services. The tray menu items also only log; they do not invoke the underlying services.

### P1.4 — Microphone Capture (`cpal`)

- **File:** `audio/mic_capture.rs`
- **Status:** Real / functional
- **Notes:**
  - Lists input devices and starts a `cpal` input stream on the default or named device.
  - Converts multi-channel input to mono f32 and pushes it into an `AudioMixer` ring buffer.
  - A processing thread resamples to 16 kHz via `rubato` and sends chunks through a `crossbeam` channel. It also optionally computes an RMS level.
  - Handles f32/i16/u16 sample formats.
  - **Gaps:** `unsafe impl Send/Sync` for `MicCapture` relies on manual reasoning; the raw `cpal::Stream` is not inherently Send/Sync.

### P1.5 — Audio Resampling (`rubato`)

- **File:** `audio/resampler.rs`
- **Status:** Real / functional
- **Notes:** Wraps `rubato::FftFixedInOut` to resample to 16 kHz mono f32, with a passthrough when already 16 kHz mono.

### P1.6 — Ring Buffer

- **File:** `audio/mixer.rs`
- **Status:** Real / functional
- **Notes:**
  - Wraps `ringbuf::HeapRb` with `parking_lot::Mutex` for producer/consumer access.
  - **Gaps:** It is named "AudioMixer" but does not actually mix multiple audio streams. It only carries one microphone channel.

### P1.7 — System Tray

- **File:** `lib.rs`
- **Status:** Real UI wiring, stub behavior
- **Notes:** Tray menu is built but all items except "Quit" only log; they do not invoke the underlying services.

### P1.9 — Global Hotkeys

- **File:** `lib.rs`
- **Status:** Real registration, stub handler
- **Notes:** Registers `CommandOrControl+Shift+Space` but the handler only logs the event.

### P1.10 — SQLite Database

- **Files:** `storage/database.rs`, `storage/migrations.rs`
- **Status:** Real / functional
- **Notes:**
  - `migrations.rs` creates `settings`, `meetings`, `speakers`, and `segments` tables with foreign keys and cascade rules.
  - `database.rs` provides helpers for settings, meetings, speakers, and segments via a `parking_lot` mutex wrapper.

---

## Phase 2: Dictation Mode

### P2.1 — Embedded Whisper (`whisper-rs` + Metal)

- **File:** `stt/whisper.rs`
- **Status:** Real / functional
- **Notes:**
  - Loads a `ggml` Whisper model via `whisper-rs`.
  - Runs full inference with greedy sampling and 4 threads.
  - Returns `TranscriptionResult` with segments and timestamps.
  - `Cargo.toml` enables the `metal` feature.
  - **Gaps:** Language hardcoded to `"en"`. No GPU-layer offloading control.

### P2.2 — Silero VAD

- **File:** `audio/vad.rs`
- **Status:** Stub / placeholder
- **Notes:**
  - The plan calls for Silero VAD (ONNX). The implementation is a simple energy/RMS-based `EnergyVad`.
  - No bundled Silero ONNX model and no `ort` dependency.

### P2.3 — `SttEngine` Trait + Whisper Implementation

- **Files:** `stt/engine.rs`, `stt/whisper.rs`
- **Status:** Real / functional
- **Notes:** Defines the trait and implements it for Whisper. Streaming method is a wrapper around full transcription.

### P2.4 — Dictation Service

- **File:** `services/dictation.rs`
- **Status:** Real / functional, with gaps
- **Notes:**
  - Orchestrates capture → VAD → STT → text injection.
  - Reads settings (model, mode, auto-paste, VAD thresholds, audio device).
  - Supports embedded Whisper and API STT engines.
  - Emits `dictation-status`, `dictation-level`, and `dictation-final` events.
  - **Gaps:**
    - Uses `tokio::runtime::Handle::current().block_on(...)` inside a spawned `std::thread`. Risky and not a clean async design.
    - No true streaming partial transcription: only final text is sent.
    - Engine cache is not effectively reused across runs.

### P2.5 / P2.6 — Pill Overlay / Waveform Visualization

- **Status:** Frontend responsibility; not in backend scope.

### P2.7 — Stream Audio Levels + Partial Text

- **Files:** `services/dictation.rs`, `audio/mic_capture.rs`
- **Status:** Real / functional (audio levels only)
- **Notes:** RMS levels are sent via `dictation-level`; partial text is not streamed.

### P2.8 — Text Injection via macOS Accessibility API

- **File:** `services/text_injector.rs`
- **Status:** Placeholder workaround
- **Notes:** Uses `pbcopy` + AppleScript `Command+V` paste, not the Accessibility API. Briefly pollutes the clipboard.

### P2.9 — Push-to-Talk, Toggle, Auto-Stop

- **File:** `services/dictation.rs`
- **Status:** Real / functional
- **Notes:** Toggle and Push-to-Talk work via start/stop commands; Auto-Stop triggers on VAD silence.

### P2.10 — Model Manager (Download Whisper GGUF Models)

- **File:** `stt/manager.rs`
- **Status:** Real / functional
- **Notes:** Hardcodes models, downloads from Hugging Face, tracks in-memory progress. **Gaps:** No SHA integrity, no resume, no frontend progress events.


---

## Phase 3: Meeting Mode + Diarization

### P3.1 — System Audio Capture via `ScreenCaptureKit` (`cidre`)

- **File:** `audio/system_capture.rs`
- **Status:** Stub / placeholder
- **Notes:**
  - The plan requires `ScreenCaptureKit` / `cidre`. The implementation looks for a virtual loopback device (BlackHole, Soundflower, etc.) using `cpal`.
  - Falls back to silent simulation if no loopback device is found.
  - **Gaps:** No ScreenCaptureKit or macOS system audio tap implementation.

### P3.2 — Audio Mixer (Mic + System Audio)

- **Files:** `audio/mixer.rs`, `services/meeting.rs`
- **Status:** Stub / not implemented as specified
- **Notes:**
  - The meeting service starts two separate captures: mic and system. They are processed independently and not mixed into a unified stream.
  - `AudioMixer` is only a single-channel ring buffer.

### P3.3 — Speaker Diarization (`speakrs`)

- **Files:** `diarization/engine.rs`, `diarization/clustering.rs`, `diarization/profiles.rs`
- **Status:** Stub / placeholder
- **Notes:**
  - The plan calls for `speakrs` (ECAPA-TDNN + segmentation). The implementation is a custom Goertzel frequency-bin clustering system.
  - `DiarizationEngine::extract_embedding` computes an 8-bin frequency embedding; `SpeakerClustering` matches using cosine distance.
  - Speaker profiles are stored in SQLite as f32 blobs.
  - **Gaps:** Not a neural diarization model. No `speakrs` dependency.

### P3.4 — Meeting Service

- **File:** `services/meeting.rs`
- **Status:** Real / functional, with significant gaps
- **Notes:**
  - Starts a meeting, creates a DB record, captures mic + system audio, runs VAD, transcribes speech chunks, runs diarization, stores segments, and emits `meeting-transcript-update` events.
  - Supports pause/resume.
  - **Gaps:**
    - Same `block_on` pattern inside a `std::thread` as dictation.
    - No actual audio mixing.
    - System audio is loopback or silent simulation.
    - No audio recording file is saved.
    - Pause is only a status flag; capture threads continue running.

### P3.5 — P3.11

- **Status:** UI/frontend features, not in backend scope.


---

## Phase 4: Provider System + STT Options

### P4.1 — Provider Registry (CRUD, Health Checks, Configuration)

- **Files:** `providers/registry.rs`, `providers/config.rs`
- **Status:** Real / functional, with gaps
- **Notes:**
  - `ProviderRegistry` stores providers as a JSON blob in the `settings` table.
  - Supports list, get, add/update, delete, set active, and get active.
  - Defaults include OpenAI, Groq, NVIDIA, Ollama, and LM Studio.
  - **Gaps:**
    - No separate `update_provider` command in `lib.rs` (registry's `add_provider` overwrites if ID exists).
    - No `list_provider_models` command.
    - No capability flags (STT/LLM/Diarization) as described in the plan.

### P4.2 — `ApiSttEngine` (OpenAI, NVIDIA NIM, Groq Compatible)

- **File:** `stt/api.rs`
- **Status:** Real / functional
- **Notes:** Converts f32 PCM to WAV and POSTs to `/audio/transcriptions`. Supports `verbose_json` and bearer auth. Parses word timestamps when available.

### P4.3 — `ort` Integration for Parakeet TDT 0.6B

- **Status:** Missing entirely
- **Notes:** No `ort` dependency, no ONNX runtime code, no Parakeet model support.

### P4.4 — distil-whisper-large-v3 / Whisper Large V3 Turbo

- **File:** `stt/manager.rs` (model list), `stt/whisper.rs` (engine)
- **Status:** Partially present in name only
- **Notes:** Models are listed and downloadable, but all run through the same `WhisperEngine` with no special handling.

### P4.5 — P4.6

- **Status:** UI/frontend features, not in backend scope.

### P4.7 — API Key Management (Encrypted Storage)

- **File:** `providers/config.rs`
- **Status:** Real but insecure
- **Notes:** XOR-based encryption with a hardcoded key and hex encoding. Not cryptographically secure. The plan's `tauri-plugin-store` is not used.

### P4.8 — Auto-Detect Ollama / LM Studio

- **File:** `providers/health.rs`
- **Status:** Real / functional
- **Notes:** Polls `localhost:11434/api/tags` and `localhost:1234/v1/models` every 10 seconds and emits `local-servers-status` events.

### P4.9 — Provider Health Check Polling

- **File:** `providers/health.rs`
- **Status:** Real / functional
- **Notes:** `check_provider_health` checks `/v1/models` (or `/api/tags` for Ollama) and reports reachability, authentication, and latency.


---

## Phase 5: LLM Integration + Summaries

### P5.1 — `llama-cpp-rs` for Embedded GGUF LLM Inference

- **Status:** Missing entirely
- **Notes:** No `llama-cpp-rs` dependency. No in-process GGUF loading or inference.

### P5.2 — `LlmEngine` Trait + Embedded + API Implementations

- **Files:** `llm/mod.rs`, `llm/local_runner.rs`, `llm/client.rs`
- **Status:** Partially real / partially stub
- **Notes:**
  - `LlmEngine` trait exists.
  - `ApiClient` is a real OpenAI-compatible chat completions client.
  - `LocalRunner` tries LM Studio, then Ollama, then falls back to a hardcoded simulated summary. It is not a true embedded runner.

### P5.3 — Meeting Summary Prompt Templates

- **File:** `llm/prompt.rs`
- **Status:** Real / functional
- **Notes:** Formats transcript segments, reads `summary_length` / `summary_style` settings, and generates a structured Markdown prompt.

### P5.4 — Summary UI

- **Status:** Frontend responsibility; not in backend scope.

### P5.5 — `ApiLlmEngine` (OpenAI-Compatible, SSE Streaming)

- **File:** `llm/client.rs`
- **Status:** Partially real
- **Notes:** Non-streaming chat completion client. The plan's SSE streaming is not implemented.

### P5.6 — LLM Model Download Manager

- **File:** `stt/manager.rs`
- **Status:** Partially real
- **Notes:** Includes a Qwen GGUF entry and can download it, but no local engine can load or run it.

### P5.7 — RAM-Based Model Recommendations

- **Status:** Missing entirely
- **Notes:** No system RAM detection or tier suggestion logic.

### P5.8 — Regenerate Summary

- **File:** `commands/llm.rs`
- **Status:** Real / functional
- **Notes:** `regenerate_summary` re-invokes `summarize_meeting`.

### P5.9 — Summary Export

- **Status:** Missing entirely
- **Notes:** No export to TXT, SRT, JSON, Markdown, or combined formats.

---

## Phase 6 / Phase 7

- **Status:** Phase 6 and 7 items are mostly UI, themes, export, onboarding, updater, and advanced features. Backend pieces are largely missing (audio playback, export, custom vocabulary, file import transcription, updater, universal binary, performance profiling).


---

## Compilation and Import Issues

- **Build Status:** `cargo check --lib` and `cargo check --bin tauri-app` pass successfully. No errors.
- **Warnings:** Only minor warnings (unused imports, unused variables, dead code). Notable warnings:
  - `audio/system_capture.rs`: `l_sender` unused when computing system loopback RMS.
  - `services/dictation.rs`: `PathBuf` import, `Sender`/`Receiver` imports, and `self_clone` unused.
  - `services/meeting.rs`: `warn` import and `Sender`/`Receiver` imports unused.
  - `diarization/profiles.rs`: `anyhow` import unused.
  - `stt/manager.rs`: `Path` import unused.
  - `stt/api.rs`: `OpenAISegment.id` field never read.

None of these warnings are blockers.

---

## Missing Files / Modules Required by the Plan

| Plan Requirement | Status | Notes |
|-------------------|--------|-------|
| `ScreenCaptureKit` / `cidre` system audio capture | Missing | `audio/system_capture.rs` uses CPAL loopback fallback instead. |
| Real audio mixer (mic + system → unified stream) | Missing | `AudioMixer` is a single-channel buffer. |
| Silero VAD (ONNX) | Missing | `EnergyVad` is a placeholder. |
| `speakrs` / ECAPA-TDNN diarization | Missing | Custom Goertzel clustering is a placeholder. |
| `ort` / Parakeet TDT 0.6B ONNX | Missing | No ONNX runtime or Parakeet code. |
| `llama-cpp-rs` embedded LLM | Missing | `LocalRunner` only calls local APIs. |
| macOS Accessibility API text injection | Missing | `text_injector.rs` uses clipboard paste. |
| `tauri-plugin-store` for API keys | Missing | Keys stored in SQLite with XOR. |
| Export engine (TXT, SRT, JSON, Markdown) | Missing | No export module. |
| Audio playback of recorded meetings | Missing | No playback module. |
| Audio file import/transcription | Missing | No import pipeline. |
| Custom vocabulary / personal dictionary | Missing | No dictionary support. |
| App updater (`tauri-plugin-updater`) | Missing | No updater module. |
| `update_provider` command | Missing | Only `add_provider` exists, which overwrites. |
| `list_provider_models` command | Missing | Not implemented. |
| RAM-based tier recommendation | Missing | Not implemented. |
| Streaming LLM summary (SSE) | Missing | `ApiClient` is non-streaming. |
| Model download progress events | Missing | Progress is in memory only. |
| Audio file saving (`save_audio`) | Missing | Meeting audio is not recorded to disk. |


---

## Summary

### What Is Working

- The project compiles and runs (`cargo check` passes for lib and bin).
- Tauri 2 runtime, system tray, global shortcut registration, and command routing are wired.
- SQLite schema for settings, meetings, speakers, and segments is in place.
- Microphone capture (`cpal`) with resampling to 16 kHz and ring buffering works.
- The `SttEngine` trait and Whisper.cpp implementation (with Metal) work.
- API STT engine for OpenAI-compatible endpoints works.
- Provider registry with add/delete/list/set-active/test works.
- Provider health checks and local server (Ollama/LM Studio) detection work.
- Dictation service orchestrates capture → VAD → STT → text injection and supports Toggle, Push-to-Talk, and Auto-Stop.
- Meeting service orchestrates capture, VAD, STT, diarization clustering, and segment storage.
- LLM summary command retrieves transcripts, formats a prompt, and calls an LLM engine.
- API LLM client works for OpenAI-compatible chat completions.
- Model manager can download listed Whisper/Qwen models from Hugging Face.

### What Is Stubbed or Placeholder

- **VAD:** `EnergyVad` is a simple energy detector; Silero VAD is not implemented.
- **System audio capture:** Uses CPAL loopback detection or silent simulation; no `ScreenCaptureKit`.
- **Audio mixing:** The mixer is a single-channel buffer; mic and system streams are not combined.
- **Diarization:** Custom Goertzel clustering is a placeholder; `speakrs`/ECAPA-TDNN is not used.
- **Text injection:** Clipboard paste via AppleScript is a workaround, not the Accessibility API.
- **Embedded LLM:** No `llama-cpp-rs`; the "embedded" provider falls back to LM Studio/Ollama APIs or a simulated summary.
- **Parakeet ONNX:** Not implemented.
- **API key storage:** XOR-hex in SQLite, not secure storage or `tauri-plugin-store`.
- **Hotkey/tray actions:** Registered and rendered, but the actual service invocation is not wired in the handlers.
- **Meeting pause:** Status flag only; capture threads keep running.
- **Streaming summary / partial transcription:** Not implemented as streams.
- **Model download progress:** Tracked in memory but not emitted to the frontend.
- **Export, audio playback, import, dictionary, updater:** Missing entirely.

### What Is Entirely Missing

- `ScreenCaptureKit` / `cidre` system audio capture.
- `ort` + Parakeet TDT 0.6B ONNX STT.
- `llama-cpp-rs` in-process GGUF inference.
- `speakrs` / ECAPA-TDNN neural diarization.
- Bundled Silero VAD model.
- True audio mixer and mic+system audio recording file.
- macOS Accessibility API text injection.
- `tauri-plugin-store` integration.
- Export engine (TXT, SRT, JSON, Markdown).
- Audio file import and transcription.
- Custom vocabulary / personal dictionary.
- `tauri-plugin-updater` and Homebrew distribution.
- RAM-based model recommendations.
- Streaming LLM summary tokens.
- Several planned commands (`update_provider`, `list_provider_models`, etc.).

### Bottom Line

The Rust backend has a solid foundation: it compiles, the database and audio capture pipeline work, and the dictation and meeting services run end-to-end (capture → STT → storage). However, many of the plan's advanced or macOS-specific components are either stubs, simplified workarounds, or entirely missing. The most significant gaps are the lack of true `ScreenCaptureKit` system audio capture, the absence of neural VAD and diarization models, the missing `llama-cpp-rs` embedded LLM, and the incomplete hotkey/tray wiring. The codebase is best described as a functional Phase 1–2 core with Phase 3–5 partially implemented using placeholder algorithms rather than the planned ML models.

