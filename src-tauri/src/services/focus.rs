//! Focused-app targeting for text injection (FluidVoice's
//! `captureSystemFocusedPID` pattern): resolve the PID that owns the
//! system-wide focused accessibility element at dictation START, so the paste
//! can later be posted directly to that process with `CGEvent::post_to_pid` —
//! immune to focus drifting to another app while transcription runs, and more
//! reliable than "frontmost app" for floating launchers / non-activating panels.

use core_foundation::base::{CFGetTypeID, CFRelease, CFTypeID};
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::base::TCFType;
use log::info;
use std::ffi::c_void;

type AXUIElementRef = *const c_void;
type AXError = i32;

const K_AX_ERROR_SUCCESS: AXError = 0;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut *const c_void,
    ) -> AXError;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
    fn AXUIElementGetTypeID() -> CFTypeID;
}

/// Best-effort: the PID of the app the user is dictating into.
///
/// Primary: the owner of the system-wide focused AX element (most precise —
/// survives floating launchers/panels). On macOS 26 that query often returns
/// `kAXErrorNoValue` even while an app clearly has key focus, so fall back to
/// the frontmost application (what FluidVoice's TypingService falls back to as
/// well). Returns `None` when neither works — the injector then uses the HID tap.
pub fn focused_app_pid() -> Option<i32> {
    let own_pid = std::process::id() as i32;
    focused_ax_element_pid()
        .or_else(frontmost_app_pid)
        .filter(|&pid| pid != own_pid)
}

/// The PID owning the currently focused accessibility element, via the
/// system-wide AXUIElement. `None` when Accessibility permission is missing
/// or the system reports no focused element.
fn focused_ax_element_pid() -> Option<i32> {
    unsafe {
        let system_wide = AXUIElementCreateSystemWide();
        if system_wide.is_null() {
            return None;
        }
        let attr = CFString::from_static_string("AXFocusedUIElement");
        let mut value: *const c_void = std::ptr::null();
        let err =
            AXUIElementCopyAttributeValue(system_wide, attr.as_concrete_TypeRef(), &mut value);
        CFRelease(system_wide as *const c_void);
        if err != K_AX_ERROR_SUCCESS || value.is_null() {
            info!("Focus capture: no system-wide focused AX element (err={err}).");
            return None;
        }

        let mut pid: i32 = 0;
        let got_pid = CFGetTypeID(value) == AXUIElementGetTypeID()
            && AXUIElementGetPid(value, &mut pid) == K_AX_ERROR_SUCCESS
            && pid > 0;
        CFRelease(value);

        if got_pid {
            info!("Focus capture: focused AX element owned by PID {pid}.");
            Some(pid)
        } else {
            None
        }
    }
}

/// The frontmost application's PID via NSWorkspace.
fn frontmost_app_pid() -> Option<i32> {
    use objc2_app_kit::NSWorkspace;
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        let pid = app.processIdentifier();
        if pid > 0 {
            info!("Focus capture: using frontmost app PID {pid}.");
            Some(pid)
        } else {
            None
        }
    }
}
