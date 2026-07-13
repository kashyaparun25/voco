use tauri::State;
use crate::state::AppState;
use std::collections::HashMap;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    let conn = state.db.conn();
    let mut stmt = conn.prepare("SELECT key, value FROM settings").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        let k: String = row.get(0)?;
        let v: String = row.get(1)?;
        Ok((k, v))
    }).map_err(|e| e.to_string())?;

    let mut map = HashMap::new();
    for row in rows {
        let (k, v) = row.map_err(|e| e.to_string())?;
        map.insert(k, v);
    }
    Ok(map)
}

#[tauri::command]
pub fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    state.db.get_setting(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    state.db.set_setting(&key, &value).map_err(|e| e.to_string())
}

/// Persist and immediately apply the dictation hotkey (no restart needed).
/// Accepts a standard combo (e.g. `"CommandOrControl+Shift+Space"`) or a bare
/// modifier token (`"LeftOption"`, `"RightOption"`, `"Fn"`, ...).
#[tauri::command]
pub fn set_dictation_hotkey(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    hotkey: String,
) -> Result<(), String> {
    state
        .db
        .set_setting("dictation_hotkey", &hotkey)
        .map_err(|e| e.to_string())?;
    crate::services::hotkey::apply_hotkey(&app, &hotkey);
    Ok(())
}

/// Whether Voco is trusted for macOS Accessibility (required for bare-modifier hotkeys).
#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    crate::services::hotkey::accessibility_trusted(false)
}

/// Prompt for Accessibility permission (opens the macOS grant dialog / settings pane).
#[tauri::command]
pub fn request_accessibility_permission() -> bool {
    crate::services::hotkey::accessibility_trusted(true)
}

/// Input Monitoring status ("granted" | "denied" | "unknown") — the permission
/// that actually gates the bare-modifier hotkey's keyboard event tap.
#[tauri::command]
pub fn check_input_monitoring_permission() -> String {
    crate::services::hotkey::input_monitoring_status().to_string()
}

/// Prompt for Input Monitoring (adds Voco to the pane).
#[tauri::command]
pub fn request_input_monitoring_permission() -> bool {
    crate::services::hotkey::request_input_monitoring()
}

// ── Screen Recording (needed for meeting system-audio capture) ───────────────
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[tauri::command]
pub fn check_screen_recording_permission() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

#[tauri::command]
pub fn request_screen_recording_permission() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

// ── Microphone ───────────────────────────────────────────────────────────────
extern "C" {
    fn dlopen(filename: *const std::os::raw::c_char, flag: std::os::raw::c_int) -> *mut std::ffi::c_void;
}

/// Microphone authorization: "granted" | "denied" | "restricted" | "notdetermined" | "unknown".
#[tauri::command]
pub fn check_microphone_permission() -> String {
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::msg_send;
    unsafe {
        // Ensure AVFoundation is loaded so the class is registered.
        let _ = dlopen(
            b"/System/Library/Frameworks/AVFoundation.framework/AVFoundation\0".as_ptr() as *const _,
            2,
        );
        let av = match AnyClass::get("AVCaptureDevice") {
            Some(c) => c,
            None => return "unknown".to_string(),
        };
        let ns = match AnyClass::get("NSString") {
            Some(c) => c,
            None => return "unknown".to_string(),
        };
        // AVMediaTypeAudio == @"soun"
        let audio: *mut AnyObject = msg_send![ns, stringWithUTF8String: b"soun\0".as_ptr() as *const std::os::raw::c_char];
        let status: isize = msg_send![av, authorizationStatusForMediaType: audio];
        match status {
            3 => "granted",
            2 => "denied",
            1 => "restricted",
            0 => "notdetermined",
            _ => "unknown",
        }
        .to_string()
    }
}

/// Open the Microphone privacy pane in System Settings (macOS auto-prompts on
/// first capture; this covers the already-denied case).
#[tauri::command]
pub fn request_microphone_permission() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
