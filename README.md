<div align="center">

# Voco

**Private, on-device voice dictation & meeting notes for macOS.**

Talk anywhere and get instant text at your cursor; record meetings and get diarized transcripts with AI summaries — all running locally by default, no cloud required.

</div>

---

## What it does

- **Dictation** — press a hotkey (default **Left Option ⌥**), speak, and the transcribed text is pasted at your cursor in any app. Near‑instant start (the mic is kept "warm"), with app‑aware AI cleanup (punctuation, capitalization, custom dictionary, per‑app prompts).
- **Meetings** — records both your microphone and system audio (the other participants), transcribes live, separates speakers (neural diarization), and generates a structured AI summary.
- **Import & re‑process** — drop in an audio file (mp3/m4a/wav/flac) to transcribe it, or re‑run transcription on any saved recording.
- **Local‑first** — embedded speech‑to‑text (Whisper, Parakeet, Audio8‑ASR) and optional embedded LLM summaries run entirely on your Mac. Cloud providers (OpenAI, Groq, NVIDIA, Ollama, LM Studio) are optional.

## Highlights

- **Instant, clip‑free capture** — a pre‑armed ("warm") microphone means the first word is never cut off, with **no persistent orange mic indicator** (the mic only turns on while you dictate).
- **Never lose a recording** — meeting audio is streamed to disk crash‑safely; if the app is force‑quit or crashes mid‑meeting, the recording is recovered on next launch and can be re‑transcribed. Failed dictations keep their audio too.
- **Smart media handling** — playing media (Apple Music, Spotify, browser video) is paused while you dictate and resumed after — and *only* if Voco was the one that paused it (works on macOS 15.4+/26 via an entitled MediaRemote bridge).
- **Structured summaries** — Google‑Meet / Granola / Meetily‑style templates (General, Standup, 1:1, Sales, Interview, Retrospective, Decision Log) with proper Markdown tables (action items with owner/due, decisions, topic‑by‑topic notes). Short / medium / detailed length. Long meetings are summarized via adaptive map‑reduce so they never exceed a provider's token limits.
- **Speaker diarization** — neural pyannote‑based separation, on by default.
- **Your keys, your models** — per‑task provider/model selection; one connection can serve STT for dictation and an LLM for summaries without collision.

## Requirements

- **macOS 12+** (Apple Silicon recommended; some embedded models are arm64‑only).
- **Permissions** (macOS will prompt): Microphone, Screen Recording (for meeting system‑audio), Accessibility + Input Monitoring (for the global hotkey and paste).
- To build: **Rust** (stable) + **Node.js** + **pnpm/npm**.

## Build & run

```bash
# install JS deps
npm install

# dev
npm run tauri dev

# release .app bundle
npm run tauri build -- --bundles app
```

The release build signs with the self‑signed **"Voco Dev"** identity (see `tauri.conf.json`). If signing fails with `Voco Dev: no identity found`, re‑import the cert:

```bash
cd .signing
openssl pkcs12 -export -inkey key.pem -in cert.pem -out /tmp/voco.p12 -passout pass:voco -name "Voco Dev"
security import /tmp/voco.p12 -k ~/Library/Keychains/login.keychain-db -P voco -T /usr/bin/codesign
```

> The system `openssl` is LibreSSL — omit the `-legacy` flag. The cert imports as untrusted (self‑signed) but `codesign` still uses it.

### Cargo features

The default build already includes on‑device diarization + the ONNX STT stack. Feature flags in `src-tauri/Cargo.toml`:

| Feature | What it enables |
| --- | --- |
| `neural-diarization` | pyannote speaker separation (default) |
| `parakeet` | Parakeet TDT 0.6B ONNX STT (default) |
| `audio8` | Audio8‑ASR 0.1B native ONNX STT (default) |
| `embedded-llm` | Local GGUF LLM summaries via llama.cpp |
| `macos-native` | ScreenCaptureKit system audio + CGEvent paste |

## Speech‑to‑text engines

Voco supports several STT engines, selectable per task (Dictation / Meetings) in **Settings**:

- **Whisper** (embedded, GGUF via whisper.cpp) — download tiny→large‑v3 from the model list. Metal‑accelerated. Defaults to **English** (configurable).
- **Parakeet TDT 0.6B** (embedded, ONNX) — fast multilingual, downloaded on demand.
- **Audio8‑ASR 0.1B** (embedded, ONNX) — a tiny multilingual speech‑LLM (7 languages incl. Cantonese). Fully native — downloads ~0.9 GB on demand, no Python/server. Auto‑detects language.
- **Cloud** — OpenAI Whisper, Groq, NVIDIA NIM, Ollama, LM Studio (OpenAI‑compatible). Bring your own key.

### Transcription language

**Settings → Recording → Transcription language** (default **English**). Applies to Whisper and cloud APIs (OpenAI/Groq); pick *Auto‑detect*, a language, or a custom ISO code. The embedded Audio8 model auto‑detects and ignores this setting.

## AI summaries

**Settings → Meetings → Summary** selects the LLM provider/model. Summaries use structured templates with tables. For transcripts that exceed a provider's per‑request token limit (e.g. Groq's free tier), Voco automatically condenses the transcript in chunks and then synthesizes the final summary — so it works regardless of meeting length. Choose a higher‑limit provider (or a local LLM) to summarize long meetings in a single fast pass.

## Privacy

- Nothing leaves your Mac unless you configure a cloud provider.
- The mic indicator appears **only while actively dictating/recording** — never at idle.
- Recordings are stored locally under Application Support (`Save recordings` is opt‑in; a crash‑safety buffer protects in‑progress recordings regardless).

## Project layout

```
src/                 React + TypeScript UI (windows/, components/, hooks/)
src-tauri/src/
  services/          dictation, meeting, media_control, hotkey, text_processing…
  stt/               whisper, parakeet, audio8, api (cloud), manager (downloads)
  llm/               summary prompt/templates, streaming client, engines
  diarization/       neural speaker separation
  audio/             capture (warm mic), mixer, resampler, VAD, system capture
scripts/audio8/      dev tools for the Audio8 model (reference server + golden dumps)
```

## Docs

- `CHANGELOG.md` — release notes.
- `docs/PROGRESS.md` — implementation status / history.
- `docs/` — original planning & audit notes (`implementation_plan.md`, `audit_*.md`), kept as historical snapshots.

## License

[MIT](LICENSE) © Arun Kashyap. Voco bundles third‑party models and frameworks under their respective licenses.
