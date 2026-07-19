//! MOSS-Transcribe-Diarize 0.9B — joint speech transcription + speaker
//! diarization in a single pass, via transcribe.cpp (GGUF, Metal).
//!
//! This is an *offline finalize* engine, not a streaming one: it takes a whole
//! recording (16 kHz mono f32) and generates a speaker-attributed, timestamped
//! transcript as text in the model's canonical inline format:
//!
//! ```text
//! [0.48][S01]Welcome everyone[1.66][12.26][S02]New pipeline ready[13.81]
//! ```
//!
//! The tags are emergent generated text (not special tokens), so this module
//! parses them defensively: unknown bracket tags (acoustic events etc.) are
//! kept as transcript text, malformed spans are tolerated, and timestamps are
//! clamped to be monotonic.
//!
//! Memory: inference costs roughly 85 MB per minute of audio on top of the
//! ~1 GB model file — callers should bound input length (see
//! `MAX_AUDIO_SAMPLES`) and fall back to the pyannote finalize pass beyond it.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use log::info;

/// GGUF file we download (Q8_0: best WER of the published quants; Q4_K_M has
/// known tail failures — empty outputs and en→zh language drift).
pub const MOSS_MODEL_FILE: &str = "MOSS-Transcribe-Diarize-Q8_0.gguf";

/// Subdirectory under `models_dir` where the bundle lives.
pub const MOSS_MODEL_SUBDIR: &str = "moss-transcribe-diarize";

/// Longest input we allow a single MOSS pass over (100 minutes). Inference
/// memory grows ~85 MB/min, so this caps the pass at roughly 8.5 GB.
pub const MAX_AUDIO_SAMPLES: usize = 100 * 60 * 16_000;

/// One diarized utterance parsed from the model's inline output.
#[derive(Debug, Clone, PartialEq)]
pub struct MossSegment {
    /// Start time in seconds.
    pub start: f64,
    /// End time in seconds.
    pub end: f64,
    /// Speaker index as emitted by the model (`[S01]` → 1). Labels are only
    /// meaningful within a single pass.
    pub speaker: u32,
    pub text: String,
}

pub struct MossEngine {
    model: transcribe_cpp::Model,
}

impl MossEngine {
    /// Default on-disk location of the GGUF, mirroring the other bundles.
    pub fn model_path_default(models_dir: &Path) -> PathBuf {
        models_dir.join(MOSS_MODEL_SUBDIR).join(MOSS_MODEL_FILE)
    }

    pub fn new(model_path: &Path) -> Result<Self> {
        info!("Loading MOSS-Transcribe-Diarize model from {:?}", model_path);
        let model = transcribe_cpp::Model::load(model_path)
            .map_err(|e| anyhow!("Failed to load MOSS model: {e}"))?;
        Ok(Self { model })
    }

    /// Run one full-recording pass and return diarized segments.
    ///
    /// `language`: the port supports `en` / `zh`; anything else (or `None`)
    /// runs auto-detection.
    pub fn transcribe_diarized(
        &self,
        pcm: &[f32],
        language: Option<&str>,
    ) -> Result<Vec<MossSegment>> {
        if pcm.len() > MAX_AUDIO_SAMPLES {
            return Err(anyhow!(
                "Audio too long for a single MOSS pass ({}min > {}min)",
                pcm.len() / 16_000 / 60,
                MAX_AUDIO_SAMPLES / 16_000 / 60
            ));
        }
        let mut session = self
            .model
            .session()
            .map_err(|e| anyhow!("MOSS session: {e}"))?;
        let mut opts = transcribe_cpp::RunOptions::default();
        // Only pass through languages the MOSS port understands; otherwise
        // let the model auto-detect rather than erroring on e.g. "es".
        opts.language = match language {
            Some(l @ ("en" | "zh")) => Some(l.to_string()),
            _ => None,
        };
        let out = session
            .run(pcm, &opts)
            .map_err(|e| anyhow!("MOSS inference: {e}"))?;
        info!(
            "MOSS pass done: {} chars of diarized text",
            out.text.len()
        );
        Ok(parse_diarized(&out.text))
    }
}

/// A lexed piece of the model output. `Ts`/`Spk` keep the raw bracket text so
/// a bracket that turns out to be inline speech (e.g. "item [3] is ready")
/// can be restored into the transcript verbatim.
#[derive(Debug, PartialEq)]
enum Tok {
    /// `[12.34]` — a timestamp in seconds.
    Ts(f64, String),
    /// `[S01]` — a speaker tag.
    Spk(u32, String),
    /// Plain transcript text (including any non-timestamp, non-speaker
    /// bracket tag, e.g. acoustic events, kept verbatim).
    Text(String),
}

fn lex(s: &str) -> Vec<Tok> {
    let mut toks: Vec<Tok> = Vec::new();
    let mut push_text = |toks: &mut Vec<Tok>, t: &str| {
        if t.is_empty() {
            return;
        }
        if let Some(Tok::Text(prev)) = toks.last_mut() {
            prev.push_str(t);
        } else {
            toks.push(Tok::Text(t.to_string()));
        }
    };

    let mut rest = s;
    while !rest.is_empty() {
        let Some(open) = rest.find('[') else {
            push_text(&mut toks, rest);
            break;
        };
        push_text(&mut toks, &rest[..open]);
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(']') else {
            // Unterminated bracket: keep verbatim as text.
            push_text(&mut toks, &rest[open..]);
            break;
        };
        let inner = &after_open[..close];
        let raw = format!("[{inner}]");
        if let Ok(v) = inner.trim().parse::<f64>() {
            if v.is_finite() && v >= 0.0 {
                toks.push(Tok::Ts(v, raw));
            } else {
                push_text(&mut toks, &raw);
            }
        } else if let Some(n) = inner
            .strip_prefix('S')
            .and_then(|d| (!d.is_empty()).then(|| d.parse::<u32>().ok()).flatten())
        {
            toks.push(Tok::Spk(n, raw));
        } else {
            // Unknown tag (acoustic event, etc.) — treat as transcript text.
            push_text(&mut toks, &raw);
        }
        rest = &after_open[close + 1..];
    }
    toks
}

/// Parse MOSS's inline `[start][Sxx]text[end]` stream into segments.
///
/// Follows the author-repo parser's confirmation rule: a timestamp only
/// terminates a segment when it is followed by another bracket (the next
/// segment's start / speaker) or end-of-stream — a numeric bracket in the
/// middle of running text ("item [3] is ready") is speech, not a timestamp.
/// Also tolerates omitted end timestamps (the following segment's start is
/// shared), non-monotonic times (clamped), and tag-free output (returned as
/// one untimed segment).
pub fn parse_diarized(text: &str) -> Vec<MossSegment> {
    let toks = lex(text);
    let mut segs: Vec<MossSegment> = Vec::new();
    let mut saw_tags = false;
    let mut i = 0;

    while i < toks.len() {
        // Seek the next `[ts][Sxx]` pair.
        let (start, speaker) = match (&toks[i], toks.get(i + 1)) {
            (Tok::Ts(t, _), Some(Tok::Spk(s, _))) => {
                saw_tags = true;
                i += 2;
                (*t, *s)
            }
            _ => {
                i += 1;
                continue;
            }
        };

        // Collect the segment body. A bracket token flows back into the text
        // unless it can actually terminate the segment: a timestamp followed
        // by more text is inline speech, and a speaker tag not preceded by a
        // timestamp can never start a segment.
        let mut body = String::new();
        loop {
            match (toks.get(i), toks.get(i + 1)) {
                (Some(Tok::Text(t)), _) => {
                    body.push_str(t);
                    i += 1;
                }
                (Some(Tok::Ts(_, raw)), Some(Tok::Text(t))) => {
                    // Whitespace-only text after a timestamp is separation
                    // between segments, not speech — the timestamp is an end.
                    if t.trim().is_empty() {
                        break;
                    }
                    body.push_str(raw);
                    i += 1;
                }
                (Some(Tok::Spk(_, raw)), _) => {
                    body.push_str(raw);
                    i += 1;
                }
                _ => break,
            }
        }

        // End timestamp handling (only reached with Ts-then-bracket or EOF):
        //  - `text [end] [start'][S..]` → consume [end]
        //  - `text [ts][S..]`           → ts is the NEXT segment's start; the
        //                                  current segment ends there too
        //  - `text <eof>`               → unknown end; fall back to start
        let end = match (toks.get(i), toks.get(i + 1)) {
            (Some(Tok::Ts(e, _)), Some(Tok::Spk(..))) => *e, // shared: don't consume
            (Some(Tok::Ts(e, _)), _) => {
                i += 1;
                *e
            }
            _ => start,
        };

        let body = body.trim();
        if body.is_empty() {
            continue;
        }
        let end = end.max(start);
        segs.push(MossSegment {
            start,
            end,
            speaker,
            text: body.to_string(),
        });
    }

    // Fallback: model produced plain text with no parsable tags at all —
    // return it as one untimed segment rather than dropping the transcript.
    // (If tags parsed but every body was empty, there was no speech: return
    // nothing rather than echoing tag soup.)
    if segs.is_empty() && !saw_tags {
        let plain = text.trim();
        if !plain.is_empty() {
            segs.push(MossSegment {
                start: 0.0,
                end: 0.0,
                speaker: 1,
                text: plain.to_string(),
            });
        }
    }

    segs
}

/// `SttEngine` adapter so MOSS can also serve per-utterance callers
/// (dictation, meeting live captions if ever selected there). Runs the joint
/// model and returns plain text — speaker labels and timestamps are parsed
/// out, since a single-voice utterance doesn't need diarization metadata.
pub struct MossSttEngine {
    engine: MossEngine,
    language: Option<String>,
}

impl MossSttEngine {
    pub fn new(model_path: &Path, language: Option<String>) -> Result<Self> {
        Ok(Self {
            engine: MossEngine::new(model_path)?,
            language,
        })
    }
}

#[async_trait::async_trait]
impl crate::stt::SttEngine for MossSttEngine {
    async fn transcribe(&self, audio: &[f32]) -> Result<crate::stt::TranscriptionResult> {
        let t0 = std::time::Instant::now();
        let segs = self.engine.transcribe_diarized(audio, self.language.as_deref())?;
        let text = segs
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        Ok(crate::stt::TranscriptionResult {
            text,
            segments: Vec::new(),
            language: self.language.clone(),
            processing_time_ms: t0.elapsed().as_millis() as u64,
        })
    }

    async fn transcribe_streaming(
        &self,
        audio: &[f32],
        tx: tokio::sync::mpsc::Sender<crate::stt::PartialResult>,
    ) -> Result<crate::stt::TranscriptionResult> {
        // Offline model: no partials, just the final result.
        let result = self.transcribe(audio).await?;
        let _ = tx
            .send(crate::stt::PartialResult { text: result.text.clone(), is_final: true })
            .await;
        Ok(result)
    }

    fn info(&self) -> crate::stt::EngineInfo {
        crate::stt::EngineInfo {
            name: "MOSS-Transcribe-Diarize 0.9B".to_string(),
            provider_type: crate::stt::ProviderType::Embedded,
            supports_streaming: false,
            supports_timestamps: true,
        }
    }
}

/// Merge consecutive segments from the same speaker separated by at most
/// `max_gap` seconds, keeping merged text under `max_chars`. Adapted from the
/// author repo's subtitle postprocess (`_merge_adjacent`), retuned for meeting
/// transcripts: the goal is one card per speaker *turn*, not screen-sized
/// subtitle lines — MOSS emits fine-grained utterances that otherwise render
/// as a stream of one-line cards.
pub fn merge_same_speaker(
    segs: Vec<MossSegment>,
    max_gap: f64,
    max_chars: usize,
) -> Vec<MossSegment> {
    let mut out: Vec<MossSegment> = Vec::new();
    for s in segs {
        if let Some(last) = out.last_mut() {
            if last.speaker == s.speaker
                && s.start - last.end <= max_gap
                && last.text.len() + 1 + s.text.len() <= max_chars
            {
                last.text.push(' ');
                last.text.push_str(&s.text);
                last.end = last.end.max(s.end);
                continue;
            }
        }
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_format() {
        let segs = parse_diarized(
            "[0.48][S01]Welcome everyone[1.66][12.26][S02]New pipeline ready[13.81]",
        );
        assert_eq!(
            segs,
            vec![
                MossSegment { start: 0.48, end: 1.66, speaker: 1, text: "Welcome everyone".into() },
                MossSegment { start: 12.26, end: 13.81, speaker: 2, text: "New pipeline ready".into() },
            ]
        );
    }

    #[test]
    fn tolerates_missing_end_timestamp() {
        // Second segment starts immediately after the first's text: the shared
        // timestamp serves as both end-of-A and start-of-B.
        let segs = parse_diarized("[0.5][S01]hello[2.0][S02]world[3.5]");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].end, 2.0);
        assert_eq!(segs[1].start, 2.0);
        assert_eq!(segs[1].end, 3.5);
    }

    #[test]
    fn keeps_unknown_tags_as_text() {
        let segs = parse_diarized("[1.0][S03]so [laughter] anyway[2.5]");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].speaker, 3);
        assert_eq!(segs[0].text, "so [laughter] anyway");
    }

    #[test]
    fn clamps_backwards_end() {
        let segs = parse_diarized("[5.0][S01]oops[4.0]");
        assert_eq!(segs.len(), 1);
        assert!(segs[0].end >= segs[0].start);
    }

    #[test]
    fn plain_text_fallback() {
        let segs = parse_diarized("just plain text output");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].speaker, 1);
        assert_eq!(segs[0].text, "just plain text output");
    }

    #[test]
    fn empty_output() {
        assert!(parse_diarized("").is_empty());
        assert!(parse_diarized("[0.1][S01][0.5]").is_empty()); // tag soup, no text
    }

    #[test]
    fn multi_digit_speakers_and_unterminated_bracket() {
        // The [11.0] is followed by more text, so it is NOT a confirmed end
        // (author-repo rule); the whole tail stays as transcript text rather
        // than the segment being dropped.
        let segs = parse_diarized("[10.0][S12]twelve speakers deep[11.0] trailing [garbage");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].speaker, 12);
        assert_eq!(segs[0].text, "twelve speakers deep[11.0] trailing [garbage");
    }

    // Ported from the author repo's test_transcript_parser.py: a numeric
    // bracket inside running text is speech, not an end timestamp.
    #[test]
    fn numeric_brackets_inside_text_are_preserved() {
        let segs = parse_diarized("[0.5][S01]item [3] is ready[2.0]");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "item [3] is ready");
        assert_eq!(segs[0].end, 2.0);
    }

    #[test]
    fn noise_before_first_segment_is_ignored() {
        let segs = parse_diarized("noise noise [0.5][S01]hello[1.0]");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "hello");
    }

    #[test]
    fn whitespace_between_segments_is_ignored() {
        let segs = parse_diarized("[0.5][S01]a[1.0] \n [2.0][S02]b[3.0]");
        assert_eq!(segs.len(), 2);
        assert_eq!((segs[0].text.as_str(), segs[0].end), ("a", 1.0));
        assert_eq!((segs[1].text.as_str(), segs[1].start), ("b", 2.0));
    }

    #[test]
    fn stray_speaker_tag_in_text_is_kept() {
        let segs = parse_diarized("[0.5][S01]he said [S09] loudly[2.0]");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "he said [S09] loudly");
    }

    #[test]
    fn merges_same_speaker_turns() {
        let segs = parse_diarized(
            "[0.0][S01]first bit[1.0][1.3][S01]second bit[2.0][5.0][S02]other voice[6.0][6.2][S01]back again[7.0]",
        );
        let merged = merge_same_speaker(segs, 1.0, 600);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].text, "first bit second bit");
        assert_eq!((merged[0].start, merged[0].end), (0.0, 2.0));
        assert_eq!(merged[1].speaker, 2);
        assert_eq!(merged[2].text, "back again");
    }

    #[test]
    fn merge_respects_gap_and_char_budget() {
        let segs = vec![
            MossSegment { start: 0.0, end: 1.0, speaker: 1, text: "a".into() },
            MossSegment { start: 5.0, end: 6.0, speaker: 1, text: "far away".into() },
        ];
        assert_eq!(merge_same_speaker(segs, 1.0, 600).len(), 2); // gap 4s > 1s

        let segs = vec![
            MossSegment { start: 0.0, end: 1.0, speaker: 1, text: "x".repeat(300) },
            MossSegment { start: 1.1, end: 2.0, speaker: 1, text: "y".repeat(300) },
        ];
        assert_eq!(merge_same_speaker(segs, 1.0, 400).len(), 2); // over char cap
    }
}
