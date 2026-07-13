import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Button } from "../ui";
import { showToast } from "../../hooks/useToast";

// Dictation hotkey presets. The `spec` is what the backend understands:
// - bare-modifier tokens (LeftOption, …) drive the Core Graphics event-tap monitor
//   and need Accessibility (tap/hold).
// - `double:` prefixed specs double-tap a modifier (also event-tap → Accessibility).
// - combos (CommandOrControl+Shift+Space) and single regular keys (F5, F6) go
//   through the global-shortcut plugin and need no permission.
const DICTATION_PRESETS: Array<{ spec: string; label: string; hint?: string }> = [
  { spec: "LeftOption", label: "Left Option ⌥", hint: "press ⌥" },
  { spec: "double:LeftOption", label: "Double-tap ⌥", hint: "tap ⌥ twice" },
  { spec: "RightOption", label: "Right Option ⌥", hint: "press ⌥" },
  { spec: "Fn", label: "Fn / Globe 🌐", hint: "press fn" },
  { spec: "double:Fn", label: "Double-tap Fn 🌐", hint: "tap fn twice" },
  { spec: "LeftControl", label: "Left Control ⌃", hint: "press ⌃" },
  { spec: "CommandOrControl+Shift+Space", label: "⌘ ⇧ Space" },
  { spec: "Alt+Space", label: "⌥ Space" },
  { spec: "F5", label: "F5", hint: "no permission" },
  { spec: "F6", label: "F6", hint: "no permission" },
];

// A spec needs macOS Accessibility permission when it drives the event-tap
// monitor: bare-modifier tokens and `double:` double-taps. Combos (contain "+")
// and single regular keys (F5/F6) go through the global-shortcut plugin and
// need no permission.
function specNeedsAccessibility(spec: string): boolean {
  if (spec.startsWith("double:")) return true;
  if (spec.includes("+")) return false;
  // Single regular keys (function keys, letters, etc.) use global-shortcut.
  return /^(Left|Right)?(Option|Control|Command|Shift|Alt|Meta|Fn)$/.test(spec);
}

const MODIFIER_KEYS = new Set(["Meta", "Control", "Alt", "Shift", "OS", "ContextMenu"]);

function formatMeetingKeyLabel(e: KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  const parts: string[] = [];
  if (e.metaKey) parts.push("⌘");
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("⌥");
  if (e.shiftKey) parts.push("Shift");
  let main = e.key;
  if (main === " ") main = "Space";
  else if (main.length === 1) main = main.toUpperCase();
  parts.push(main);
  return parts.join("+");
}

export default function HotkeySettings() {
  const [dictationSpec, setDictationSpec] = useState<string>("LeftOption");
  const [meetingHotkey, setMeetingHotkey] = useState<string>("⌘+Shift+M");
  const [capturingMeeting, setCapturingMeeting] = useState(false);
  const [axTrusted, setAxTrusted] = useState<boolean | null>(null);
  // Input Monitoring is the permission that actually gates the key monitor.
  const [inputMon, setInputMon] = useState<string | null>(null);

  const refreshAx = useCallback(async () => {
    try {
      const ok = await invoke<boolean>("check_accessibility_permission");
      setAxTrusted(ok);
    } catch {
      setAxTrusted(null);
    }
    try {
      const im = await invoke<string>("check_input_monitoring_permission");
      setInputMon(im);
    } catch {
      setInputMon(null);
    }
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const d = await invoke<string | null>("get_setting", { key: "dictation_hotkey" });
        const m = await invoke<string | null>("get_setting", { key: "meeting_hotkey" });
        if (d) setDictationSpec(d);
        if (m) setMeetingHotkey(m);
      } catch (err) {
        console.warn("HotkeySettings: load failed", err);
      }
    })();
    void refreshAx();
  }, [refreshAx]);

  const requestAx = useCallback(async () => {
    try {
      // Input Monitoring is what gates the key monitor; request it first.
      await invoke("request_input_monitoring_permission");
      await invoke("request_accessibility_permission");
      showToast("Enable Voco under Input Monitoring (and Accessibility), then restart Voco.", "info");
      setTimeout(() => void refreshAx(), 1500);
    } catch (err) {
      console.warn("permission request failed", err);
    }
  }, [refreshAx]);

  const dictationNeedsAx = specNeedsAccessibility(dictationSpec);

  // Apply the dictation hotkey live (no restart) via the dedicated command.
  const selectDictation = useCallback(async (spec: string) => {
    setDictationSpec(spec);
    try {
      await invoke("set_dictation_hotkey", { hotkey: spec });
      const needsAx = specNeedsAccessibility(spec);
      showToast(
        needsAx
          ? "Dictation hotkey set. This trigger needs macOS Accessibility permission (grant below, then restart)."
          : "Dictation hotkey updated.",
        "success"
      );
      if (needsAx) void refreshAx();
    } catch (err) {
      console.warn("set_dictation_hotkey failed", err);
      showToast("Couldn't update hotkey", "error");
    }
  }, [refreshAx]);

  useEffect(() => {
    if (!capturingMeeting) return;
    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") { setCapturingMeeting(false); return; }
      const label = formatMeetingKeyLabel(e);
      if (label) {
        setMeetingHotkey(label);
        invoke("set_setting", { key: "meeting_hotkey", value: label }).catch(() => {});
        setCapturingMeeting(false);
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [capturingMeeting]);

  return (
    <VStack gap={4} style={{ width: "100%" }}>
      <VStack gap={1}>
        <Text style={{ fontSize: "20px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
          Hotkeys
        </Text>
        <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
          Choose how you trigger dictation. Changes apply instantly.
        </Text>
      </VStack>

      {/* Dictation hotkey presets */}
      <VStack gap={2} style={{ maxWidth: 560 }}>
        <Text style={{ fontSize: "14px", fontWeight: "600", color: "var(--color-text-primary)" }}>
          Dictation trigger
        </Text>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
          {DICTATION_PRESETS.map((p) => {
            const active = dictationSpec === p.spec;
            return (
              <button
                key={p.spec}
                onClick={() => selectDictation(p.spec)}
                style={{
                  padding: "8px 14px",
                  borderRadius: 999,
                  cursor: "pointer",
                  fontSize: 13,
                  fontWeight: 600,
                  backgroundColor: active ? "var(--color-accent)" : "var(--color-background-elevated)",
                  color: active ? "#ffffff" : "var(--color-text-primary)",
                  border: `1px solid ${active ? "var(--color-accent)" : "var(--color-border-strong)"}`,
                  transition: "all 0.15s ease",
                }}
              >
                {p.label}
              </button>
            );
          })}
        </div>
        {dictationNeedsAx && (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 12,
              padding: "10px 14px",
              borderRadius: 10,
              backgroundColor: axTrusted && inputMon === "granted"
                ? "color-mix(in srgb, var(--color-speaker-2) 12%, transparent)"
                : "color-mix(in srgb, var(--color-recording) 12%, transparent)",
              border: `1px solid ${axTrusted && inputMon === "granted" ? "var(--color-speaker-2)" : "var(--color-recording)"}`,
            }}
          >
            <Text style={{ fontSize: 12, color: "var(--color-text-primary)" }}>
              {axTrusted === null || inputMon === null
                ? "Checking permissions…"
                : axTrusted && inputMon === "granted"
                ? "✅ Input Monitoring + Accessibility granted — this trigger is active."
                : inputMon !== "granted"
                ? "⚠︎ This trigger needs Input Monitoring (System Settings → Privacy & Security → Input Monitoring). Enable Voco there, then restart Voco."
                : "⚠︎ This trigger also needs Accessibility. Enable Voco there, then restart Voco."}
            </Text>
            {!(axTrusted && inputMon === "granted") && (
              <Button
                variant="secondary"
                label="Grant permissions"
                onClick={requestAx}
                style={{ cursor: "pointer", whiteSpace: "nowrap" }}
              />
            )}
          </div>
        )}
        <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
          Bare-modifier and double-tap triggers (⌥, Fn, Double-tap ⌥…) require macOS{" "}
          <strong>Input Monitoring</strong> (to see the key press) and <strong>Accessibility</strong>{" "}
          (to paste) — System Settings → Privacy &amp; Security. Combos like <strong>⌘⇧Space</strong>,{" "}
          <strong>F5 / F6</strong>, and the menu-bar icon always work, no permission needed.
        </Text>
      </VStack>

      {/* Meeting hotkey */}
      <VStack gap={2} style={{ maxWidth: 560 }}>
        <Text style={{ fontSize: "14px", fontWeight: "600", color: "var(--color-text-primary)" }}>
          Meeting trigger
        </Text>
        <HStack gap={3} style={{ alignItems: "center" }}>
          <div
            style={{
              padding: "6px 14px",
              borderRadius: 8,
              minWidth: 140,
              textAlign: "center",
              fontSize: 13,
              fontWeight: 600,
              fontFamily: "monospace",
              backgroundColor: "var(--color-background-elevated)",
              border: `1px solid ${capturingMeeting ? "var(--color-accent)" : "var(--color-border-strong)"}`,
              color: capturingMeeting ? "var(--color-accent)" : "var(--color-text-primary)",
            }}
          >
            {capturingMeeting ? "Press keys…" : meetingHotkey}
          </div>
          <Button
            variant="secondary"
            label={capturingMeeting ? "Cancel" : "Change"}
            onClick={() => setCapturingMeeting((v) => !v)}
            style={{ cursor: "pointer" }}
          />
        </HStack>
      </VStack>
    </VStack>
  );
}
