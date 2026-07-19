//! Optional start/stop sound cues for dictation, gated by the
//! `sound_feedback` setting. Ships seven synthesized cue styles (bundled
//! WAVs under resources/sounds) selectable via the `sound_cue_style`
//! setting; falls back to macOS system sounds if the resources are missing.

use crate::storage::Database;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

pub enum Cue {
    Start,
    Stop,
}

/// Bundled cue styles: (id, display name). The id maps to
/// `resources/sounds/<id>-start.wav` / `<id>-stop.wav`.
pub const CUE_STYLES: &[(&str, &str)] = &[
    ("deep_tap", "Deep tap"),
    ("triad_roll", "Triad roll"),
    ("kalimba", "Kalimba"),
    ("pluck", "Harp pluck"),
    ("vibes", "Vibraphone"),
    ("bloop", "Bloop"),
    ("wood", "Woodblock"),
];

pub const DEFAULT_CUE_STYLE: &str = "deep_tap";

static SOUNDS_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Record where the bundled cue WAVs live (resolved from the app's resource
/// dir at startup). No-op if called twice.
pub fn init_sounds_dir(dir: PathBuf) {
    let _ = SOUNDS_DIR.set(dir);
}

fn cue_path(style: &str, which: &Cue) -> Option<PathBuf> {
    let dir = SOUNDS_DIR.get()?;
    let suffix = match which {
        Cue::Start => "start",
        Cue::Stop => "stop",
    };
    let p = dir.join(format!("{style}-{suffix}.wav"));
    p.exists().then_some(p)
}

fn play(style: &str, which: Cue) {
    let path = cue_path(style, &which)
        .or_else(|| cue_path(DEFAULT_CUE_STYLE, &which))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            // Resources missing entirely (dev build without bundling) —
            // keep the old system-sound behavior as a last resort.
            match which {
                Cue::Start => "/System/Library/Sounds/Pop.aiff".to_string(),
                Cue::Stop => "/System/Library/Sounds/Bottle.aiff".to_string(),
            }
        });
    // Fire-and-forget; never block the dictation path on audio playback.
    let _ = Command::new("afplay").arg(path).spawn();
}

pub fn cue(db: &Database, which: Cue) {
    let enabled = db
        .get_setting("sound_feedback")
        .ok()
        .flatten()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    if !enabled {
        return;
    }
    let style = db
        .get_setting("sound_cue_style")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CUE_STYLE.to_string());
    play(&style, which);
}

/// Preview a style regardless of the `sound_feedback` setting (settings UI).
/// Plays start, then stop after a beat, without blocking the caller.
pub fn preview(style: &str) {
    let style = style.to_string();
    std::thread::spawn(move || {
        play(&style, Cue::Start);
        std::thread::sleep(std::time::Duration::from_millis(600));
        play(&style, Cue::Stop);
    });
}
