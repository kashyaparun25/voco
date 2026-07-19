use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition};
use log::warn;
use objc2::encode::{Encode, Encoding};
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};

// AppKit geometry structs (NSPoint/NSSize/NSRect == CG* on 64-bit macOS). We
// implement Encode with the exact Objective-C struct names so msg_send's ABI
// verification passes for by-value struct returns/args.
#[repr(C)]
#[derive(Clone, Copy)]
struct NsPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct NsSize {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct NsRect {
    origin: NsPoint,
    size: NsSize,
}
unsafe impl Encode for NsPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for NsSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for NsRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[NsPoint::ENCODING, NsSize::ENCODING]);
}

/// Points above the screen's bottom edge to keep the pill clear of the Dock.
const PILL_BOTTOM_MARGIN: f64 = 90.0;

/// Position the pill (given its `NSWindow*`) at the bottom-center of the screen
/// the CURSOR is on — done entirely in AppKit's coordinate space (bottom-left,
/// points) so it's immune to Tauri/tao's cross-display DPI coordinate mismatch
/// (which was placing the pill on the primary display). Returns true on success.
///
/// DO NOT replace this with Tauri `cursor_position()` + monitor matching — that
/// path is unreliable on multi-monitor mixed-DPI setups (see git history / the
/// `show_pill` logs). This AppKit path is the canonical fix.
unsafe fn position_pill_appkit(ns_window: *mut AnyObject) -> bool {
    if ns_window.is_null() {
        return false;
    }
    let cursor: NsPoint = msg_send![class!(NSEvent), mouseLocation];
    let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
    if screens.is_null() {
        return false;
    }
    let count: usize = msg_send![screens, count];

    let mut target: Option<NsRect> = None;
    for i in 0..count {
        let scr: *mut AnyObject = msg_send![screens, objectAtIndex: i];
        if scr.is_null() {
            continue;
        }
        let f: NsRect = msg_send![scr, frame];
        if cursor.x >= f.origin.x
            && cursor.x < f.origin.x + f.size.width
            && cursor.y >= f.origin.y
            && cursor.y < f.origin.y + f.size.height
        {
            target = Some(f);
            break;
        }
    }
    let f = match target {
        Some(f) => f,
        None => return false,
    };

    let origin = NsPoint {
        x: f.origin.x + (f.size.width - pill_size().0) / 2.0,
        y: f.origin.y + PILL_BOTTOM_MARGIN,
    };
    let _: () = msg_send![ns_window, setFrameOrigin: origin];
    log::info!(
        "show_pill(appkit): cursor=({:.0},{:.0}) screen=({:.0},{:.0} {:.0}x{:.0}) origin=({:.0},{:.0})",
        cursor.x, cursor.y, f.origin.x, f.origin.y, f.size.width, f.size.height, origin.x, origin.y
    );
    true
}

/// Intended pill size (logical points). Enforced on show so a stale saved
/// window-state geometry can't leave the pill oversized. Kept deliberately
/// compact (~⅓ of the original width) so it doesn't cover the Dock.
const PILL_W: f64 = 120.0;
const PILL_H: f64 = 40.0;
/// Pill used while the live transcript preview is active: compact width,
/// slightly taller — live words on top, waveform underneath.
const PILL_W_EXPANDED: f64 = 200.0;
const PILL_H_EXPANDED: f64 = 58.0;

/// Whether the next `show_pill` should use the expanded (live-preview) size.
/// Set by the dictation service before showing the pill.
static PILL_EXPANDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_pill_expanded(expanded: bool) {
    PILL_EXPANDED.store(expanded, std::sync::atomic::Ordering::Relaxed);
}

fn pill_size() -> (f64, f64) {
    if PILL_EXPANDED.load(std::sync::atomic::Ordering::Relaxed) {
        (PILL_W_EXPANDED, PILL_H_EXPANDED)
    } else {
        (PILL_W, PILL_H)
    }
}

/// Overlay config that makes the pill behave like FluidVoice's HUD panel:
/// visible on every Space and over fullscreen apps, floating at status level,
/// immune to app-hide, and ordered in even though Voco is NOT the active app
/// while the user dictates into Chrome/etc. (plain orderFront can be ignored
/// for inactive apps). Idempotent — safe to apply repeatedly.
///
/// DO NOT reintroduce `window.set_always_on_top(true)` next to this: tao
/// applies its level via dispatch_async, which lands AFTER these synchronous
/// msg_sends and clobbers the status level back to floating.
unsafe fn apply_pill_overlay_config(w: *mut AnyObject) {
    // canJoinAllSpaces (1<<0) | ignoresCycle (1<<6) | fullScreenAuxiliary (1<<8)
    let behavior: usize = (1 << 0) | (1 << 6) | (1 << 8);
    let _: () = msg_send![w, setCollectionBehavior: behavior];
    let _: () = msg_send![w, setLevel: 25isize]; // NSStatusWindowLevel
    let _: () = msg_send![w, setCanHide: objc2::runtime::Bool::NO];
    let _: () = msg_send![w, setHidesOnDeactivate: objc2::runtime::Bool::NO];
    let _: () = msg_send![w, orderFrontRegardless];
}

/// Positions the pill near the bottom-center of the monitor the user is working
/// on (the one containing the cursor) and shows it.
///
/// All the cursor/monitor/positioning work is marshalled to the MAIN THREAD:
/// this is invoked from the hotkey's background thread, and `cursor_position()`
/// returns an error off the main thread on macOS — which made us fall back to
/// the primary monitor, so the pill always landed on the built-in/main screen.
pub fn show_pill(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("pill") else {
        warn!("show_pill_window: pill window does not exist");
        return Ok(());
    };

    let app = app.clone();
    let result = app.clone().run_on_main_thread(move || {
        // Force the intended size (overrides any restored/oversized geometry).
        let (pw, ph) = pill_size();
        let _ = window.set_size(LogicalSize::new(pw, ph));

        // Primary path: AppKit NSScreen placement on the cursor's display.
        let positioned = window
            .ns_window()
            .ok()
            .map(|p| unsafe { position_pill_appkit(p as *mut AnyObject) })
            .unwrap_or(false);

        // Fallback: Tauri monitor matching (only if AppKit path failed).
        if !positioned {
            let cursor = app.cursor_position().ok();
            let monitor = cursor
                .and_then(|c| {
                    window.available_monitors().ok().and_then(|mons| {
                        mons.into_iter().find(|m| {
                            let o = m.position();
                            let s = m.size();
                            c.x >= o.x as f64
                                && c.x < o.x as f64 + s.width as f64
                                && c.y >= o.y as f64
                                && c.y < o.y as f64 + s.height as f64
                        })
                    })
                })
                .or_else(|| window.current_monitor().ok().flatten())
                .or_else(|| window.primary_monitor().ok().flatten());
            if let Some(monitor) = monitor {
                let screen = monitor.size();
                let origin = monitor.position();
                if let Ok(win_size) = window.outer_size() {
                    let x = origin.x + (screen.width as i32 - win_size.width as i32) / 2;
                    let y = origin.y + screen.height as i32 - win_size.height as i32 - 200;
                    let _ = window.set_position(PhysicalPosition::new(x, y));
                    log::info!("show_pill(fallback): pos=({x},{y}) monitor origin ({},{})", origin.x, origin.y);
                }
            }
        }

        let _ = window.show();

        // Make the pill visible on EVERY Space — other desktops AND fullscreen
        // apps (e.g. Chrome in fullscreen). Without canJoinAllSpaces the window
        // belongs to the Space it was created on, so dictating anywhere else
        // showed the pill only on the "home" desktop. Status level keeps it
        // above fullscreen app windows.
        //
        // DO NOT add `window.set_always_on_top(true)` here: tao applies its
        // level change via dispatch_async on the main queue, so it lands AFTER
        // these synchronous msg_sends and clobbers the status level back to
        // floating (observed live: final level was tao's, not ours). The
        // window is created `alwaysOnTop` and we own the level below.
        if let Ok(p) = window.ns_window() {
            let w = p as *mut AnyObject;
            unsafe {
                apply_pill_overlay_config(w);
                let level_now: isize = msg_send![w, level];
                let behavior_now: usize = msg_send![w, collectionBehavior];
                log::info!(
                    "show_pill: pill shown (appkit={positioned}) level={level_now} behavior=0x{behavior_now:x}"
                );
            }
        } else {
            log::warn!("show_pill: ns_window() unavailable; Space/level config skipped");
        }

        // Re-assert after tao's queue drains: tao applies several window ops via
        // dispatch_async on the main queue (observed live with set_always_on_top's
        // level), so config applied synchronously above can be silently reverted
        // a beat later — which left the pill off-screen whenever another app's
        // Space was active. The delayed pass re-applies everything once the
        // queued work has run, and logs ground truth (isVisible/isOnActiveSpace).
        {
            let app2 = app.clone();
            let window2 = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                let _ = app2.run_on_main_thread(move || {
                    if let Ok(p) = window2.ns_window() {
                        let w = p as *mut AnyObject;
                        unsafe {
                            apply_pill_overlay_config(w);
                            let level_now: isize = msg_send![w, level];
                            let behavior_now: usize = msg_send![w, collectionBehavior];
                            let visible: objc2::runtime::Bool = msg_send![w, isVisible];
                            let on_space: objc2::runtime::Bool = msg_send![w, isOnActiveSpace];
                            log::info!(
                                "show_pill(+150ms): level={} behavior=0x{:x} visible={} onActiveSpace={}",
                                level_now, behavior_now, visible.as_bool(), on_space.as_bool()
                            );
                        }
                    }
                });
            });
        }
    });

    if let Err(e) = result {
        warn!("show_pill: run_on_main_thread failed: {}", e);
    }
    Ok(())
}

/// Hides the pill window. Gracefully no-ops if the window is missing.
pub fn hide_pill(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("pill") else {
        warn!("hide_pill_window: pill window does not exist");
        return Ok(());
    };
    window.hide().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verifies the by-value struct-return msg_send ABI (NsPoint/NsRect) against
    // NSEvent/NSScreen. objc2 checks type encodings in debug builds, so a wrong
    // encoding panics here rather than shipping.
    #[test]
    fn appkit_geometry_abi_ok() {
        unsafe {
            let _cursor: NsPoint = msg_send![class!(NSEvent), mouseLocation];
            // mainScreen avoids the array `count`/index ABI (a Swift-bridge
            // artifact in the test binary) while still exercising the NsRect
            // struct-return of `-[NSScreen frame]`.
            let scr: *mut AnyObject = msg_send![class!(NSScreen), mainScreen];
            if !scr.is_null() {
                let _f: NsRect = msg_send![scr, frame];
            }
        }
    }
}

#[tauri::command]
pub fn show_pill_window(app_handle: AppHandle) -> Result<(), String> {
    show_pill(&app_handle)
}

#[tauri::command]
pub fn hide_pill_window(app_handle: AppHandle) -> Result<(), String> {
    hide_pill(&app_handle)
}
