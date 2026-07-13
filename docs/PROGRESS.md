# Voco — Implementation Progress Tracker

> **Purpose:** Single source of truth for Voco's implementation status.
> Tracks every phase from the original plan, what was built before the resume session,
> what the resume session changed, and what remains.
> **Any future session can read this file to pick up exactly where we left off.**
>
> **Last updated:** 2026-07-13 (resume session #3 — see "Resume session #3" at the bottom)

---

## Quick Status Dashboard

| Phase | Name | Pre-Resume | Now | Notes |
|-------|------|------------|-----|-------|
| **P1** | Foundation | ✅ Mostly done | ✅ Mostly done | Shell, audio, DB, tray, shortcut registration done |
| **P2** | Dictation Mode | ⚠️ Backend core only | ⚠️ Backend core only | Whisper works; pill lifecycle NOT wired; text injection is AppleScript fallback |
| **P3** | Meeting + Diarization | ⚠️ Partial | ⚠️ Partial | Meeting service + transcript UI exist; system audio NOT ScreenCaptureKit; diarization placeholder |
| **P4** | Provider System | ✅ Mostly done | ✅ Mostly done | Provider CRUD, health checks, Ollama/LM Studio detection, API STT wired |
| **P5** | LLM Summaries | 🟡 Started | 🟡 In progress | summarize command + API client exist; embedded LLM NOT building; streaming stubbed |
| **P6** | Polish/Themes/Export | ❌ Not done | ❌ Not done | Only 2 themes, no vibrancy, no export, no onboarding |
| **P7** | Advanced | ❌ Not done | ❌ Not done | Updater, Homebrew, universal binary not started |

**Current blocker:** None. Default build + full `--features ml-all` build both pass. The original `llama_cpp_rs` blocker is resolved by swapping to `llama-cpp-2` (builds with Metal). All four ML backends now compile and are wired into the app behind opt-in features (default build stays lean/OFF).

---

## Build Status

| Command | Status | Notes |
|---------|--------|-------|
| `pnpm build` | ✅ Passes | tsc + vite; produces `dist/` |
| `cargo check` | ✅ Passes | 1 benign dead-code warning (intentional JSON field) |
| `cargo build` (debug) | ✅ Links | full binary compiles + links |
| `cargo test --lib` | ✅ 6/6 pass | export formatters (SRT/TXT/MD/JSON) |
| `tauri build` (release) | ✅ Bundles | `Voco.app` (20 MB) + `Voco_0.1.0_aarch64.dmg` |
| App launch smoke test | ✅ Runs | boots, initializes DB/registry/tray/shortcut/windows, stays alive; no crash |
| `cargo check --features ml-all` | ✅ Passes | all 4 ML backends compile together (1 benign warning) |
| `cargo build --release --features ml-all` | ✅ Links | real binary with llama.cpp/Metal + ONNX Runtime + speakrs |

## ML Features (session #2c) — now build & wired

All four previously-gated ML backends compile and are integrated behind opt-in Cargo features
(default build unchanged/lean). Enable individually or all at once with `--features ml-all`.

| Feature flag | Backend | Crate | Wired into |
|---|---|---|---|
| `silero-vad` | Silero neural VAD (ONNX) | `ort` 2.0-rc.12 | `audio/silero_vad.rs` → `audio::Vad` selector used by dictation + meeting (auto-picks Silero when model present, else energy VAD) |
| `parakeet` | Parakeet TDT 0.6B STT (ONNX) | `ort` + `ndarray` | `stt/parakeet.rs` (`ParakeetEngine: SttEngine`); dictation/meeting select it when the model id contains "parakeet" |
| `neural-diarization` | pyannote diarization (CoreML) | `speakrs` 0.1 (coreml) | `diarization/neural.rs` (`NeuralDiarizer`); meeting service accumulates audio and runs a batch finalize pass → emits `meeting-diarization` speaker turns |
| `embedded-llm` | In-app GGUF LLM (Metal) | `llama-cpp-2` 0.1 (metal) | `llm/embedded.rs` (`EmbeddedLlm`); `LocalRunner` prefers it when `embedded_llm_path` setting points to a downloaded GGUF |

**Key fix:** the original blocker (`llama_cpp_rs` 0.3 — build script mis-invokes `ar` on missing
ggml objects) was resolved by replacing it with the maintained `llama-cpp-2` crate, which builds
llama.cpp cleanly (with the `metal` feature). `ort` ships prebuilt ONNX Runtime binaries (no local
C++ build). `speakrs` builds with its `coreml` feature for Apple-Neural-Engine acceleration.

**Runtime note:** these compile and are wired, but require model files at runtime (Silero .onnx,
Parakeet ONNX dir, pyannote via HF download, a GGUF for the LLM). They degrade gracefully to the
built-in fallbacks (energy VAD, Whisper STT, Goertzel clustering, local-API/simulated LLM) when
models are absent.

## Configurable hotkey + custom-URL models (session #2f)

- **Configurable dictation hotkey, applied live (no restart).** New `services/hotkey.rs`:
  a persistent Core Graphics **event-tap monitor** supports *bare modifier keys* (Left/Right Option,
  Fn, Control, …) — which a normal global shortcut can't represent — plus standard combos via the
  global-shortcut plugin. `set_dictation_hotkey` command persists + applies instantly. Default is
  **Left Option (⌥)** as requested; ⌘⇧Space stays registered as a fallback. `core-graphics` +
  `core-foundation` are now always-on macOS deps. Bare-modifier keys need macOS *Input Monitoring*
  permission (tray + ⌘⇧Space work regardless). Frontend `HotkeySettings` shows preset chips.
- **Custom model download URL.** `ModelManager` now has a custom-model registry (`register_custom_model`
  + resolver used by list/download/path/delete). `add_custom_model(name, url, category)` command
  derives the filename from any http(s) URL (GGUF/ggml/ONNX), downloads with progress, and persists
  to the `custom_models` DB setting (re-registered at startup). Frontend `CustomModelAdder` in
  Settings → Models. Custom models are selectable as STT/LLM everywhere the built-ins are.

## Remaining-features completion (session #2e)

Finished the outstanding Phase 6/7 gaps. All builds green: default, `--features ml-all`,
`--features macos-native`, frontend `tsc` + `vite`, 6/6 unit tests.

**Frontend (agent, owns `src/`):** streaming summary (live tokens via `summarize_meeting_streaming`
+ `summary-token`/`summary-done`), real model-download progress (`model-download-progress`),
cross-meeting transcript search (`GlobalSearch`), audio playback (`AudioPlayer` via `convertFileSrc`),
**custom theme builder** (9th theme + live color pickers), RAM-based model recommendations banner,
first-run onboarding + mic-permission prompt, diarization/reload event consumption, micro-animations.

**Native macOS (agent, `macos-native` feature):** **real CGEvent-based Cmd+V text injection**
(replaces AppleScript when built with the feature; AppleScript remains the default fallback).
ScreenCaptureKit system audio: bindings compile but the objc2-0.5-generation `objc2-screen-capture-kit`
lacks the audio-callback surface (real SCK audio needs objc2 0.6), so it keeps the working
cpal-loopback fallback. Honest limitation, documented in `audio/system_capture.rs`.

**Backend (this session):**
- Frosted-glass **vibrancy** applied to the pill (Hud) + main (Sidebar) windows via `window-vibrancy`.
- **Meeting audio recording**: when `save_audio` is on, the mixed 16kHz mono stream is written to
  `<app_data>/recordings/<id>.wav` (hound) and its path stored; `get_meeting_audio_path` serves it.
- **Transcript search**: `search_segments` DB query + `search_transcripts` command (LIKE across all meetings).
- **Downloadable ML models**: added Silero VAD (~2MB), Qwen 0.5B GGUF (nano LLM) to the model manager.
- **Embedded LLM auto-discovery**: `get_llm_engine` finds any downloaded `*.gguf` in the models dir.
- **Asset protocol** enabled + scoped for audio playback (`protocol-asset` tauri feature).
- New commands registered: `search_transcripts`, `get_meeting_audio_path`.

**Genuinely deferred (with rationale):**
- **Real ScreenCaptureKit audio** — blocked by objc2 0.5 pin; fallback = cpal loopback (BlackHole). Needs an objc2 0.6 migration.
- **App auto-updater / Homebrew cask** — require a release server + signing keys + published cask; can't be built/verified without that infra.
- **Universal binary** — one build flag: `tauri build --target universal-apple-darwin` (arm64+x86_64); current bundles are arm64.

## Runtime verification with REAL audio (session #2d)

Proved the core pipelines actually work — not just compile — using speech synthesized by macOS
`say` and a real Whisper model. Integration tests live in `src-tauri/tests/pipeline_integration.rs`
(`#[ignore]`d; run with `cargo test --test pipeline_integration --features neural-diarization -- --ignored`).

| Pipeline | Result |
|---|---|
| **Dictation STT** (Whisper tiny, real speech) | ✅ Transcribed *"…test of the Voco dictation system. Please transcribe this sentence accurately."* near-perfectly |
| **VAD** (energy) on real speech | ✅ detected ~97% as speech |
| **Meeting STT** | ✅ same `WhisperEngine.transcribe` path as dictation |
| **Neural diarization / speaker ID** (`neural-diarization`, pyannote) | ✅ correctly separated 2 speakers with accurate turn boundaries on a 25 s clip — CPU **and** CoreML (CoreML ~2× faster, ANE confirmed) |
| **Meeting speaker relabeling** | ✅ finalize pass relabels stored segments via `speaker_for_span` → correct speakers surface in the transcript UI |

**Two findings baked into the code/docs:**
1. The **default Goertzel clustering does NOT reliably separate speakers** (merged two distinct
   synthetic voices). Real speaker identification requires the `neural-diarization` feature — which
   works. This is why the meeting service runs the neural finalize pass when that feature is built.
2. **Neural diarization needs ≥ ~10–15 s of audio** (pyannote sliding window); a short 8.6 s clip
   returned 0 turns. Not a bug — a windowing characteristic. `VOCO_DIARIZER_MODE=cpu|coreml|coreml-fast`
   overrides the execution mode.

**Runtime fix (session #2b):** the release smoke test caught a crash-on-launch panic
(`there is no reactor running` at `providers/health.rs`) — `tokio::spawn`/`Handle::current().block_on`
were called from contexts with no ambient Tokio reactor (the Tauri `setup` hook and plain OS worker
threads). Fixed all 5 sites to use Tauri's managed runtime: `tauri::async_runtime::spawn` (health poll,
model download) and `tauri::async_runtime::block_on` (dictation + meeting transcription on worker
threads); clipboard-restore in `text_injector` moved to a plain `std::thread`. App now launches clean.

---

## Detailed Phase Tracking

### Phase 1: Foundation — ✅ Mostly Complete

| ID | Task | Status | Evidence |
|----|------|--------|----------|
| P1.1 | Scaffold Tauri 2 + React + Vite + TS | ✅ | package.json, vite.config.ts, tsconfig.json |
| P1.2 | Install Astryx, CSS imports | ⚠️ Installed, misused | Components use inline styles, not Astryx tokens |
| P1.3 | Rust project structure | ✅ | commands/, audio/, stt/, llm/, providers/, diarization/, storage/, services/ |
| P1.4 | Microphone capture (cpal) | ✅ | audio/mic_capture.rs — real, handles f32/i16/u16 |
| P1.5 | Audio resampling (rubato) | ✅ | audio/resampler.rs — FftFixedInOut → 16kHz mono f32 |
| P1.6 | Ring buffer + channels | ✅ | audio/mixer.rs (ringbuf), crossbeam channels |
| P1.7 | System tray with menu | ⚠️ UI done, actions stub | lib.rs — tray built; items only log |
| P1.8 | Main window AppShell + SideNav | ✅ | src/windows/MainWindow.tsx |
| P1.9 | Register global hotkeys | ⚠️ Registered, handler stub | lib.rs — ⌘+Shift+Space; handler only logs |
| P1.10 | SQLite database + migrations | ✅ | storage/ — settings, meetings, speakers, segments |
| P1.11 | Base theme system | ⚠️ Partial | Only 2 custom themes, not planned 8 |

### Phase 2: Dictation Mode — ⚠️ Backend Core Only

| ID | Task | Status | Evidence |
|----|------|--------|----------|
| P2.1 | whisper-rs with Metal | ✅ | stt/whisper.rs — loads ggml, greedy, 4 threads |
| P2.2 | Silero VAD | ❌ Stub | audio/vad.rs — EnergyVad (energy detector), NOT Silero |
| P2.3 | SttEngine trait + Whisper | ✅ | stt/engine.rs, stt/whisper.rs |
| P2.4 | Dictation service | ⚠️ Partial | services/dictation.rs — capture→VAD→STT→inject; Toggle/PTT/AutoStop |
| P2.5 | Pill overlay window | ⚠️ UI exists, no lifecycle | src/windows/PillWindow.tsx — not managed from backend |
| P2.6 | Waveform visualization | ✅ Component exists | WaveformCanvas.tsx — NOT used in dictation tab |
| P2.7 | Stream audio levels + partial text | ⚠️ Partial | RMS via dictation-level event; partial text NOT streamed |
| P2.8 | Text injection (Accessibility API) | ❌ Fallback | text_injector.rs — clipboard paste via AppleScript |
| P2.9 | Push-to-talk, toggle, auto-stop | ✅ Backend | services/dictation.rs — all 3 modes |
| P2.10 | Model download from HuggingFace | ✅ | stt/manager.rs — downloads Whisper/Qwen |
| P2.11 | Model selector UI | ⚠️ Real + simulated | ModelSelector.tsx — real invoke + simulated progress |

### Phase 3: Meeting + Diarization — ⚠️ Partial

| ID | Task | Status | Evidence |
|----|------|--------|----------|
| P3.1 | System audio via ScreenCaptureKit | ❌ Not SCK | system_capture.rs — cpal loopback or silent simulation |
| P3.2 | Audio mixer (mic + system) | ❌ Single channel | audio/mixer.rs — carries mic only, no mixing |
| P3.3 | speakrs diarization | ❌ Placeholder | diarization/ — custom Goertzel clustering, NOT speakrs |
| P3.4 | Meeting service | ✅ | services/meeting.rs — capture→STT→diarize→store |
| P3.5 | Transcript view UI | ✅ | src/components/transcript/TranscriptView.tsx |
| P3.6 | Meeting controls | ✅ | src/components/meeting/MeetingControls.tsx |
| P3.7 | Real-time transcript streaming | ⚠️ Polling | Frontend polls, not true streaming |
| P3.8 | Speaker timeline | ✅ | src/components/meeting/SpeakerTimeline.tsx |
| P3.9 | Speaker renaming | ✅ | rename_speaker command + UI |
| P3.10 | Meeting history list | ✅ | src/components/meeting/MeetingList.tsx |
| P3.11 | Screen Recording onboarding | ✅ | ScreenRecordingOnboarding.tsx |

### Phase 4: Provider System — ✅ Mostly Done

| ID | Task | Status | Evidence |
|----|------|--------|----------|
| P4.1 | Provider registry (CRUD, health) | ✅ | providers/registry.rs, config.rs, health.rs |
| P4.2 | ApiSttEngine (OpenAI/NVIDIA/Groq) | ✅ | stt/api.rs |
| P4.3 | ort for Parakeet TDT 0.6B | ❌ Not impl | ort dep added, no ONNX code yet |
| P4.4 | distil-whisper + Turbo | ⚠️ Listed | Model manager lists them; no special handling |
| P4.5 | Provider settings UI | ✅ | src/components/providers/ |
| P4.6 | Model selector grouped by provider | ⚠️ Partial | Exists but not fully grouped |
| P4.7 | API key encrypted storage | ❌ XOR-hex | SQLite XOR, NOT tauri-plugin-store |
| P4.8 | Auto-detect Ollama/LM Studio | ✅ | providers/health.rs — start_local_server_detection |
| P4.9 | Health check polling | ✅ | Periodic polling implemented |

### Phase 5: LLM Integration + Summaries — 🟡 In Progress

| ID | Task | Status | Evidence |
|----|------|--------|----------|
| P5.1 | llama-cpp-rs embedded GGUF | ❌ Blocked | llama_cpp_rs dep added, native build fails |
| P5.2 | LlmEngine trait + impls | ⚠️ Partial | llm/mod.rs — trait + API client done; embedded falls back |
| P5.3 | Summary prompt templates | ✅ | llm/prompt.rs |
| P5.4 | Summary UI (streaming) | ⚠️ Partial | SummaryView.tsx exists; streaming NOT impl |
| P5.5 | ApiLlmEngine (SSE) | ⚠️ Partial | llm/client.rs — works, no SSE streaming |
| P5.6 | LLM model download | ⚠️ Partial | Manager downloads Qwen; no LLM-specific UI |
| P5.7 | RAM-based recommendations | ❌ | No sysctl RAM detection |
| P5.8 | Regenerate summary | ❌ | Command registered, not wired |
| P5.9 | Summary export | ❌ | No export engine |

### Phase 6: Polish, Themes & Export — ❌ Not Done

| ID | Task | Status |
|----|------|--------|
| P6.1 | All 8 curated themes | ❌ Only 2 (midnight, daylight) |
| P6.2 | Theme picker UI | ❌ |
| P6.3 | Custom theme builder | ❌ |
| P6.4 | Complete settings panel | ⚠️ Partial (providers/models, missing general/dictation/meeting) |
| P6.5 | Transcript export (TXT/SRT/JSON/MD) | ❌ |
| P6.6 | Frosted glass (window-vibrancy) | ❌ Dep added, not used |
| P6.7 | Micro-animations | ❌ |
| P6.8 | Audio playback | ❌ |
| P6.9 | Transcript search | ❌ |
| P6.10 | Keyboard shortcuts | ❌ |
| P6.11 | Error handling, crash recovery | ⚠️ Basic |
| P6.12 | First-run onboarding | ⚠️ Screen recording only |

### Phase 7: Advanced Features — ❌ Not Done
All tasks P7.1–P7.9 not started.

---

## Resume Session #1 — Changes Made (2026-07-11)

### What was done
1. **Full audit** via 3 sub-agents (Rust backend, frontend, build/config).
   - Reports: `audit_backend_rust.md`, `audit_frontend.md`, `audit_build_config.md`
2. **`src-tauri/Cargo.toml`** — added missing dependencies:
   - `macos-private-api` feature on `tauri` ✅
   - Tauri plugins: store, notification, positioner, window-state, dialog, fs, clipboard-manager ✅
   - Crates: hound, ort (2.0.0-rc.12), llama_cpp_rs (0.3), speakrs, reqwest-eventsource, window-vibrancy, objc2 ✅
3. **Build verification:**
   - `pnpm build` ✅ still passes
   - `cargo check` ❌ FAILS — `llama_cpp_rs` native C++ build fails

### Current blocker (IN PROGRESS)
- `llama_cpp_rs` v0.3.0: compiles llama.cpp C++ but `ggml.o`/`ggml-metal.o` not found by `ar`.
- **Resolution:** Gate `llama_cpp_rs`, `speakrs`, `ort` behind optional Cargo features so core builds. Tackle embedded LLM separately.

### Files modified this session
- `src-tauri/Cargo.toml` (added dependencies)
- `audit_backend_rust.md` (created — full Rust audit)
- `audit_frontend.md` (created — full frontend audit)
- `audit_build_config.md` (created — full build/config audit)
- `PROGRESS.md` (this file — created)

### Files NOT yet modified (planned)
- `src-tauri/tauri.conf.json` — macOSPrivateApi, pill window, productName "Voco", Info.plist
- `src-tauri/capabilities/default.json` — plugin permissions + window management
- `src-tauri/Info.plist` — LSUIElement, mic/screen usage descriptions (TO CREATE)
- `package.json` — @tauri-apps/plugin-* frontend packages
- `src-tauri/src/lib.rs` — plugin init, real shortcut/tray handlers
- `vite.config.ts` — envPrefix

---

## Resume Session #2 — Changes Made (2026-07-11)

Config baseline from session #1's plan was found already applied (tauri.conf.json Voco branding + macOSPrivateApi + pill window + macOS bundle; capabilities/default.json with all plugin perms; package.json JS plugins installed; Info.plist created; lib.rs plugins initialized). Built on top of that with **3 parallel sub-agents** (disjoint file ownership) + integration.

### Backend (Rust) — all in `src-tauri/`
- **Tray + global-shortcut handlers wired** (`lib.rs`): shortcut `⌘+Shift+Space` emits `trigger-dictation-toggle` + shows pill; tray items emit `navigate("meetings"/"settings")` and show/focus main window. Non-blocking (event-based).
- **Pill window lifecycle**: new `commands/window.rs` — `show_pill_window` / `hide_pill_window` (bottom-center positioning, graceful if window absent).
- **Provider commands**: `update_provider` (edits existing, errors if missing), `list_provider_models` (queries endpoint; empty list on failure).
- **Export engine**: new `services/export.rs` + `commands/export.rs` — `export_meeting(meetingId, format)` and `export_meeting_to_path(...)` for TXT / SRT / JSON / Markdown.
- **RAM recommendations**: new `commands/system.rs` — `get_system_ram_mb`, `recommend_models` (tiered by `sysctl hw.memsize`).
- **Model download progress events**: `stt/manager.rs` streams + emits `model-download-progress`.
- **LLM SSE streaming**: `llm/client.rs` `generate_stream` + `summarize_meeting_streaming` command → emits `summary-token` / `summary-done`. Non-streaming `summarize_meeting` unchanged.
- Warning cleanup; added `futures-util` dep.

### Frontend (React/TS) — `src/`
- **8-theme system**: added `aurora, sunset, ocean, monochrome, rose, neon` CSS (identical variable contract to midnight/daylight, dual-tone gradients for aurora/sunset), `lib/themes.ts` registry, `hooks/useTheme.ts`, `components/settings/ThemeSettings.tsx` picker.
- **Settings panels**: `GeneralSettings`, `DictationSettings`, `MeetingSettings`, `HotkeySettings` (load/persist via `get_setting`/`set_setting`).
- **Common**: `components/common/Toast.tsx` + `hooks/useToast.ts` (module pub/sub, standalone `showToast`), `components/common/StatusIndicator.tsx`.
- **Dictation hook**: `hooks/useDictation.ts` (real `start/stop`, throttled audio level, subscribes to `dictation-status`/`dictation-audio-level`/`dictation-final`/`trigger-dictation-toggle`).

### Integration (this session, by hand)
- `App.tsx`: switched to multi-theme `useTheme()`; mounts `<Toast/>`.
- `MainWindow.tsx`: **removed mock dictation** (now real `useDictation` + pill show/hide + live waveform) and **removed the 7-second mock meeting dialogue** (transcript now comes only from the real backend meeting service); rebuilt Settings tab to mount all panels + theme picker + provider list; added transcript **ExportMenu** (TXT/SRT/JSON/MD via export command + dialog/fs); wired `navigate` events; pause/resume now call backend.
- `PillWindow.tsx`: subscribes to the real backend events (`dictation-audio-level`, `dictation-final`, `dictation-status`) and hides itself on stop.

### Still gated / not done (require failing native builds or deep macOS work)
- Embedded LLM (`llama_cpp_rs`), Parakeet ONNX (`ort`), neural diarization (`speakrs`), Silero VAD — feature-gated, native builds fail. Current fallbacks: energy VAD, Goertzel clustering, local-API/simulated LLM.
- ScreenCaptureKit system audio (still cpal loopback fallback), true mic+system mixing, Accessibility-API text injection (still clipboard paste), audio file saving/playback, transcript full-text search, custom theme builder, updater/Homebrew (Phase 7).

---

## Resume Instructions for Next Session

### To pick up where we left off:
1. Read this file (`PROGRESS.md`) for full context.
2. Read audit reports (`audit_*.md`) for detailed findings.
3. Check current build: `cd src-tauri && cargo check` and `pnpm build`.
4. Continue from "Next Steps" below.

### Next Steps (priority order):
1. **Fix `cargo check`** — gate llama_cpp_rs/speakrs/ort behind optional features OR swap crates. Get core compiling with all Tauri plugins.
2. **Update `tauri.conf.json`** — macOSPrivateApi: true, pill window, rename "Voco", Info.plist ref.
3. **Create `src-tauri/Info.plist`** — LSUIElement, NSMicrophoneUsageDescription, NSScreenCaptureUsageDescription.
4. **Update `capabilities/default.json`** — all plugin permissions + window management.
5. **Update `package.json`** — @tauri-apps/plugin-* packages, run `pnpm install`.
6. **Update `src-tauri/src/lib.rs`** — init new plugins, wire real shortcut + tray handlers.
7. **Complete Phase 5** — embedded LLM (swap crate or API fallback), streaming, regenerate, export.
8. **Wire pill window lifecycle** — create/show/hide pill from backend.
9. **Phase 6 polish** — themes, vibrancy, export, onboarding, settings.

### Key decisions to make:
- **Embedded LLM:** llama_cpp_rs (build fails) vs llama-cpp (more maintained) vs API-only fallback
- **Diarization:** verify speakrs exists on crates.io; may need alternative
- **System audio:** ScreenCaptureKit via objc2/cidre vs cpal loopback

---

## Success Criteria (from plan) — Current Status

- [ ] Single app replaces FluidVoice + Meetily
- [ ] Dictation: < 1s latency
- [ ] Meeting: diarization ≥ 90% accuracy
- [ ] LLM summaries: streaming + action items + key points
- [ ] Provider system: seamless switch
- [ ] App startup: < 3 seconds
- [ ] Memory: < 500MB active
- [ ] Beautiful UI: 8+ themes, frosted glass, micro-animations
- [ ] 100% offline core
- [ ] Settings persisted across restarts

---

## Resume session #3 (2026-07-13)

Reliability + features pass. Full details in `CHANGELOG.md`. Highlights:

**Brand**
- New 3D "Waveform‑V" logo across app icon set, favicon, and in‑app mark (`public/logo.svg`).

**Dictation**
- Warm‑mic instant capture (pre‑built, paused stream → ~ms start; no idle mic dot); fixed initial‑word clipping (VAD load was blocking mic open).
- Save‑on‑failure: keep the audio + a placeholder history entry even if STT fails.
- Transcription language setting (default English) for Whisper + cloud APIs.

**Meetings (P3)**
- Fixed "Failed to stop meeting" / ghost recording desync (backend `meeting-status` listener, idempotent Stop, capture‑death logging).
- Crash‑safe recording (raw sidecar streamed to disk) + startup recovery of interrupted recordings; always‑on regardless of "Save recordings".
- "↻ Reprocess recording" to re‑run transcription/diarization on a saved recording.
- Fixed transcript export (all formats) — now written Rust‑side via `export_meeting_to_path`.
- Fixed meetings‑list preview showing raw Markdown; fixed recording‑view layout overflow.
- Transcription failures no longer stop recording and are surfaced to the UI.

**STT engines**
- **Audio8‑ASR 0.1B**: new fully‑native embedded ONNX engine (no Python/server), download‑on‑demand bundle, chunked for its 512‑token cap. Removed the old remote `/asr` provider preset (existing installs auto‑migrated to the embedded engine).

**LLM summaries (P5)**
- Structured templates (Meetily/Granola/Google‑Meet style) with Markdown tables (action items/decisions/topics); short/medium/detailed; summary view renders tables.
- Adaptive map‑reduce: one request when the provider allows it, else chunk‑and‑synthesize with rate‑limit backoff — fixes "fail to generate summary" (Groq free‑tier 413 on long meetings).

**Media control**
- Rewrote pause/resume for macOS 15.4+/26 (MediaRemote is entitlement‑gated) using an entitled `/usr/bin/perl` bridge + bundled helper dylib. Pauses only actually‑playing media, resumes only what Voco paused, and flushes the async command so resume isn't dropped.

**Release & repository**
- Version bumped to **0.2.0** (`package.json`, `src-tauri/Cargo.toml`, `tauri.conf.json`).
- npm package + Rust crate renamed `tauri-app` → **`voco`** (lib `tauri_app_lib` → `voco_lib`); bundle executable is now `Voco.app/Contents/MacOS/voco`.
- `Cargo.toml` metadata filled in (description, author `Arun Kashyap <kashyaparun25@gmail.com>`, `license = "MIT"`); `package.json` `license: MIT`.
- **MIT `LICENSE`** added; `README.md` fully rewritten; `CHANGELOG.md` added (0.2.0); planning/audit docs moved under `docs/`.
- `.gitignore` hardened (excludes `.signing/` keys, `src-tauri/target`, generated schemas); git repo initialized and **published: https://github.com/kashyaparun25/voco** (public).

> Detailed rationale for many of these lives in the session memory files under
> `~/.claude/.../memory/` (settings architecture, meeting‑stop desync, Audio8 provider, build/signing).
