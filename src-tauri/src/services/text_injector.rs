use std::process::Command;
use std::io::Write;
use log::{info, error};

pub struct TextInjector;

impl TextInjector {
    /// Injects text at the current cursor position by setting the clipboard and simulating Command+V.
    ///
    /// The public contract is unchanged: set clipboard -> synthesize Cmd+V -> restore
    /// the original clipboard after a short delay.
    ///
    /// When the `macos-native` feature is enabled the Cmd+V keystroke is
    /// synthesized with Core Graphics `CGEvent`s (fast, no subprocess). When the
    /// feature is off (default), the existing AppleScript / `osascript` path is
    /// used unchanged. If the native path fails at runtime we fall back to the
    /// AppleScript path so a paste is still attempted.
    pub fn inject(text: &str) -> anyhow::Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }

        info!("Injecting {} characters of text...", text.len());

        // 1. Get original clipboard content so we can restore it later (premium feature!)
        let original_clipboard = Self::get_clipboard().unwrap_or_default();

        // 2. Set clipboard content to the text
        if let Err(e) = Self::set_clipboard(text) {
            error!("Failed to set clipboard: {:?}", e);
            return Err(anyhow::anyhow!("Failed to set clipboard: {}", e));
        }

        // 3. Trigger Command+V.
        Self::paste();

        // 4. Restore original clipboard content after a brief delay to allow paste operation to finish
        let original_clone = original_clipboard.clone();
        if !original_clone.is_empty() {
            // Restore on a plain OS thread — this runs from the dictation worker
            // thread, which has no Tokio reactor, so avoid async entirely.
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                if let Err(e) = Self::set_clipboard(&original_clone) {
                    error!("Failed to restore original clipboard: {:?}", e);
                }
            });
        }

        Ok(())
    }

    /// Synthesize a Command+V paste keystroke.
    ///
    /// Uses the native CGEvent path when `macos-native` is enabled, falling back
    /// to AppleScript on any error. When the feature is off, always uses AppleScript.
    fn paste() {
        #[cfg(feature = "macos-native")]
        {
            match Self::paste_native() {
                Ok(()) => {
                    info!("Text successfully pasted (native CGEvent).");
                    return;
                }
                Err(e) => {
                    error!("Native CGEvent paste failed ({:?}); falling back to AppleScript.", e);
                }
            }
        }

        Self::paste_applescript();
    }

    /// Native Cmd+V using Core Graphics events.
    ///
    /// This is the standard, low-latency approach: post a key-down and key-up for
    /// the 'V' key (virtual key code 0x09, the same `key code 9` the AppleScript
    /// path uses) with the Command modifier flag set. Requires Accessibility
    /// permission (the app must be granted control under
    /// System Settings > Privacy & Security > Accessibility); if permission is
    /// missing the events are silently dropped by the OS, so the caller relies on
    /// the AppleScript fallback path for that case.
    #[cfg(feature = "macos-native")]
    fn paste_native() -> anyhow::Result<()> {
        use core_graphics::event::{
            CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode,
        };
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        // 'V' virtual key code (ANSI keyboard); matches AppleScript `key code 9`.
        const KEY_V: CGKeyCode = 0x09;

        // A combined-session-state source lets the injected modifier flags combine
        // correctly with the synthetic key events.
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow::anyhow!("Failed to create CGEventSource (accessibility permission?)"))?;

        // Key down with Command held.
        let key_down = CGEvent::new_keyboard_event(source.clone(), KEY_V, true)
            .map_err(|_| anyhow::anyhow!("Failed to create key-down CGEvent"))?;
        key_down.set_flags(CGEventFlags::CGEventFlagCommand);
        key_down.post(CGEventTapLocation::HID);

        // Key up (also with Command flag so the OS sees a clean release).
        let key_up = CGEvent::new_keyboard_event(source, KEY_V, false)
            .map_err(|_| anyhow::anyhow!("Failed to create key-up CGEvent"))?;
        key_up.set_flags(CGEventFlags::CGEventFlagCommand);
        key_up.post(CGEventTapLocation::HID);

        Ok(())
    }

    /// Existing AppleScript paste path (default behavior).
    fn paste_applescript() {
        let script = r#"
            tell application "System Events"
                key code 9 using {command down} -- 9 is the virtual key code for 'V'
            end tell
        "#;

        let status = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .status();

        match status {
            Ok(s) if s.success() => {
                info!("Text successfully pasted.");
            }
            Ok(s) => {
                error!("AppleScript execution failed with status: {:?}", s);
            }
            Err(e) => {
                error!("Failed to execute AppleScript: {:?}", e);
            }
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
