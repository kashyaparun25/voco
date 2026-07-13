//! Optional start/stop sound cues for dictation, gated by the
//! `sound_feedback` setting. Uses macOS `afplay` (non-blocking spawn) with
//! built-in system sounds so there are no bundled assets to ship.

use crate::storage::Database;
use std::process::Command;

pub enum Cue {
    Start,
    Stop,
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
    let path = match which {
        Cue::Start => "/System/Library/Sounds/Pop.aiff",
        Cue::Stop => "/System/Library/Sounds/Bottle.aiff",
    };
    // Fire-and-forget; never block the dictation path on audio playback.
    let _ = Command::new("afplay").arg(path).spawn();
}
