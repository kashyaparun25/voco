# Audio8-ASR (local) — trial setup

Voco can use the [Audio8-ASR-0.1B](https://huggingface.co/AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime)
ONNX model as a **local** speech-to-text provider. Voco already knows how to talk
to it: a built-in provider preset **"Audio8-ASR (local)"** points at
`http://localhost:7860` and speaks the model server's `POST /asr` API natively
(no OpenAI-compatibility shim needed).

You only need to run the model server locally.

## 1. Start the server

```bash
bash scripts/audio8/run-server.sh
```

First run downloads the **int8 subset** (~1.1 GB) into `~/.voco/audio8/repo` and
creates a Python venv. Leave it running; it serves on `http://127.0.0.1:7860`.

> The full repo is ~3.4 GB because it ships fp32 + int8 + int4 copies of every
> graph. The script skips the fp32/int4 files we don't use. Note the int8 runtime
> is still ~1.1 GB, not 200 MB — the 200 MB figure is just `audio_hidden_int8.onnx`;
> the 311 MB `token_embedding.npy` and 414 MB `lm_logits.onnx` have no int8 variants
> and are required. Set `AUDIO8_FULL=1` before running to download everything.

## 2. Select it in Voco

Settings → **Dictation** (and/or **Meetings**) → set the STT provider to
**Audio8-ASR (local)**, model **audio8-asr-0.1b**. Use the "Test connection"
button — it pings `/health` on the server.

That's it. Dictate or record a meeting and compare quality against Whisper/Groq.
Because it's local, there are no credits/quotas involved.

## Notes / limits
- 7 languages: English, Chinese, **Cantonese**, French, Japanese, German, Korean.
- The server caps each request at 30 s of audio and 512 decoder tokens — fine for
  Voco, which already splits speech into short VAD segments before transcribing.
- Precision defaults to int8/int8 (lowest memory). Override with
  `ASR_CACHE_PRECISION` / `ASR_AUDIO_PRECISION` env vars (`fp32`, `int8`, `int4`).
- No published accuracy (WER) numbers — this is a trial. Keep Whisper/Groq as your
  main provider until you've confirmed quality on your own audio.
- To free the disk later: `rm -rf ~/.voco/audio8`.
