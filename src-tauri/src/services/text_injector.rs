//! Insert transcribed text at the cursor position.
//!
//! Insertion strategy (ported from FluidVoice's `TypingService`, which is the
//! most battle-tested implementation of this on macOS):
//!
//! 1. **Clipboard-free unicode typing (primary).** The text is posted as
//!    chunked CGEvent unicode-string keyboard events (`keyboardSetUnicodeString`,
//!    virtual key 0). No clipboard involvement at all — which removes the
//!    entire class of "restore raced the target app's paste read" bugs that
//!    made the old ⌘V path intermittent — and it is what works best in
//!    terminals and Electron apps (VS Code, Discord, Slack).
//! 2. **Clipboard + ⌘V CGEvent (fallback).** Sets the clipboard, posts a real
//!    ⌘V, then restores the old clipboard from a background thread — guarded
//!    by NSPasteboard `changeCount` (never clobbers something the user copied
//!    meanwhile) and only after a generous settle window, not the old fixed
//!    300 ms.
//! 3. **AppleScript ⌘V (last resort).** The original `osascript` path.
//!
//! All CGEvent posting requires the Accessibility permission; we gate on
//! `AXIsProcessTrusted()` up front and return an actionable error so the UI
//! can tell the user instead of failing silently.

use std::process::Command;
use std::io::Write;
use log::{info, warn, error};

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// Max UTF-16 units per unicode keyboard event (FluidVoice uses 200).
const UNICODE_CHUNK: usize = 200;

/// 'V' virtual key code on ANSI layouts (used only by the ⌘V fallbacks).
const KEY_V: CGKeyCode = 0x09;

pub struct TextInjector;

impl TextInjector {
    /// Injects text at the current cursor position.
    pub fn inject(text: &str) -> anyhow::Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }

        if !unsafe { AXIsProcessTrusted() } {
            return Err(anyhow::anyhow!(
                "Voco needs the Accessibility permission to type text. \
                 Enable it in System Settings → Privacy & Security → Accessibility (toggle Voco off and on)."
            ));
        }

        info!("Injecting {} characters of text...", text.len());

        // 1. Clipboard-free unicode typing.
        match Self::inject_unicode_events(text) {
            Ok(()) => {
                info!("Text injected (clipboard-free unicode events).");
                return Ok(());
            }
            Err(e) => warn!("Unicode-event injection failed ({e}); falling back to clipboard paste."),
        }

        // 2. Clipboard + native ⌘V.
        match Self::inject_clipboard_paste(text) {
            Ok(()) => {
                info!("Text injected (clipboard + CGEvent ⌘V).");
                return Ok(());
            }
            Err(e) => warn!("CGEvent clipboard paste failed ({e}); falling back to AppleScript."),
        }

        // 3. Legacy AppleScript path.
        Self::inject_applescript_paste(text)
    }

    /// Type the text directly as unicode keyboard events — no clipboard.
    ///
    /// Chunks are split on UTF-16 boundaries that never separate a surrogate
    /// pair, so emoji and non-BMP characters survive intact.
    fn inject_unicode_events(text: &str) -> anyhow::Result<()> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow::anyhow!("failed to create CGEventSource"))?;

        let utf16: Vec<u16> = text.encode_utf16().collect();
        let mut start = 0usize;
        while start < utf16.len() {
            let mut end = (start + UNICODE_CHUNK).min(utf16.len());
            // Don't split a surrogate pair: if the last unit in the chunk is a
            // high surrogate, leave it for the next chunk.
            if end < utf16.len() && (0xD800..=0xDBFF).contains(&utf16[end - 1]) {
                end -= 1;
            }
            let chunk = &utf16[start..end];

            let down = CGEvent::new_keyboard_event(source.clone(), 0, true)
                .map_err(|_| anyhow::anyhow!("failed to create key-down event"))?;
            down.set_string_from_utf16_unchecked(chunk);
            down.post(CGEventTapLocation::HID);

            let up = CGEvent::new_keyboard_event(source.clone(), 0, false)
                .map_err(|_| anyhow::anyhow!("failed to create key-up event"))?;
            up.post(CGEventTapLocation::HID);

            start = end;
            // Tiny gap between chunks so slow event queues keep ordering.
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Ok(())
    }

    /// Clipboard + native ⌘V fallback with a race-safe restore.
    fn inject_clipboard_paste(text: &str) -> anyhow::Result<()> {
        let original_clipboard = Self::get_clipboard().unwrap_or_default();
        Self::set_clipboard(text)?;
        let our_change_count = Self::pasteboard_change_count();

        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow::anyhow!("failed to create CGEventSource"))?;
        let key_down = CGEvent::new_keyboard_event(source.clone(), KEY_V, true)
            .map_err(|_| anyhow::anyhow!("failed to create key-down CGEvent"))?;
        key_down.set_flags(CGEventFlags::CGEventFlagCommand);
        key_down.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let key_up = CGEvent::new_keyboard_event(source, KEY_V, false)
            .map_err(|_| anyhow::anyhow!("failed to create key-up CGEvent"))?;
        key_up.set_flags(CGEventFlags::CGEventFlagCommand);
        key_up.post(CGEventTapLocation::HID);

        // Restore the user's clipboard from a background thread. Unlike the
        // old fixed 300 ms, wait a generous settle window (slow Electron apps
        // read the pasteboard late) and ONLY restore if the pasteboard still
        // holds our text — if the user copied something meanwhile, leave it.
        if !original_clipboard.is_empty() {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(2_000));
                if Self::pasteboard_change_count() == our_change_count {
                    if let Err(e) = Self::set_clipboard(&original_clipboard) {
                        error!("Failed to restore original clipboard: {:?}", e);
                    }
                } else {
                    info!("Skipped clipboard restore: clipboard changed externally after paste.");
                }
            });
        }
        Ok(())
    }

    /// NSPasteboard.general.changeCount — bumps on every clipboard write.
    fn pasteboard_change_count() -> isize {
        use objc2_app_kit::NSPasteboard;
        unsafe { NSPasteboard::generalPasteboard().changeCount() }
    }

    /// Original AppleScript path, kept as the last resort.
    fn inject_applescript_paste(text: &str) -> anyhow::Result<()> {
        let original_clipboard = Self::get_clipboard().unwrap_or_default();
        Self::set_clipboard(text)?;
        let our_change_count = Self::pasteboard_change_count();

        let script = r#"
            tell application "System Events"
                key code 9 using {command down} -- 9 is the virtual key code for 'V'
            end tell
        "#;
        let status = Command::new("osascript").arg("-e").arg(script).status();
        let ok = matches!(&status, Ok(s) if s.success());
        if ok {
            info!("Text successfully pasted (AppleScript).");
        } else {
            error!("AppleScript paste failed: {:?}", status);
        }

        if !original_clipboard.is_empty() {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(2_000));
                if Self::pasteboard_change_count() == our_change_count {
                    let _ = Self::set_clipboard(&original_clipboard);
                }
            });
        }

        if ok {
            Ok(())
        } else {
            Err(anyhow::anyhow!("AppleScript paste failed: {:?}", status))
        }
    }

    /// Sets the macOS clipboard content using pbcopy
    fn set_clipboard(text: &str) -> anyhow::Result<()> {
        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }

        let status = child.wait()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("pbcopy failed with status: {:?}", status))
        }
    }

    /// Gets the macOS clipboard content using pbpaste
    fn get_clipboard() -> anyhow::Result<String> {
        let output = Command::new("pbpaste")
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(anyhow::anyhow!("pbpaste failed"))
        }
    }
}
