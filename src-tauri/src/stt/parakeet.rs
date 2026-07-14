//! Parakeet TDT 0.6B STT engine backed by ONNX Runtime (`ort`).
//!
//! This implements a real, feature-gated speech-to-text engine for the
//! community ONNX export `istupakov/parakeet-tdt-0.6b-v2-onnx`.
//!
//! The model is a NeMo-style **Token-and-Duration Transducer (TDT)** and is
//! shipped as several separate ONNX files:
//!
//! * an **encoder** (Conformer) — consumes log-mel features and produces
//!   acoustic embeddings,
//! * a **decoder** / prediction network — an autoregressive LSTM over emitted
//!   tokens,
//! * a **joiner** / joint network — combines an encoder frame with a decoder
//!   state to produce token + duration logits.
//!
//! Some exports fuse the decoder and joiner into a single
//! `decoder_joint-model.onnx`; this module probes for both layouts.
//!
//! Because the actual model weights cannot be downloaded in the build
//! environment, this code is written defensively:
//! * input/output tensor names are resolved from the loaded session metadata
//!   rather than hard-coded, and are logged at load time for debugging,
//! * all model/IO paths propagate errors via `anyhow` (no `unwrap`/`panic!`),
//! * feature dimension and duration buckets are exposed as constants so they
//!   can be corrected against the real model.
//!
//! The heavy inference is synchronous; the async trait methods simply run it
//! inline (it is CPU/ANE bound, not IO bound).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use ort::session::Session;
use ort::value::{Tensor, TensorElementType, ValueType};

use crate::stt::engine::{
    EngineInfo, PartialResult, ProviderType, SttEngine, TranscriptSegment, TranscriptionResult,
};

// ---------------------------------------------------------------------------
// Tunable constants (verify against the real model / preprocessor config).
// ---------------------------------------------------------------------------

/// Expected input sample rate (Hz). Parakeet is trained on 16 kHz mono audio.
const SAMPLE_RATE: usize = 16_000;

/// Number of mel filterbank features.
///
/// NeMo Parakeet TDT 0.6B **v2** uses 128 log-mel features; the original
/// (v1) uses 80. Defaulting to 128; adjust if the shipped preprocessor config
/// says otherwise (this is a single knob).
const N_MELS: usize = 128;

/// FFT / analysis window length in samples (25 ms @ 16 kHz).
const WIN_LENGTH: usize = 400;

/// Hop length in samples (10 ms @ 16 kHz).
const HOP_LENGTH: usize = 160;

/// FFT size. NeMo rounds the 400-sample window up to the next power of two.
const N_FFT: usize = 512;

/// Lower / upper bounds of the mel filterbank (Hz).
const MEL_LOW_HZ: f32 = 0.0;
const MEL_HIGH_HZ: f32 = 8_000.0;

/// Log floor used before taking the log of the mel energies.
const LOG_EPS: f32 = 1e-9;

/// Number of TDT duration buckets. NeMo TDT commonly uses durations
/// `[0, 1, 2, 3, 4]` (5 buckets). Adjust if the joiner's duration head differs;
/// at runtime we defensively derive the split from the joiner output width.
const NUM_DURATIONS: usize = 5;

/// The default duration values corresponding to each bucket index.
const DURATION_VALUES: [usize; NUM_DURATIONS] = [0, 1, 2, 3, 4];

/// Safety cap on the number of non-blank symbols emitted at a single encoder
/// frame, to guarantee decoding terminates.
const MAX_SYMBOLS_PER_STEP: usize = 10;

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Resolved input/output tensor names for a loaded session.
struct IoNames {
    inputs: Vec<String>,
    outputs: Vec<String>,
}

impl IoNames {
    fn from_session(session: &Session) -> Self {
        IoNames {
            inputs: session.inputs().iter().map(|o| o.name().to_string()).collect(),
            outputs: session.outputs().iter().map(|o| o.name().to_string()).collect(),
        }
    }

    /// Find an input name that contains any of `needles` (case-insensitive),
    /// falling back to the input at `default_idx` if none match.
    fn input_matching(&self, needles: &[&str], default_idx: usize) -> Result<String> {
        pick(&self.inputs, needles, default_idx, "input")
    }

    fn output_matching(&self, needles: &[&str], default_idx: usize) -> Result<String> {
        pick(&self.outputs, needles, default_idx, "output")
    }
}

fn pick(names: &[String], needles: &[&str], default_idx: usize, kind: &str) -> Result<String> {
    for needle in needles {
        let lc = needle.to_lowercase();
        if let Some(found) = names.iter().find(|n| n.to_lowercase().contains(&lc)) {
            return Ok(found.clone());
        }
    }
    names
        .get(default_idx)
        .cloned()
        .ok_or_else(|| anyhow!("model has no {kind} at index {default_idx} (names: {names:?})"))
}

/// A real Parakeet TDT 0.6B engine loaded from a directory of ONNX files.
///
/// Sessions are wrapped in a [`Mutex`] because `ort`'s `Session::run` requires
/// `&mut self`, while the [`SttEngine`] trait exposes `&self`.
pub struct ParakeetEngine {
    encoder: Mutex<Session>,
    encoder_io: IoNames,
    /// The prediction (decoder) network. When the export fuses decoder+joiner
    /// into a single model, this holds that fused session and `joiner` is
    /// `None`.
    decoder: Mutex<Session>,
    decoder_io: IoNames,
    /// The joint (joiner) network, if separate from the decoder.
    joiner: Option<Mutex<Session>>,
    joiner_io: Option<IoNames>,
    /// Whether decoder and joiner are fused into a single `decoder_joint` model.
    fused_decoder_joint: bool,
    /// Whether the decoder's integer inputs (targets / target_length) are
    /// declared `int32`. The v3 int8 export uses int32 where older exports use
    /// int64 — feeding the wrong width fails at run() with a dtype error.
    decoder_ids_are_i32: bool,
    /// Zero-filled initial LSTM states, shaped from the decoder's declared
    /// state inputs. Fed on the first step of fused decoding (the v3 export
    /// requires all state inputs on every call).
    decoder_zero_states: Vec<StateTensor>,
    /// SentencePiece vocabulary (index -> piece).
    vocab: Vec<String>,
    /// Blank token index (last index in the token vocabulary by convention).
    blank_id: usize,
    /// Precomputed mel filterbank.
    mel_filters: Vec<Vec<f32>>,
}

impl ParakeetEngine {
    /// Default on-disk location of the Parakeet model directory relative to the
    /// app's models directory.
    pub fn model_dir_default(models_dir: &Path) -> PathBuf {
        models_dir.join("parakeet-tdt-0.6b")
    }

    /// Load encoder/decoder/joiner sessions and the vocabulary from
    /// `model_dir`.
    ///
    /// Filename probing:
    /// * encoder: `encoder-model.onnx` or `encoder.onnx`
    /// * decoder+joiner fused: `decoder_joint-model.onnx`
    ///   (else separate `decoder.onnx` + `joiner.onnx`)
    /// * vocab: `vocab.txt`
    pub fn new(model_dir: &Path) -> Result<Self> {
        if !model_dir.is_dir() {
            return Err(anyhow!(
                "Parakeet model directory does not exist: {}",
                model_dir.display()
            ));
        }

        // --- Locate model files -------------------------------------------------
        let encoder_path = first_existing(model_dir, &["encoder-model.onnx", "encoder.onnx"])
            .ok_or_else(|| {
                anyhow!(
                    "encoder model not found in {} (looked for encoder-model.onnx / encoder.onnx)",
                    model_dir.display()
                )
            })?;

        let fused_path = first_existing(model_dir, &["decoder_joint-model.onnx"]);
        let decoder_path = first_existing(model_dir, &["decoder.onnx", "decoder-model.onnx"]);
        let joiner_path = first_existing(model_dir, &["joiner.onnx", "joiner-model.onnx"]);

        let vocab_path = first_existing(model_dir, &["vocab.txt"]).ok_or_else(|| {
            anyhow!("vocab.txt not found in {}", model_dir.display())
        })?;

        // --- Load vocab ---------------------------------------------------------
        let vocab = load_vocab(&vocab_path)
            .with_context(|| format!("failed to load vocab from {}", vocab_path.display()))?;
        if vocab.is_empty() {
            return Err(anyhow!("vocab.txt is empty: {}", vocab_path.display()));
        }
        // Blank id: the v3 exports list the blank explicitly in vocab.txt
        // ("<blk>", last entry) — and the prediction-network embedding table is
        // sized to the vocab, so an out-of-vocab id crashes the Gather op.
        // Fall back to the old NeMo convention (blank appended after the vocab
        // as the final logit index) only when no explicit blank exists.
        let blank_id = vocab
            .iter()
            .position(|p| p == "<blk>" || p == "<blank>" || p == "<b>")
            .unwrap_or(vocab.len());
        info!(
            "Parakeet vocab loaded: {} tokens, blank_id={} ({})",
            vocab.len(),
            blank_id,
            if blank_id < vocab.len() { "explicit in vocab" } else { "appended" }
        );

        // --- Load sessions ------------------------------------------------------
        let encoder = build_session(&encoder_path)
            .with_context(|| format!("loading encoder {}", encoder_path.display()))?;
        let encoder_io = IoNames::from_session(&encoder);
        info!(
            "Parakeet encoder loaded: inputs={:?} outputs={:?}",
            encoder_io.inputs, encoder_io.outputs
        );

        let (decoder, decoder_io, joiner, joiner_io, fused_decoder_joint) =
            if let Some(fused) = fused_path {
                // Fused decoder+joiner in one model.
                let sess = build_session(&fused)
                    .with_context(|| format!("loading decoder_joint {}", fused.display()))?;
                let io = IoNames::from_session(&sess);
                info!(
                    "Parakeet decoder_joint loaded: inputs={:?} outputs={:?}",
                    io.inputs, io.outputs
                );
                (sess, io, None, None, true)
            } else {
                let dec_path = decoder_path.ok_or_else(|| {
                    anyhow!(
                        "no decoder found in {} (looked for decoder_joint-model.onnx, decoder.onnx)",
                        model_dir.display()
                    )
                })?;
                let joi_path = joiner_path.ok_or_else(|| {
                    anyhow!(
                        "separate decoder found but joiner.onnx missing in {}",
                        model_dir.display()
                    )
                })?;
                let dsess = build_session(&dec_path)
                    .with_context(|| format!("loading decoder {}", dec_path.display()))?;
                let dio = IoNames::from_session(&dsess);
                info!(
                    "Parakeet decoder loaded: inputs={:?} outputs={:?}",
                    dio.inputs, dio.outputs
                );
                let jsess = build_session(&joi_path)
                    .with_context(|| format!("loading joiner {}", joi_path.display()))?;
                let jio = IoNames::from_session(&jsess);
                info!(
                    "Parakeet joiner loaded: inputs={:?} outputs={:?}",
                    jio.inputs, jio.outputs
                );
                (dsess, dio, Some(Mutex::new(jsess)), Some(jio), false)
            };

        // The decoder's token inputs differ in width across exports (the v3
        // int8 export declares int32; older exports int64). Read the declared
        // dtype so run_decoder feeds matching tensors instead of failing at
        // inference time with "Unexpected input data type".
        let decoder_ids_are_i32 = {
            let tok_name = decoder_io
                .input_matching(&["targets", "target", "labels", "input_ids", "y"], 0)
                .unwrap_or_default();
            decoder.inputs().iter().any(|i| {
                i.name() == tok_name
                    && matches!(
                        i.dtype(),
                        ValueType::Tensor { ty: TensorElementType::Int32, .. }
                    )
            })
        };
        info!(
            "Parakeet decoder token inputs: {}",
            if decoder_ids_are_i32 { "int32 (v3-style export)" } else { "int64" }
        );

        // Zero-filled initial LSTM states. The v3 fused export REQUIRES its
        // state inputs on every call (omitting them fails mid-graph with
        // "Missing Input: input_states_2"), so the first decode step feeds
        // zeros. Dynamic dims (batch) resolve to 1; concrete dims are kept.
        let decoder_zero_states: Vec<StateTensor> = decoder
            .inputs()
            .iter()
            .filter(|i| {
                let lc = i.name().to_lowercase();
                lc.contains("state") || lc.contains("hidden") || lc.contains("cell")
            })
            .filter_map(|i| match i.dtype() {
                ValueType::Tensor { ty: TensorElementType::Float32, shape, .. } => {
                    let dims: Vec<i64> =
                        shape.iter().map(|&d| if d <= 0 { 1 } else { d }).collect();
                    let numel = dims.iter().product::<i64>().max(0) as usize;
                    Some(StateTensor {
                        name: i.name().to_string(),
                        shape: dims,
                        data: vec![0.0; numel],
                    })
                }
                _ => None,
            })
            .collect();
        if !decoder_zero_states.is_empty() {
            info!(
                "Parakeet decoder state inputs: {:?}",
                decoder_zero_states
                    .iter()
                    .map(|s| (s.name.as_str(), s.shape.clone()))
                    .collect::<Vec<_>>()
            );
        }

        // --- Precompute mel filterbank -----------------------------------------
        let mel_filters = mel::mel_filterbank(N_MELS, N_FFT, SAMPLE_RATE, MEL_LOW_HZ, MEL_HIGH_HZ);

        Ok(Self {
            encoder: Mutex::new(encoder),
            encoder_io,
            decoder: Mutex::new(decoder),
            decoder_io,
            joiner,
            joiner_io,
            fused_decoder_joint,
            decoder_ids_are_i32,
            decoder_zero_states,
            vocab,
            blank_id,
            mel_filters,
        })
    }

    /// Run the full synchronous inference pipeline on 16 kHz mono `audio`.
    fn run_inference(&self, audio: &[f32]) -> Result<String> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        // 1. Feature extraction: [n_frames][N_MELS], per-feature normalized.
        let features = mel::log_mel_features(audio, &self.mel_filters);
        let n_frames = features.len();
        if n_frames == 0 {
            return Ok(String::new());
        }
        info!("Parakeet: extracted {} mel frames ({} mels)", n_frames, N_MELS);

        // 2. Encoder.
        let (enc_out, enc_len) = self.run_encoder(&features)?;
        // enc_out is [T', D_enc] flattened row-major; enc_len frames valid.
        let (t_frames, d_enc) = enc_out_dims(&enc_out, enc_len);
        info!(
            "Parakeet: encoder output T'={} D_enc={} (valid frames {})",
            t_frames, d_enc, enc_len
        );

        // 3. TDT greedy decode.
        let tokens = self.tdt_greedy_decode(&enc_out.data, t_frames, d_enc)?;

        // 4. Detokenize.
        Ok(detokenize(&tokens, &self.vocab))
    }

    /// Run the encoder. Returns the encoder output tensor (flattened) plus the
    /// number of valid time frames.
    fn run_encoder(&self, features: &[Vec<f32>]) -> Result<(EncoderOutput, usize)> {
        let n_frames = features.len();

        // NeMo layout is [B, D, T]: features stored as [n_frames][N_MELS] must be
        // transposed to [N_MELS][n_frames] in row-major flat form.
        let mut flat = vec![0f32; N_MELS * n_frames];
        for (t, frame) in features.iter().enumerate() {
            for (m, &v) in frame.iter().enumerate() {
                flat[m * n_frames + t] = v;
            }
        }

        let feat_tensor = Tensor::from_array((vec![1i64, N_MELS as i64, n_frames as i64], flat))
            .context("building encoder feature tensor")?;
        let len_tensor = Tensor::from_array((vec![1i64], vec![n_frames as i64]))
            .context("building encoder length tensor")?;

        let feat_name = self
            .encoder_io
            .input_matching(&["audio_signal", "features", "input", "signal"], 0)?;
        let len_name = self
            .encoder_io
            .input_matching(&["length", "len", "signal_length"], 1)
            .unwrap_or_else(|_| String::new());

        let mut session = self
            .encoder
            .lock()
            .map_err(|_| anyhow!("encoder session mutex poisoned"))?;

        // Build inputs; only include the length input if the encoder declares one.
        let outputs = if !len_name.is_empty() && self.encoder_io.inputs.len() >= 2 {
            let inputs = ort::inputs![
                feat_name.as_str() => feat_tensor,
                len_name.as_str() => len_tensor,
            ];
            session.run(inputs).context("encoder inference failed")?
        } else {
            let inputs = ort::inputs![feat_name.as_str() => feat_tensor];
            session.run(inputs).context("encoder inference failed")?
        };

        // Primary output: the encoded features.
        let out_name = self
            .encoder_io
            .output_matching(&["outputs", "encoder", "encoded"], 0)?;
        let value = outputs
            .get(out_name.as_str())
            .ok_or_else(|| anyhow!("encoder output '{out_name}' missing"))?;
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .context("extracting encoder output tensor")?;
        let dims: Vec<i64> = shape.iter().copied().collect();

        // Determine valid encoded length if the encoder emits one.
        let mut enc_len: Option<usize> = None;
        if self.encoder_io.outputs.len() >= 2 {
            let len_out = self
                .encoder_io
                .output_matching(&["encoded_lengths", "length", "len"], 1)
                .ok();
            if let Some(name) = len_out {
                if let Some(v) = outputs.get(name.as_str()) {
                    if let Ok((_, ld)) = v.try_extract_tensor::<i64>() {
                        if let Some(&l) = ld.first() {
                            enc_len = Some(l.max(0) as usize);
                        }
                    }
                }
            }
        }

        // Interpret shape. NeMo encoder output is typically [B, D_enc, T'] or
        // [B, T', D_enc]. We detect which axis is time via the length hint or by
        // assuming the larger of the two trailing dims is time is unreliable, so
        // we default to NeMo's [B, D, T] and transpose to [T, D].
        let enc = normalize_encoder_output(&dims, data)?;
        let valid = enc_len.unwrap_or(enc.t_frames).min(enc.t_frames);
        Ok((enc, valid))
    }

    /// NeMo TDT greedy decoding over encoder frames.
    ///
    /// `enc` is the encoder output as `[t_frames][d_enc]` flattened row-major.
    fn tdt_greedy_decode(
        &self,
        enc: &[f32],
        t_frames: usize,
        d_enc: usize,
    ) -> Result<Vec<usize>> {
        if self.fused_decoder_joint {
            return self.tdt_greedy_decode_fused(enc, t_frames, d_enc);
        }
        let mut emitted: Vec<usize> = Vec::new();

        // Decoder state: for the very first step, feed the blank/SOS token.
        let mut prev_token: i64 = self.blank_id as i64;
        // Opaque LSTM states threaded between decoder invocations, if the model
        // exposes them. Stored as (name, shape, data) triples.
        let mut dec_states: Vec<StateTensor> = Vec::new();
        let mut have_states = false;

        // Cache the current decoder output; recompute only when a token is
        // emitted (standard transducer optimization).
        let mut decoder_out = self.run_decoder(prev_token, &mut dec_states, &mut have_states)?;

        let mut t = 0usize;
        while t < t_frames {
            let enc_frame = &enc[t * d_enc..(t + 1) * d_enc];

            let mut symbols_this_step = 0usize;
            loop {
                let (token, duration) = self.run_joiner(enc_frame, &decoder_out)?;

                if token == self.blank_id || symbols_this_step >= MAX_SYMBOLS_PER_STEP {
                    // Blank: advance time by the predicted duration (>=1 to make
                    // progress).
                    let adv = DURATION_VALUES
                        .get(duration)
                        .copied()
                        .unwrap_or(1)
                        .max(1);
                    t += adv;
                    break;
                } else {
                    // Emit token, update decoder state.
                    emitted.push(token);
                    prev_token = token as i64;
                    decoder_out =
                        self.run_decoder(prev_token, &mut dec_states, &mut have_states)?;
                    symbols_this_step += 1;

                    // TDT: a non-blank emission also advances time by its
                    // predicted duration.
                    let adv = DURATION_VALUES.get(duration).copied().unwrap_or(0);
                    if adv > 0 {
                        t += adv;
                        break;
                    }
                    // duration == 0: stay on the same frame and emit again.
                }
            }
        }

        Ok(emitted)
    }

    /// Greedy TDT decoding against the FUSED `decoder_joint` export (the
    /// istupakov v3 layout): ONE session call per step runs the prediction
    /// network and joint together — encoder frame `[1, D, 1]` + previous token
    /// + LSTM states in, TDT logits + next states out. The prediction network
    /// only advances on emitted tokens, so state outputs from blank steps are
    /// discarded.
    fn tdt_greedy_decode_fused(
        &self,
        enc: &[f32],
        t_frames: usize,
        d_enc: usize,
    ) -> Result<Vec<usize>> {
        let mut emitted: Vec<usize> = Vec::new();
        let mut states = self.decoder_zero_states.clone();
        let mut prev_token = self.blank_id as i64; // blank doubles as SOS
        let mut t = 0usize;

        while t < t_frames {
            let enc_frame = &enc[t * d_enc..(t + 1) * d_enc];
            let mut symbols_this_step = 0usize;
            loop {
                let (token, duration, new_states) =
                    self.run_fused_step(enc_frame, prev_token, &states)?;

                if token == self.blank_id || symbols_this_step >= MAX_SYMBOLS_PER_STEP {
                    // Blank: keep the old states, advance by the predicted
                    // duration (>= 1 so decoding always makes progress).
                    let adv = DURATION_VALUES.get(duration).copied().unwrap_or(1).max(1);
                    t += adv;
                    break;
                }

                emitted.push(token);
                prev_token = token as i64;
                states = new_states;
                symbols_this_step += 1;

                // TDT: a non-blank emission also advances time by its duration.
                let adv = DURATION_VALUES.get(duration).copied().unwrap_or(0);
                if adv > 0 {
                    t += adv;
                    break;
                }
                // duration == 0: stay on this frame and emit again.
            }
        }

        Ok(emitted)
    }

    /// One fused decoder_joint call: returns `(token, duration_bucket, states)`.
    fn run_fused_step(
        &self,
        enc_frame: &[f32],
        prev_token: i64,
        states: &[StateTensor],
    ) -> Result<(usize, usize, Vec<StateTensor>)> {
        // encoder_outputs is declared [B, D, T]; a single frame is [1, D, 1].
        let enc_tensor = Tensor::from_array((
            vec![1i64, enc_frame.len() as i64, 1i64],
            enc_frame.to_vec(),
        ))
        .context("building fused encoder tensor")?;

        let enc_name = self
            .decoder_io
            .input_matching(&["encoder_outputs", "encoder"], 0)?;
        let tok_name = self
            .decoder_io
            .input_matching(&["targets", "target", "labels", "input_ids", "y"], usize::MAX)?;
        let len_name = self
            .decoder_io
            .input_matching(&["target_length", "length", "len"], usize::MAX)?;

        let mut named: Vec<(std::borrow::Cow<str>, ort::session::SessionInputValue)> = Vec::new();
        named.push((enc_name.into(), enc_tensor.into()));
        if self.decoder_ids_are_i32 {
            named.push((
                tok_name.into(),
                Tensor::from_array((vec![1i64, 1i64], vec![prev_token as i32]))
                    .context("building fused token tensor (i32)")?
                    .into(),
            ));
            named.push((
                len_name.into(),
                Tensor::from_array((vec![1i64], vec![1i32]))
                    .context("building fused length tensor (i32)")?
                    .into(),
            ));
        } else {
            named.push((
                tok_name.into(),
                Tensor::from_array((vec![1i64, 1i64], vec![prev_token]))
                    .context("building fused token tensor")?
                    .into(),
            ));
            named.push((
                len_name.into(),
                Tensor::from_array((vec![1i64], vec![1i64]))
                    .context("building fused length tensor")?
                    .into(),
            ));
        }
        for st in states {
            let tensor = Tensor::from_array((st.shape.clone(), st.data.clone()))
                .context("rebuilding fused state tensor")?;
            named.push((st.name.clone().into(), tensor.into()));
        }

        let mut session = self
            .decoder
            .lock()
            .map_err(|_| anyhow!("decoder session mutex poisoned"))?;
        let outputs = session.run(named).context("fused decoder_joint inference failed")?;

        let out_name = self
            .decoder_io
            .output_matching(&["outputs", "logits", "output"], 0)?;
        let value = outputs
            .get(out_name.as_str())
            .ok_or_else(|| anyhow!("fused output '{out_name}' missing"))?;
        let (_, logits) = value
            .try_extract_tensor::<f32>()
            .context("extracting fused TDT logits")?;
        let (token, duration) = split_tdt_logits(logits, self.vocab.len(), self.blank_id);

        // Capture the state outputs, mapped back to their input names.
        let mut new_states: Vec<StateTensor> = Vec::new();
        for out in &self.decoder_io.outputs {
            let lc = out.to_lowercase();
            if lc.contains("state") || lc.contains("hidden") || lc.contains("cell") {
                if let Some(v) = outputs.get(out.as_str()) {
                    if let Ok((sh, sd)) = v.try_extract_tensor::<f32>() {
                        let in_name = map_state_out_to_in(out, &self.decoder_io.inputs)
                            .unwrap_or_else(|| out.clone());
                        new_states.push(StateTensor {
                            name: in_name,
                            shape: sh.iter().map(|&d| d).collect(),
                            data: sd.to_vec(),
                        });
                    }
                }
            }
        }

        Ok((token, duration, new_states))
    }

    /// Run the decoder / prediction network for `token`, threading LSTM state.
    ///
    /// Returns the decoder output as a flat `Vec<f32>` (the joint network's
    /// prediction-side input).
    fn run_decoder(
        &self,
        token: i64,
        states: &mut Vec<StateTensor>,
        have_states: &mut bool,
    ) -> Result<Vec<f32>> {
        // Prediction-network target ([1, 1]) and target length ([1] = 1),
        // built at the integer width the export declares (int32 for the v3
        // int8 export, int64 for older ones).
        let (tok_tensor, len_tensor): (
            ort::session::SessionInputValue,
            ort::session::SessionInputValue,
        ) = if self.decoder_ids_are_i32 {
            (
                Tensor::from_array((vec![1i64, 1i64], vec![token as i32]))
                    .context("building decoder token tensor (i32)")?
                    .into(),
                Tensor::from_array((vec![1i64], vec![1i32]))
                    .context("building decoder length tensor (i32)")?
                    .into(),
            )
        } else {
            (
                Tensor::from_array((vec![1i64, 1i64], vec![token]))
                    .context("building decoder token tensor")?
                    .into(),
                Tensor::from_array((vec![1i64], vec![1i64]))
                    .context("building decoder length tensor")?
                    .into(),
            )
        };

        let tok_name = self
            .decoder_io
            .input_matching(&["targets", "target", "labels", "input_ids", "y"], 0)?;

        let mut session = self
            .decoder
            .lock()
            .map_err(|_| anyhow!("decoder session mutex poisoned"))?;

        // Assemble named inputs dynamically so we can thread hidden states.
        let mut named: Vec<(std::borrow::Cow<str>, ort::session::SessionInputValue)> = Vec::new();
        named.push((tok_name.clone().into(), tok_tensor));

        // Optional target length input.
        if let Ok(len_name) =
            self.decoder_io.input_matching(&["target_length", "length", "len"], usize::MAX)
        {
            if self.decoder_io.inputs.iter().any(|n| n == &len_name) && len_name != tok_name {
                named.push((len_name.into(), len_tensor));
            }
        }

        // Thread hidden/cell states from the previous step if we have them.
        // On the first step we omit them and let the model use its declared
        // defaults; the first real state outputs (captured below) populate them
        // for subsequent steps.
        if *have_states {
            for st in states.iter() {
                let tensor = Tensor::from_array((st.shape.clone(), st.data.clone()))
                    .context("rebuilding decoder state tensor")?;
                named.push((st.name.clone().into(), tensor.into()));
            }
        }

        let outputs = session
            .run(named)
            .context("decoder inference failed")?;

        // Extract the decoder output (prediction embedding).
        let out_name = self
            .decoder_io
            .output_matching(&["outputs", "decoder", "output", "prednet"], 0)?;
        let value = outputs
            .get(out_name.as_str())
            .ok_or_else(|| anyhow!("decoder output '{out_name}' missing"))?;
        let (_, data) = value
            .try_extract_tensor::<f32>()
            .context("extracting decoder output")?;
        let dec_vec = data.to_vec();

        // Capture any state outputs for the next step.
        let mut new_states: Vec<StateTensor> = Vec::new();
        for out in &self.decoder_io.outputs {
            let lc = out.to_lowercase();
            if lc.contains("state") || lc.contains("hidden") || lc.contains("cell") {
                if let Some(v) = outputs.get(out.as_str()) {
                    if let Ok((sh, sd)) = v.try_extract_tensor::<f32>() {
                        // Map the state *output* name back to the corresponding
                        // state *input* name if a matching one exists.
                        let in_name = map_state_out_to_in(out, &self.decoder_io.inputs)
                            .unwrap_or_else(|| out.clone());
                        new_states.push(StateTensor {
                            name: in_name,
                            shape: sh.iter().map(|&d| d as i64).collect(),
                            data: sd.to_vec(),
                        });
                    }
                }
            }
        }
        if !new_states.is_empty() {
            *states = new_states;
            *have_states = true;
        }

        Ok(dec_vec)
    }

    /// Run the joint network for one encoder frame + decoder output. Returns
    /// `(argmax_token, argmax_duration_bucket)`.
    fn run_joiner(&self, enc_frame: &[f32], dec_out: &[f32]) -> Result<(usize, usize)> {
        let enc_tensor =
            Tensor::from_array((vec![1i64, 1i64, enc_frame.len() as i64], enc_frame.to_vec()))
                .context("building joiner encoder tensor")?;
        let dec_tensor =
            Tensor::from_array((vec![1i64, 1i64, dec_out.len() as i64], dec_out.to_vec()))
                .context("building joiner decoder tensor")?;

        // For the fused decoder_joint layout the joint logits come out of the
        // decoder session's forward pass; but here run_joiner is only invoked in
        // the separate-model layout OR by re-running the fused model as a joiner.
        let (io, sess_mutex): (&IoNames, &Mutex<Session>) = if self.fused_decoder_joint {
            (&self.decoder_io, &self.decoder)
        } else {
            (
                self.joiner_io
                    .as_ref()
                    .ok_or_else(|| anyhow!("joiner IO metadata missing"))?,
                self.joiner
                    .as_ref()
                    .ok_or_else(|| anyhow!("joiner session missing"))?,
            )
        };

        let enc_name = io.input_matching(&["encoder_outputs", "encoder", "input_1", "f"], 0)?;
        let dec_name = io.input_matching(&["decoder_outputs", "decoder", "input_2", "g"], 1)?;

        let mut session = sess_mutex
            .lock()
            .map_err(|_| anyhow!("joiner session mutex poisoned"))?;

        let inputs = ort::inputs![
            enc_name.as_str() => enc_tensor,
            dec_name.as_str() => dec_tensor,
        ];
        let outputs = session.run(inputs).context("joiner inference failed")?;

        let out_name = io.output_matching(&["outputs", "logits", "output"], 0)?;
        let value = outputs
            .get(out_name.as_str())
            .ok_or_else(|| anyhow!("joiner output '{out_name}' missing"))?;
        let (_, logits) = value
            .try_extract_tensor::<f32>()
            .context("extracting joiner logits")?;

        Ok(split_tdt_logits(logits, self.vocab.len(), self.blank_id))
    }
}

/// A named opaque state tensor threaded through the decoder LSTM.
#[derive(Clone)]
struct StateTensor {
    name: String,
    shape: Vec<i64>,
    data: Vec<f32>,
}

/// Encoder output normalized to `[t_frames][d_enc]` row-major flat form.
struct EncoderOutput {
    data: Vec<f32>,
    t_frames: usize,
    d_enc: usize,
}

fn enc_out_dims(enc: &EncoderOutput, _valid: usize) -> (usize, usize) {
    (enc.t_frames, enc.d_enc)
}

/// Interpret a raw encoder output tensor into `[T', D_enc]` row-major.
///
/// NeMo's Conformer encoder emits `[B, D_enc, T']`. We transpose to
/// `[T', D_enc]`. If the export already uses `[B, T', D_enc]` we detect that
/// heuristically: the feature/embedding dimension for the 0.6B model is 1024,
/// so whichever trailing axis equals a "hidden-like" size (>= 256) is treated
/// as `D_enc`.
fn normalize_encoder_output(dims: &[i64], data: &[f32]) -> Result<EncoderOutput> {
    if dims.len() != 3 {
        return Err(anyhow!(
            "unexpected encoder output rank {} (dims {:?})",
            dims.len(),
            dims
        ));
    }
    let a = dims[1].max(0) as usize; // axis 1
    let b = dims[2].max(0) as usize; // axis 2
    if a == 0 || b == 0 {
        return Err(anyhow!("encoder output has empty dim: {:?}", dims));
    }

    // Heuristic: the hidden dim (D_enc) is typically >= 256 and constant, while
    // the time dim varies. Assume NeMo [B, D, T]: axis1 = D_enc, axis2 = T'.
    // If axis2 looks more like a hidden size than axis1, swap.
    let (d_enc, t_frames, layout_dt) = if b >= 256 && a < 256 {
        // Looks like [B, T', D_enc].
        (b, a, false)
    } else {
        // Default NeMo [B, D_enc, T'].
        (a, b, true)
    };

    let mut out = vec![0f32; t_frames * d_enc];
    if layout_dt {
        // data is [D_enc, T'] row-major -> transpose to [T', D_enc].
        for d in 0..d_enc {
            for t in 0..t_frames {
                out[t * d_enc + d] = data[d * t_frames + t];
            }
        }
    } else {
        // data already [T', D_enc].
        out.copy_from_slice(&data[..t_frames * d_enc]);
    }

    Ok(EncoderOutput {
        data: out,
        t_frames,
        d_enc,
    })
}

/// Split joint-network logits into token + duration heads and return the
/// argmax of each.
///
/// The joiner emits `vocab_size + 1 (blank) + NUM_DURATIONS` logits. The first
/// `blank_id + 1` entries are token logits (including blank), the trailing
/// `NUM_DURATIONS` are duration logits.
fn split_tdt_logits(logits: &[f32], vocab_size: usize, blank_id: usize) -> (usize, usize) {
    let token_count = blank_id + 1; // vocab + blank
    let total = logits.len();

    // Defensive: derive the actual duration head width from the output size when
    // possible; fall back to NUM_DURATIONS.
    let dur_count = if total > token_count {
        total - token_count
    } else {
        0
    };

    let token_logits = &logits[..token_count.min(total)];
    let token = argmax(token_logits).unwrap_or(blank_id);

    let duration = if dur_count > 0 {
        let dur_logits = &logits[token_count..token_count + dur_count];
        argmax(dur_logits).unwrap_or(1)
    } else {
        // No explicit duration head detected: behave like a standard RNN-T
        // (advance by 1 on blank).
        1
    };

    let _ = vocab_size;
    (token, duration)
}

fn argmax(xs: &[f32]) -> Option<usize> {
    if xs.is_empty() {
        return None;
    }
    let mut best = 0usize;
    let mut best_v = xs[0];
    for (i, &v) in xs.iter().enumerate().skip(1) {
        if v > best_v {
            best_v = v;
            best = i;
        }
    }
    Some(best)
}

/// Map a decoder state *output* name to the matching state *input* name so the
/// LSTM state can be fed back in on the next step (e.g. `output_states_1` ->
/// `input_states_1`).
fn map_state_out_to_in(out_name: &str, inputs: &[String]) -> Option<String> {
    // Extract trailing digits to match by index.
    let idx: String = out_name.chars().filter(|c| c.is_ascii_digit()).collect();
    inputs
        .iter()
        .find(|n| {
            let lc = n.to_lowercase();
            (lc.contains("state") || lc.contains("hidden") || lc.contains("cell"))
                && n.chars().filter(|c| c.is_ascii_digit()).collect::<String>() == idx
        })
        .cloned()
}

/// Join SentencePiece pieces into text: `▁` marks a word boundary (space).
fn detokenize(tokens: &[usize], vocab: &[String]) -> String {
    let mut out = String::new();
    for &t in tokens {
        if let Some(piece) = vocab.get(t) {
            // SentencePiece uses U+2581 (▁) to mark the start of a word.
            let replaced = piece.replace('\u{2581}', " ");
            out.push_str(&replaced);
        }
    }
    out.trim().to_string()
}

/// Load a SentencePiece `vocab.txt`, one piece per line.
fn load_vocab(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let mut v = Vec::new();
    for line in text.lines() {
        // Formats seen in the wild: "piece", "piece\tscore", and "piece id"
        // (the istupakov v3 export is space-separated). Take the piece and
        // preserve the ▁ prefix; SentencePiece pieces never contain raw
        // spaces, so a trailing space-separated integer is an id, not content.
        let piece = line.split('\t').next().unwrap_or(line);
        let piece = match piece.rsplit_once(' ') {
            Some((p, id)) if !p.is_empty() && !id.is_empty()
                && id.chars().all(|c| c.is_ascii_digit()) => p,
            _ => piece,
        };
        v.push(piece.trim_end_matches('\r').to_string());
    }
    Ok(v)
}

/// Locate the first existing file among `candidates` in `dir`.
fn first_existing(dir: &Path, candidates: &[&str]) -> Option<PathBuf> {
    for name in candidates {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Build an `ort` session, preferring CoreML (Apple Neural Engine / GPU) and
/// falling back to the default CPU provider.
fn build_session(path: &Path) -> Result<Session> {
    use ort::execution_providers::CoreMLExecutionProvider;

    // Try CoreML first; if EP registration fails, fall back to a plain session.
    let builder = Session::builder().context("creating ort session builder")?;
    match builder.with_execution_providers([CoreMLExecutionProvider::default().build()]) {
        Ok(b) => {
            let mut b = b;
            b.commit_from_file(path)
                .with_context(|| format!("committing session (CoreML) from {}", path.display()))
        }
        Err(e) => {
            warn!(
                "CoreML EP unavailable for {} ({e}); falling back to CPU",
                path.display()
            );
            let mut b = Session::builder().context("creating fallback ort session builder")?;
            b.commit_from_file(path)
                .with_context(|| format!("committing session (CPU) from {}", path.display()))
        }
    }
}

// ---------------------------------------------------------------------------
// SttEngine impl
// ---------------------------------------------------------------------------

use async_trait::async_trait;

#[async_trait]
impl SttEngine for ParakeetEngine {
    async fn transcribe(&self, audio: &[f32]) -> Result<TranscriptionResult> {
        let start = std::time::Instant::now();
        let n_samples = audio.len();

        let text = self.run_inference(audio)?;

        let end_time = n_samples as f64 / SAMPLE_RATE as f64;
        let segment = TranscriptSegment {
            id: uuid::Uuid::new_v4(),
            meeting_id: None,
            text: text.clone(),
            start_time: 0.0,
            end_time,
            speaker_id: None,
            confidence: 1.0,
            words: Vec::new(),
        };

        Ok(TranscriptionResult {
            text,
            segments: vec![segment],
            language: Some("en".to_string()),
            processing_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn transcribe_streaming(
        &self,
        audio: &[f32],
        tx: tokio::sync::mpsc::Sender<PartialResult>,
    ) -> Result<TranscriptionResult> {
        // Parakeet TDT here is non-streaming: run once, emit a single final
        // partial result.
        let res = self.transcribe(audio).await?;
        let _ = tx
            .send(PartialResult {
                text: res.text.clone(),
                is_final: true,
            })
            .await;
        Ok(res)
    }

    fn info(&self) -> EngineInfo {
        EngineInfo {
            name: "Parakeet TDT 0.6B".to_string(),
            provider_type: ProviderType::Embedded,
            supports_streaming: false,
            supports_timestamps: true,
        }
    }
}

// ---------------------------------------------------------------------------
// mel: self-contained log-mel spectrogram (pure Rust radix-2 FFT).
// ---------------------------------------------------------------------------

/// Pure-Rust mel-spectrogram feature extraction matching NeMo's Parakeet
/// preprocessor as closely as practical:
/// * 25 ms Hann window, 10 ms hop, 16 kHz,
/// * power spectrum via radix-2 FFT (window zero-padded to [`N_FFT`]),
/// * triangular mel filterbank over 0–8000 Hz,
/// * natural log with a small floor,
/// * per-feature (per-mel-bin) mean/variance normalization ("per_feature").
mod mel {
    use super::{HOP_LENGTH, LOG_EPS, N_FFT, N_MELS, WIN_LENGTH};

    /// Convert Hz to mel (HTK formula, as used by NeMo/librosa htk=True).
    fn hz_to_mel(hz: f32) -> f32 {
        2595.0 * (1.0 + hz / 700.0).log10()
    }

    /// Convert mel to Hz (HTK formula).
    fn mel_to_hz(mel: f32) -> f32 {
        700.0 * (10f32.powf(mel / 2595.0) - 1.0)
    }

    /// Build a triangular mel filterbank: `n_mels` filters over the positive
    /// half of the FFT (`n_fft/2 + 1` bins).
    pub fn mel_filterbank(
        n_mels: usize,
        n_fft: usize,
        sample_rate: usize,
        low_hz: f32,
        high_hz: f32,
    ) -> Vec<Vec<f32>> {
        let n_bins = n_fft / 2 + 1;
        let low_mel = hz_to_mel(low_hz);
        let high_mel = hz_to_mel(high_hz);

        // n_mels + 2 equally spaced points in mel space.
        let mut mel_points = vec![0f32; n_mels + 2];
        for (i, p) in mel_points.iter_mut().enumerate() {
            let m = low_mel + (high_mel - low_mel) * (i as f32) / (n_mels as f32 + 1.0);
            *p = mel_to_hz(m);
        }

        // Map the Hz points to FFT bin indices (fractional).
        let bin = |hz: f32| -> f32 { hz * (n_fft as f32) / (sample_rate as f32) };

        let mut filters = vec![vec![0f32; n_bins]; n_mels];
        for m in 0..n_mels {
            let f_left = bin(mel_points[m]);
            let f_center = bin(mel_points[m + 1]);
            let f_right = bin(mel_points[m + 2]);
            for k in 0..n_bins {
                let kf = k as f32;
                let w = if kf >= f_left && kf <= f_center {
                    if (f_center - f_left).abs() < f32::EPSILON {
                        0.0
                    } else {
                        (kf - f_left) / (f_center - f_left)
                    }
                } else if kf > f_center && kf <= f_right {
                    if (f_right - f_center).abs() < f32::EPSILON {
                        0.0
                    } else {
                        (f_right - kf) / (f_right - f_center)
                    }
                } else {
                    0.0
                };
                filters[m][k] = w.max(0.0);
            }
        }
        filters
    }

    /// Periodic Hann window of length `n`.
    fn hann_window(n: usize) -> Vec<f32> {
        if n == 0 {
            return Vec::new();
        }
        (0..n)
            .map(|i| {
                let x = std::f32::consts::PI * 2.0 * (i as f32) / (n as f32);
                0.5 - 0.5 * x.cos()
            })
            .collect()
    }

    /// In-place iterative radix-2 Cooley–Tukey FFT. `re`/`im` have length that
    /// must be a power of two.
    fn fft_radix2(re: &mut [f32], im: &mut [f32]) {
        let n = re.len();
        if n <= 1 {
            return;
        }
        debug_assert!(n.is_power_of_two());

        // Bit-reversal permutation.
        let mut j = 0usize;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j |= bit;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }

        // Butterfly stages.
        let mut len = 2usize;
        while len <= n {
            let ang = -2.0 * std::f32::consts::PI / (len as f32);
            let (wr_step, wi_step) = (ang.cos(), ang.sin());
            let half = len / 2;
            let mut i = 0;
            while i < n {
                let mut wr = 1.0f32;
                let mut wi = 0.0f32;
                for k in 0..half {
                    let a = i + k;
                    let b = i + k + half;
                    let tr = wr * re[b] - wi * im[b];
                    let ti = wr * im[b] + wi * re[b];
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                    let new_wr = wr * wr_step - wi * wi_step;
                    wi = wr * wi_step + wi * wr_step;
                    wr = new_wr;
                }
                i += len;
            }
            len <<= 1;
        }
    }

    /// Compute per-feature-normalized log-mel features.
    ///
    /// Returns a `Vec` of frames, each of length [`N_MELS`].
    pub fn log_mel_features(audio: &[f32], mel_filters: &[Vec<f32>]) -> Vec<Vec<f32>> {
        if audio.len() < WIN_LENGTH {
            return Vec::new();
        }

        let window = hann_window(WIN_LENGTH);
        let n_bins = N_FFT / 2 + 1;

        let n_frames = 1 + (audio.len() - WIN_LENGTH) / HOP_LENGTH;
        let mut frames: Vec<Vec<f32>> = Vec::with_capacity(n_frames);

        let mut re = vec![0f32; N_FFT];
        let mut im = vec![0f32; N_FFT];

        for f in 0..n_frames {
            let start = f * HOP_LENGTH;

            // Windowed, zero-padded frame.
            for v in re.iter_mut() {
                *v = 0.0;
            }
            for v in im.iter_mut() {
                *v = 0.0;
            }
            for i in 0..WIN_LENGTH {
                re[i] = audio[start + i] * window[i];
            }

            fft_radix2(&mut re, &mut im);

            // Power spectrum over positive frequencies.
            let mut power = vec![0f32; n_bins];
            for (k, p) in power.iter_mut().enumerate() {
                *p = re[k] * re[k] + im[k] * im[k];
            }

            // Apply mel filters, then log.
            let mut mel = vec![0f32; N_MELS];
            for (m, filt) in mel_filters.iter().enumerate() {
                let mut e = 0f32;
                for (k, &w) in filt.iter().enumerate() {
                    if w != 0.0 {
                        e += w * power[k];
                    }
                }
                mel[m] = (e + LOG_EPS).ln();
            }
            frames.push(mel);
        }

        per_feature_normalize(&mut frames);
        frames
    }

    /// Per-feature (per mel bin) mean/std normalization across time
    /// (NeMo `normalize = "per_feature"`).
    fn per_feature_normalize(frames: &mut [Vec<f32>]) {
        if frames.is_empty() {
            return;
        }
        let t = frames.len() as f32;
        for m in 0..N_MELS {
            let mut mean = 0f32;
            for fr in frames.iter() {
                mean += fr[m];
            }
            mean /= t;
            let mut var = 0f32;
            for fr in frames.iter() {
                let d = fr[m] - mean;
                var += d * d;
            }
            // Unbiased-ish; guard against zero.
            var /= t;
            let std = (var + 1e-5).sqrt();
            for fr in frames.iter_mut() {
                fr[m] = (fr[m] - mean) / std;
            }
        }
    }
}
