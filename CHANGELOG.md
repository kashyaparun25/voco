# Changelog

All notable changes to Voco. Dates are the day work landed.

## [0.3.0] — 2026-07-14

A dictation-reliability release: transcription that works at any mic level, a
pill that shows up everywhere, and Parakeet as the new default local model.

### Added
- **Parakeet TDT 0.6B v3** (istupakov int8 ONNX export) as the dictation
  model — punctuated, capitalized output at ~0.85s for 8s of audio. The
  embedded engine now handles the v3 fused export (int32 token inputs,
  required LSTM states, one-call prediction+joint decoding, space-separated
  vocab with explicit `<blk>`).
- **Real voice waveform in the pill.** Bars are a scrolling render of the
  live mic amplitude (20ms resolution), normalized adaptively between the
  room's noise floor and a decaying peak — honest at any input volume.
- **Processing spinner.** Stopping keeps the pill up with a spinner through
  transcription; it disappears only after the text is pasted, so a stop
  always gives visible feedback.
- **"No speech detected" toast** (with a mic-level hint) when a session ends
  without detectable speech — sessions no longer vanish silently.

### Fixed
- **Dictation missing words or whole sessions on quiet mics.** The energy VAD
  now tracks an adaptive noise floor instead of a fixed threshold, the whole
  session is buffered from t0 with the VAD only annotating the speech
  envelope (late triggers can't drop words), audio is peak-normalized before
  STT, sub-1s clips are padded (whisper.cpp asserts on shorter buffers), and
  the capture tail is drained so the final phoneme survives.
- **Pill invisible over other Spaces/fullscreen apps** (e.g. Chrome on
  another display). The pill is now a real nonactivating NSPanel
  (tauri-nspanel) with canJoinAllSpaces + fullScreenAuxiliary at status
  window level — and interacting with it no longer steals focus from the app
  being dictated into.

## [0.2.0] — 2026-07-13

A large reliability + features pass focused on the meeting/dictation pipeline, a
new local STT engine, and a fresh brand.

### Added
- **Seamless install.** Prebuilt DMG on [GitHub Releases](https://github.com/kashyaparun25/voco/releases),
  a Homebrew cask (`brew install --cask kashyaparun25/voco/voco`), and a one‑line
  `curl | bash` installer (`scripts/install.sh`). Release DMGs are ad‑hoc signed
  (`src-tauri/tauri.release.conf.json`) and built by CI on version tags
  (`.github/workflows/release.yml`); the Homebrew/one‑line paths clear the
  Gatekeeper quarantine flag so unsigned builds open without warnings.
- **New 3D "Waveform‑V" brand.** App icon (dock/Finder/tray, full `.icns`/`.ico`
  set), favicon, and the in‑app sidebar mark are now an audio‑waveform whose
  envelope traces a "V" (voice made visible). Master source: `public/logo.svg`.
- **Audio8‑ASR 0.1B — native embedded STT engine.** Fully in‑process Rust port
  (128‑mel Whisper features → ONNX audio tower → MLP adapter → 8‑layer KV‑cache
  decoder) via the `ort` runtime — **no Python, no local server**. Downloads the
  ~0.9 GB int8 bundle on demand (`SUPPORTED_BUNDLES`), selectable as an embedded
  STT model. 7 languages incl. Cantonese. Long audio is auto‑chunked to respect
  the model's 512‑token context.
- **Warm‑mic instant capture.** The microphone stream is pre‑built at launch and
  kept paused, so pressing the dictation hotkey starts recording in ~ms instead
  of ~450 ms — the first word is no longer clipped. **No idle mic indicator**
  (the dot appears only while dictating).
- **Crash‑safe meeting recording + recovery.** Meeting audio is streamed to a
  raw sidecar on disk while recording; a clean stop finalizes the WAV, and an
  unclean shutdown (crash/force‑quit/power loss) is recovered into a WAV on next
  launch. Always on, independent of the "Save recordings" setting.
- **Re‑process recording.** A "↻ Reprocess recording" action re‑runs
  transcription + diarization over a saved meeting recording (e.g. to recover a
  transcript that failed live).
- **Dictation durability.** If transcription fails (dead API, out of credits,
  etc.), the audio clip and a placeholder history entry are still saved so the
  recording isn't lost.
- **Structured summary templates** (Meetily/Granola/Google‑Meet style) with
  Markdown **tables** — Action Items (`Owner | Action Item | Due`), Decisions,
  Attendees, topic‑by‑topic Key Discussion Points — across General, Standup, 1:1,
  Sales, Interview, Retrospective, and Decision‑Log templates. Short / medium /
  detailed length controls depth. The summary view now renders tables.
- **Adaptive map‑reduce summarization.** The whole transcript is summarized in
  one request when the provider can handle it; only if the provider rejects it
  as too large does Voco condense in chunks and synthesize — so long meetings
  summarize regardless of a provider's per‑request token limit (with rate‑limit
  backoff).
- **Transcription language setting** (Settings → Recording), default **English**,
  with Auto‑detect / common languages / custom ISO code. Applies to Whisper and
  cloud APIs (OpenAI/Groq).

### Fixed
- **Media pause/resume during dictation.** Reworked for macOS 15.4+/26, where
  Apple gated the MediaRemote framework behind an entitlement. Detection +
  pause/play now run through an entitled `/usr/bin/perl` bridge loading a small
  helper (the FluidVoice/`mediaremote-adapter` technique). Voco now pauses media
  only if it's actually playing and resumes **only what it paused** — it no
  longer wrongly starts already‑paused media, and the resume command is flushed
  so it isn't dropped.
- **"Failed to stop meeting" / ghost recording.** A meeting could show a running
  timer forever after the backend had already stopped. The UI now listens for
  backend `meeting-status`, resets on stop, and the Stop button is idempotent;
  the backend logs + surfaces unexpected capture‑device deaths instead of
  stopping silently.
- **"Fail to generate summary" on long meetings.** Root cause was the provider's
  per‑request token limit (Groq free tier, HTTP 413) — fixed by the adaptive
  map‑reduce above.
- **Export transcript failed for all formats.** The frontend wrote the file via
  the fs plugin, which refuses arbitrary save‑dialog paths; now written on the
  Rust side (`export_meeting_to_path`).
- **Initial words clipped on dictation start.** ~450 ms of that was the VAD model
  loading *before* the mic opened; capture now starts first (fixed by the warm
  mic + reordering).
- **Meetings list preview showed raw Markdown.** Now flattened to plain text.
- **Recording view layout broke** (buttons escaping their area). Header now
  flexes/truncates the title and pins the controls.
- **Transcription failure no longer stops meeting recording**, and failures are
  surfaced to the UI (e.g. out‑of‑credits) instead of a silently‑empty transcript.

### Changed
- Removed the deprecated remote Audio8 (`/asr` server) provider preset now that
  the native engine exists; existing installs are migrated from that provider to
  the embedded engine automatically.
- Whisper transcription language is now configurable (was hardcoded to English).

### Dev / internal
- Added `rustfft`, `tokenizers`, `memmap2`, `ndarray-npy` (Audio8), bundled
  `mediaremote-helper.dylib` + `mediaremote-adapter.pl` (media control), and
  `scripts/audio8/` dev tooling (reference server runner + golden‑tensor dumper).
- Build/signing notes: system `openssl` is LibreSSL (drop `-legacy`); the
  "Voco Dev" self‑signed identity periodically drops from the keychain and must
  be re‑imported (see README).
