#!/usr/bin/env bash
# Download + run the Audio8-ASR-0.1B local ONNX server so Voco can use it for
# speech-to-text. Everything runs inside a dedicated, isolated Python virtualenv
# (~/.voco/audio8/.venv) with the model's exact pinned dependencies — so there
# are no version clashes with your system Python. Voco ships a provider preset
# "Audio8-ASR (local)" pointing at http://localhost:7860 (this server's default),
# and speaks its POST /asr contract natively.
#
# Usage:   bash scripts/audio8/run-server.sh
# Env:     AUDIO8_DIR (default ~/.voco/audio8), HOST (127.0.0.1), PORT (7860),
#          ASR_CACHE_PRECISION / ASR_AUDIO_PRECISION (default int8 / int8),
#          AUDIO8_FULL=1 to download all precisions (~3.4GB) instead of int8 (~1.1GB).
set -euo pipefail

REPO="AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime"
DIR="${AUDIO8_DIR:-$HOME/.voco/audio8}"
VENV="$DIR/.venv"
export HOST="${HOST:-127.0.0.1}"
export PORT="${PORT:-7860}"
export ASR_CACHE_PRECISION="${ASR_CACHE_PRECISION:-int8}"
export ASR_AUDIO_PRECISION="${ASR_AUDIO_PRECISION:-int8}"

# ── 1. Pick a Python the pinned deps support ─────────────────────────────────
# numpy 1.26.4 / onnxruntime 1.22.0 ship no wheels for Python 3.13+, so prefer a
# 3.10–3.12 interpreter. We actually RUN each candidate (not just `command -v`)
# so a dead pyenv shim that's on PATH but won't execute is skipped, not chosen.
pick_python() {
  local c ver p root
  # PATH-named interpreters first…
  local candidates=(python3.12 python3.11 python3.10)
  # …then pyenv-installed versions by REAL path (bypasses non-executable shims)…
  root="$( (command -v pyenv >/dev/null 2>&1 && pyenv root) 2>/dev/null || echo "$HOME/.pyenv" )"
  for p in "$root"/versions/3.12*/bin/python3 "$root"/versions/3.11*/bin/python3 "$root"/versions/3.10*/bin/python3; do
    [ -x "$p" ] && candidates+=("$p")
  done
  # …then Homebrew formula paths.
  for p in /opt/homebrew/opt/python@3.12/bin/python3.12 /opt/homebrew/opt/python@3.11/bin/python3.11 \
           /opt/homebrew/opt/python@3.10/bin/python3.10 /usr/local/opt/python@3.12/bin/python3.12; do
    [ -x "$p" ] && candidates+=("$p")
  done
  # Require 3.10–3.12 (the pins numpy 1.26.4 / onnxruntime 1.22.0 have no 3.13+ wheels).
  for c in "${candidates[@]}"; do
    if ver="$("$c" -c 'import sys; print("%d.%d" % sys.version_info[:2])' 2>/dev/null)"; then
      case "$ver" in 3.10|3.11|3.12) echo "$c $ver"; return 0 ;; esac
    fi
  done
  return 1
}
SEL="$(pick_python)" || {
  echo "❌ Need Python 3.10–3.12 (the model's pinned deps have no wheels for 3.13+)."
  echo "   Install one, e.g.:  pyenv install 3.12   or   brew install python@3.12 ,  then re-run."
  exit 1
}
PYBIN="${SEL% *}"; PYVER="${SEL#* }"
echo "Using Python $PYVER  ($PYBIN)"

# ── 2. Isolated virtualenv (reused across runs; recreate by deleting it) ──────
mkdir -p "$DIR"
# If a previous run left a venv on an incompatible Python (e.g. a 3.13 default),
# rebuild it with the interpreter we just selected.
if [ -x "$VENV/bin/python" ]; then
  cur="$("$VENV/bin/python" -c 'import sys; print("%d.%d" % sys.version_info[:2])' 2>/dev/null || echo none)"
  case "$cur" in
    3.10|3.11|3.12) : ;;
    *) echo "Recreating venv (was Python $cur; need 3.10–3.12)…"; rm -rf "$VENV" ;;
  esac
fi
if [ ! -x "$VENV/bin/python" ]; then
  echo "Creating isolated virtualenv at $VENV (Python $PYVER)…"
  "$PYBIN" -m venv "$VENV"
fi
# shellcheck disable=SC1091
source "$VENV/bin/activate"
python -m pip install --quiet --upgrade pip
# huggingface_hub only for the download. Pin <1.0: transformers 4.57.6 (in the
# model's requirements) requires huggingface-hub<1.0, and the newer 1.x line
# would otherwise be pulled in and conflict.
python -m pip install --quiet "huggingface_hub>=0.34,<1.0"

# ── 3. Download the model (int8 subset by default) ───────────────────────────
if [ ! -f "$DIR/repo/server.py" ]; then
  if [ "${AUDIO8_FULL:-0}" = "1" ]; then
    echo "Downloading FULL $REPO into $DIR/repo (~3.4GB, first run only)…"
    python -c "from huggingface_hub import snapshot_download; snapshot_download('$REPO', local_dir='$DIR/repo')"
  else
    # The repo ships every precision (~3.4GB). We run int8, so skip the unused
    # fp32/int4 graphs — cuts the download to ~1.1GB. AUDIO8_FULL=1 grabs all.
    echo "Downloading int8 subset of $REPO into $DIR/repo (~1.1GB, first run only)…"
    python - "$REPO" "$DIR/repo" <<'PY'
import sys
from huggingface_hub import snapshot_download
repo, dst = sys.argv[1], sys.argv[2]
snapshot_download(
    repo,
    local_dir=dst,
    ignore_patterns=[
        "model_bundle/audio_hidden.onnx",       # 880MB fp32 (int8 used instead)
        "model_bundle/lm_cache_decode.onnx",    # 414MB fp32
        "model_bundle/lm_cache_prefill.onnx",   # 414MB fp32
        "*_int4.onnx", "*_int4.onnx.data",      # int4 variant (unused at int8)
    ],
)
PY
  fi
fi

# ── 4. Install the model's exact pinned requirements into the venv ───────────
echo "Installing pinned requirements into the venv…"
python -m pip install --quiet -r "$DIR/repo/requirements-onnx.txt"

# ── 5. Serve ─────────────────────────────────────────────────────────────────
echo "──────────────────────────────────────────────────────────────"
echo " Audio8-ASR on http://$HOST:$PORT  (endpoint: POST /asr)  [venv: $VENV]"
echo " In Voco → Settings → Dictation / Meetings, choose provider"
echo " 'Audio8-ASR (local)' and model 'audio8-asr-0.1b'."
echo "──────────────────────────────────────────────────────────────"
cd "$DIR/repo"
exec python server.py
