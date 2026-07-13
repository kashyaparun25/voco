//! Configurable dictation hotkey.
//!
//! Two kinds of hotkeys are supported:
//!   * **Standard combos** (e.g. `"CommandOrControl+Shift+Space"`) — registered
//!     through `tauri-plugin-global-shortcut`.
//!   * **Bare modifier keys** (e.g. `"LeftOption"`, `"RightOption"`, `"Fn"`) —
//!     a global shortcut can't represent a lone modifier, so we watch the raw
//!     `flagsChanged` stream with a Core Graphics event tap and fire when the
//!     chosen modifier is pressed. This is what enables "press Left Option to
//!     dictate" (FluidVoice-style).
//!
//! A single persistent event-tap thread runs for the app's lifetime and consults
//! atomics for which modifier (if any) is currently armed, so changing the hotkey
//! at runtime is instant — no restart, no thread churn.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

// Secure Input detection: while any process holds secure keyboard entry
// (Terminal's "Secure Keyboard Entry", password fields, password managers),
// macOS blocks ALL keyboard event taps system-wide and instantly re-disables
// them. This is the classic cause of TapDisabledByUserInput.
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn IsSecureEventInputEnabled() -> bool;
}

/// Whether some process currently holds Secure Input (blocks all key taps).
pub fn secure_input_active() -> bool {
    unsafe { IsSecureEventInputEnabled() }
}

// Input Monitoring (kTCCServiceListenEvent): since macOS Catalina, LISTEN-ONLY
// keyboard event taps require this permission — Accessibility is NOT enough.
// Without it the tap is created successfully but tccd silently stops delivery
// (the exact "few events then dead forever" pattern we observed).
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOHIDCheckAccess(request_type: u32) -> u32;
    fn IOHIDRequestAccess(request_type: u32) -> bool;
}
const IOHID_REQUEST_LISTEN_EVENT: u32 = 1; // kIOHIDRequestTypeListenEvent

/// Input Monitoring status: "granted", "denied", or "unknown" (not yet asked).
pub fn input_monitoring_status() -> &'static str {
    match unsafe { IOHIDCheckAccess(IOHID_REQUEST_LISTEN_EVENT) } {
        0 => "granted",
        1 => "denied",
        _ => "unknown",
    }
}

/// Prompt for Input Monitoring (adds Voco to the pane; user toggles once).
pub fn request_input_monitoring() -> bool {
    unsafe { IOHIDRequestAccess(IOHID_REQUEST_LISTEN_EVENT) }
}

// flagsChanged virtual key codes for modifier keys (layout-independent).
const KC_LEFT_COMMAND: i64 = 0x37;
const KC_RIGHT_COMMAND: i64 = 0x36;
const KC_LEFT_SHIFT: i64 = 0x38;
const KC_RIGHT_SHIFT: i64 = 0x3C;
const KC_LEFT_OPTION: i64 = 0x3A;
const KC_RIGHT_OPTION: i64 = 0x3D;
const KC_LEFT_CONTROL: i64 = 0x3B;
const KC_RIGHT_CONTROL: i64 = 0x3E;
const KC_FN: i64 = 0x3F;

// CGEventFlags bits.
const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_CONTROL: u64 = 0x0004_0000;
const FLAG_ALTERNATE: u64 = 0x0008_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;
const FLAG_FN: u64 = 0x0080_0000;

/// Keycode of the armed bare-modifier hotkey, or `-1` when none is armed.
static ARMED_KEYCODE: AtomicI64 = AtomicI64::new(-1);
/// Flag bit that must be *set* for the armed key to count as "pressed".
static ARMED_FLAG: AtomicU64 = AtomicU64::new(0);
/// When true, require a *double-tap* of the armed key to fire.
static ARMED_DOUBLE: AtomicBool = AtomicBool::new(false);

/// Map a bare-modifier hotkey token to `(keycode, flag_bit)`. Returns `None` for
/// anything that isn't a lone modifier (those go through global-shortcut).
fn bare_modifier(token: &str) -> Option<(i64, u64)> {
    match token.trim() {
        "LeftOption" | "left-option" | "Option" | "Alt" | "⌥" => Some((KC_LEFT_OPTION, FLAG_ALTERNATE)),
        "RightOption" | "right-option" => Some((KC_RIGHT_OPTION, FLAG_ALTERNATE)),
        "LeftControl" | "Control" | "Ctrl" | "⌃" => Some((KC_LEFT_CONTROL, FLAG_CONTROL)),
        "RightControl" | "right-control" => Some((KC_RIGHT_CONTROL, FLAG_CONTROL)),
        "LeftCommand" | "Command" | "Cmd" | "⌘" => Some((KC_LEFT_COMMAND, FLAG_COMMAND)),
        "RightCommand" | "right-command" => Some((KC_RIGHT_COMMAND, FLAG_COMMAND)),
        "LeftShift" | "Shift" | "⇧" => Some((KC_LEFT_SHIFT, FLAG_SHIFT)),
        "RightShift" | "right-shift" => Some((KC_RIGHT_SHIFT, FLAG_SHIFT)),
        "Fn" | "fn" | "Function" | "Globe" => Some((KC_FN, FLAG_FN)),
        _ => None,
    }
}

/// Whether the given spec is a lone modifier key (handled by the event tap).
/// A `double:` prefix (e.g. `double:LeftOption`) means "double-tap".
pub fn is_bare_modifier(spec: &str) -> bool {
    bare_modifier(spec.trim_start_matches("double:")).is_some()
}

/// Whether this process is trusted for Accessibility — required for the keyboard
/// event tap to receive events. If `prompt` is true, macOS shows its
/// "grant Accessibility access" dialog (deep-links to the right settings pane).
pub fn accessibility_trusted(prompt: bool) -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::CFString;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    }

    let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
    let value = CFBoolean::from(prompt);
    let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef()) }
}

fn arm(spec: Option<&str>) {
    let (double, base) = match spec {
        Some(s) if s.starts_with("double:") => (true, Some(&s["double:".len()..])),
        other => (false, other),
    };
    match base.and_then(bare_modifier) {
        Some((kc, flag)) => {
            ARMED_KEYCODE.store(kc, Ordering::SeqCst);
            ARMED_FLAG.store(flag, Ordering::SeqCst);
            ARMED_DOUBLE.store(double, Ordering::SeqCst);
        }
        None => {
            ARMED_KEYCODE.store(-1, Ordering::SeqCst);
            ARMED_FLAG.store(0, Ordering::SeqCst);
            ARMED_DOUBLE.store(false, Ordering::SeqCst);
        }
    }
}

/// Apply a hotkey spec live: (re)configures the event-tap monitor and/or the
/// global-shortcut registration. A bare modifier also keeps `⌘⇧Space` registered
/// as a guaranteed fallback (in case Input Monitoring permission isn't granted).
pub fn apply_hotkey(app: &AppHandle, spec: &str) {
    let _ = app.global_shortcut().unregister_all();

    if is_bare_modifier(spec) {
        arm(Some(spec));
        // Safety-net combo so a keyboard trigger always exists.
        if let Ok(sc) = Shortcut::from_str("CommandOrControl+Shift+Space") {
            let _ = app.global_shortcut().register(sc);
        }
        log::info!("Dictation hotkey set to bare modifier '{}' (fallback ⌘⇧Space active).", spec);
    } else {
        arm(None);
        match Shortcut::from_str(spec) {
            Ok(sc) => {
                if let Err(e) = app.global_shortcut().register(sc) {
                    log::warn!("Failed to register hotkey '{}': {}", spec, e);
                }
            }
            Err(e) => log::warn!("Invalid hotkey spec '{}': {}", spec, e),
        }
    }
}

// Debounce/double-tap timing (millis since Unix epoch; atomics so both the
// global and local NSEvent monitors share one state).
static LAST_FIRE_MS: AtomicU64 = AtomicU64::new(0);
static LAST_PRESS_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Shared press-detection logic for both NSEvent monitors. `flags` uses the
/// device-independent modifier bits (NSEventModifierFlags == CGEventFlags).
fn handle_flags_event(kc: i64, flags: u64, on_trigger: &std::sync::Arc<dyn Fn() + Send + Sync>) {
    let armed = ARMED_KEYCODE.load(Ordering::SeqCst);
    if armed < 0 || kc != armed {
        return;
    }
    let flag_bit = ARMED_FLAG.load(Ordering::SeqCst);
    // flagsChanged fires on press AND release; the modifier bit is set only
    // while the key is down → this is a press.
    if flags & flag_bit == 0 {
        return;
    }
    let now = now_ms();
    let fire = if ARMED_DOUBLE.load(Ordering::SeqCst) {
        let last = LAST_PRESS_MS.swap(now, Ordering::SeqCst);
        now.saturating_sub(last) < 350
    } else {
        true
    };
    if fire && now.saturating_sub(LAST_FIRE_MS.load(Ordering::SeqCst)) > 400 {
        LAST_FIRE_MS.store(now, Ordering::SeqCst);
        log::info!("hotkey: FIRING (keycode {})", kc);
        // on_trigger may be slow (starts capture, shows pill) — never block the
        // main thread's event delivery.
        let cb = on_trigger.clone();
        std::thread::spawn(move || cb());
    }
}

/// Install AppKit `NSEvent` monitors for the bare-modifier dictation hotkey.
/// MUST be called on the main thread (AppKit).
///
/// This replaces the previous CGEventTap: macOS repeatedly force-disabled the
/// tap (observed as `TapDisabledByUserInput` + `watchdog: tap enabled=false`
/// cycles), losing key presses. NSEvent monitors are AppKit-managed — no
/// enable/disable war — and are what production dictation apps use.
///
/// Two monitors are needed for full coverage:
///   * global — key events destined for OTHER applications
///   * local  — key events while Voco itself is frontmost (global monitors
///     never see your own app's events)
pub fn install_nsevent_monitors<F>(on_trigger: F)
where
    F: Fn() + Send + Sync + 'static,
{
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask};
    use std::ptr::NonNull;

    log::info!("Hotkey: Accessibility trusted = {}", accessibility_trusted(false));
    let im = input_monitoring_status();
    log::info!("Hotkey: Input Monitoring = {}", im);
    if im != "granted" {
        log::warn!("Hotkey: requesting Input Monitoring access…");
        let _ = request_input_monitoring();
    }

    let on_trigger: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(on_trigger);

    // Global monitor — events going to other apps.
    let g_cb = on_trigger.clone();
    let global_block = RcBlock::new(move |ev: NonNull<NSEvent>| {
        let (kc, flags) = unsafe {
            (ev.as_ref().keyCode() as i64, ev.as_ref().modifierFlags().0 as u64)
        };
        handle_flags_event(kc, flags, &g_cb);
    });
    let global = unsafe {
        NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::FlagsChanged,
            &global_block,
        )
    };

    // Local monitor — events while Voco is frontmost. Must return the event
    // pointer unchanged so normal in-app processing continues.
    let l_cb = on_trigger.clone();
    let local_block = RcBlock::new(move |ev: NonNull<NSEvent>| -> *mut NSEvent {
        let (kc, flags) = unsafe {
            (ev.as_ref().keyCode() as i64, ev.as_ref().modifierFlags().0 as u64)
        };
        handle_flags_event(kc, flags, &l_cb);
        ev.as_ptr()
    });
    let local = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::FlagsChanged,
            &local_block,
        )
    };

    log::info!(
        "Hotkey: NSEvent monitors installed (global={}, local={}).",
        global.is_some(),
        local.is_some()
    );
    // Monitors and their blocks must live for the app's lifetime.
    std::mem::forget(global);
    std::mem::forget(local);
    std::mem::forget(global_block);
    std::mem::forget(local_block);
}
