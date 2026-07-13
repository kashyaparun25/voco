//! Pause/resume media around a dictation session — **one-way and state-aware**.
//!
//! Requirements: pause media when dictation starts; resume it when dictation
//! ends; NEVER start media that was already paused.
//!
//! macOS 15.4+ (incl. 26) locks the private MediaRemote framework behind an
//! entitlement in `mediaremoted`, so reading now-playing state directly from the
//! app returns nil. FluidVoice's fix (and now ours): shell out to `/usr/bin/perl`
//! — a system binary (`com.apple.perl`) that IS entitled — and have it load a
//! tiny helper dylib that queries the real play state and sends pause/play. All
//! detection + control happen inside that entitled process, so:
//!   * we pause ONLY when something is actually playing, and
//!   * we resume ONLY the media we paused (never start already-paused media).
//!
//! Ships as bundled resources: `mediaremote-adapter.pl` + `mediaremote-helper.dylib`
//! (built from `resources/mediaremote-helper.m`). See `init_helper`.

use crate::storage::Database;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const PAUSED_KEY: &str = "media_paused_by_voco";

/// Resolved at startup from the app bundle: (adapter.pl, helper.dylib).
static HELPER: OnceLock<Option<(PathBuf, PathBuf)>> = OnceLock::new();

/// Register the bundled MediaRemote helper resource paths (called once at
/// startup). If either is missing, media control degrades to a no-op.
pub fn init_helper(adapter_pl: PathBuf, helper_dylib: PathBuf) {
    let ok = adapter_pl.exists() && helper_dylib.exists();
    if !ok {
        log::warn!(
            "media_control: helper resources missing (pl={:?} exists={}, dylib={:?} exists={}); media pause disabled",
            adapter_pl, adapter_pl.exists(), helper_dylib, helper_dylib.exists()
        );
    }
    let _ = HELPER.set(if ok { Some((adapter_pl, helper_dylib)) } else { None });
}

fn enabled(db: &Database) -> bool {
    db.get_setting("pause_media_on_dictation")
        .ok()
        .flatten()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

/// Run the MediaRemote perl bridge with `func` (`pause_if_playing` | `play` |
/// `get`), returning stdout. Runs via `/usr/bin/perl` so MediaRemote is usable
/// on macOS 15.4+.
fn run_helper(func: &str) -> Option<String> {
    let (pl, dylib) = HELPER.get()?.as_ref()?;
    let out = Command::new("/usr/bin/perl")
        .arg(pl)
        .arg(dylib)
        .arg(func)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        log::warn!(
            "media_control: helper '{}' failed: {}",
            func,
            String::from_utf8_lossy(&out.stderr).trim()
        );
        None
    }
}

/// Pause the now-playing media (browser/Music/Spotify/…) if — and only if — it
/// is actually playing. Detection + pause happen together inside the entitled
/// perl process, so already-paused media is never touched. Records whether we
/// paused so `resume` can put it back without ever starting paused media.
pub fn pause_if_enabled(db: &Database) {
    if !enabled(db) {
        return;
    }
    let paused = run_helper("pause_if_playing")
        .map(|s| s.contains("\"paused\":true"))
        .unwrap_or(false);
    if paused {
        let _ = db.set_setting(PAUSED_KEY, "1");
        log::info!("media: paused now-playing media for dictation");
    } else {
        let _ = db.set_setting(PAUSED_KEY, "");
    }
}

/// Resume media ONLY if we paused it for this dictation.
pub fn resume(db: &Database) {
    let should = db
        .get_setting(PAUSED_KEY)
        .ok()
        .flatten()
        .map(|s| s == "1")
        .unwrap_or(false);
    if !should {
        return;
    }
    let _ = db.set_setting(PAUSED_KEY, "");
    let _ = run_helper("play");
    log::info!("media: resumed media after dictation");
}
