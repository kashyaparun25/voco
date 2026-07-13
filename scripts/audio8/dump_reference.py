#!/usr/bin/env python3
"""Dump golden intermediate tensors from the Python Audio8 reference pipeline so
the Rust port can be verified stage-by-stage (bit-exactness of mel, audio tower,
projector, prompt, prefill logits, and final text).

Run inside the model venv:
  ~/.voco/audio8/.venv/bin/python scripts/audio8/dump_reference.py <audio.wav> [out_dir]

Writes <out_dir>/*.npy + summary.json with shapes and float64 checksums.
"""
import sys, os, json, hashlib
from pathlib import Path
import numpy as np

REPO = Path(os.environ.get("AUDIO8_DIR", str(Path.home() / ".voco/audio8"))) / "repo"
sys.path.insert(0, str(REPO))
from asr_onnx_runtime import OnnxCacheAsrEngine  # noqa: E402


def ck(a):
    a = np.asarray(a)
    return {
        "shape": list(a.shape),
        "dtype": str(a.dtype),
        "sum": float(np.asarray(a, dtype=np.float64).sum()),
        "mean": float(np.asarray(a, dtype=np.float64).mean()) if a.size else 0.0,
        "sha1_first1k": hashlib.sha1(np.asarray(a, dtype=np.float32).tobytes()[:4000]).hexdigest(),
    }


def main():
    audio_path = Path(sys.argv[1])
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("/tmp/audio8_ref")
    out.mkdir(parents=True, exist_ok=True)
    eng = OnnxCacheAsrEngine(str(REPO / "model_bundle"), cache_precision="int8", audio_precision="int8")

    audio_bytes = audio_path.read_bytes()
    from asr_onnx_runtime import load_audio_bytes
    audio = load_audio_bytes(audio_bytes, eng.sampling_rate)
    feature, sample_count, enc_len, hop = eng._extract_features(audio)
    # raw audio-tower outputs (before projector) to isolate int8 vs projector bugs
    _hidden, _mask = eng.audio_session.run(None, {
        "audios": feature.astype("float32"),
        "audio_feature_lengths": np.asarray([enc_len], dtype=np.int64),
    })
    np.save(out / "audio_hidden.npy", np.asarray(_hidden, dtype=np.float32))
    np.save(out / "audio_valid_mask.npy", np.asarray(_mask).astype(np.int64))
    audio_emb = eng._audio_embeddings(feature, sample_count, enc_len, hop)
    token_ids, embeds = eng._initial_embeddings(audio_emb, language=None)

    caches = eng._new_cache()
    prefill_logits = eng._run_cache_prefill(embeds, caches)

    result = eng.transcribe(audio_bytes, max_new_tokens=200)

    summary = {
        "audio_samples": int(audio.shape[0]),
        "sample_count": int(sample_count),
        "encoder_feature_len": int(enc_len),
        "hop_length": int(hop),
        "feature": ck(feature),
        "audio_embeddings": ck(audio_emb),
        "prompt_token_ids_len": len(token_ids),
        "prompt_token_ids_head": token_ids[:24],
        "prompt_token_ids_tail": token_ids[-8:],
        "embeds": ck(embeds),
        "prefill_last_logits": ck(prefill_logits),
        "prefill_argmax": int(np.argmax(prefill_logits)),
        "final_text": result["text"],
        "generated_tokens": result["generated_tokens"],
    }
    np.save(out / "feature.npy", feature)
    np.save(out / "audio_embeddings.npy", audio_emb)
    np.save(out / "embeds.npy", embeds)
    np.save(out / "prefill_last_logits.npy", prefill_logits)
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
