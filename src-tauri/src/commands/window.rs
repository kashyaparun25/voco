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
        x: f.origin.x + (f.size.width - PILL_W) / 2.0,
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
        let _ = window.set_size(LogicalSize::new(PILL_W, PILL_H));

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
        let _ = window.set_always_on_top(true);
        log::info!("show_pill: pill shown (appkit={})", positioned);
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
