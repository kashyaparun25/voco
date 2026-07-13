import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Button } from "../ui";

interface GettingStartedProps {
  onOpenSettings: (section: string) => void;
}

type StepStatus = "done" | "todo" | "unknown";

interface StepView {
  key: string;
  title: string;
  desc: string;
  status: StepStatus;
  action?: { label: string; run: () => void };
}

const HOTKEY_LABELS: Record<string, string> = {
  LeftOption: "Left Option (⌥)",
  RightOption: "Right Option (⌥)",
  "double:LeftOption": "Double-tap Left Option",
  Fn: "Fn / Globe",
  LeftControl: "Left Control (⌃)",
  "CommandOrControl+Shift+Space": "⌘⇧Space",
};

export default function GettingStartedPage({ onOpenSettings }: GettingStartedProps) {
  const [accessibility, setAccessibility] = useState<StepStatus>("unknown");
  const [inputMon, setInputMon] = useState<StepStatus>("unknown");
  const [mic, setMic] = useState<StepStatus>("unknown");
  const [screen, setScreen] = useState<StepStatus>("unknown");
  const [modelReady, setModelReady] = useState<StepStatus>("unknown");
  const [hotkey, setHotkey] = useState<string>("");

  const load = useCallback(async () => {
    const call = async <T,>(cmd: string, def: T): Promise<T> => {
      try { return await invoke<T>(cmd); } catch { return def; }
    };
    const acc = await call<boolean>("check_accessibility_permission", false);
    setAccessibility(acc ? "done" : "todo");
    const im = await call<string>("check_input_monitoring_permission", "unknown");
    setInputMon(im === "granted" ? "done" : "todo");
    const m = await call<string>("check_microphone_permission", "unknown");
    setMic(m === "granted" ? "done" : "todo");
    const sr = await call<boolean>("check_screen_recording_permission", false);
    setScreen(sr ? "done" : "todo");

    // Model: ready if a cloud provider is set, or an embedded STT model is downloaded.
    try {
      const provider = await invoke<string | null>("get_setting", { key: "default_stt_provider" });
      let ready = !!provider && provider !== "embedded";
      if (!ready) {
        const models = await invoke<any[]>("list_models");
        ready = Array.isArray(models) && models.some((x) => x.category === "stt" && x.is_downloaded);
      }
      setModelReady(ready ? "done" : "todo");
    } catch { setModelReady("unknown"); }

    try {
      const hk = await invoke<string | null>("get_setting", { key: "dictation_hotkey" });
      setHotkey(hk || "LeftOption");
    } catch { setHotkey("LeftOption"); }
  }, []);

  useEffect(() => {
    void load();
    const onFocus = () => void load();
    window.addEventListener("focus", onFocus);
    const timer = window.setInterval(() => void load(), 4000);
    return () => { window.removeEventListener("focus", onFocus); window.clearInterval(timer); };
  }, [load]);

  const steps: StepView[] = [
    {
      key: "accessibility",
      title: "Accessibility",
      desc: "Lets Voco paste transcriptions at your cursor and use the dictation hotkey.",
      status: accessibility,
      action: { label: "Grant", run: () => void invoke("request_accessibility_permission").then(() => setTimeout(load, 800)) },
    },
    {
      key: "input",
      title: "Input Monitoring",
      desc: "Required for the bare-modifier dictation hotkey (e.g. Left Option) to fire.",
      status: inputMon,
      action: { label: "Grant", run: () => void invoke("request_input_monitoring_permission").then(() => setTimeout(load, 800)) },
    },
    {
      key: "mic",
      title: "Microphone",
      desc: "Needed to record your voice for dictation and meetings.",
      status: mic,
      action: { label: "Open Settings", run: () => void invoke("request_microphone_permission") },
    },
    {
      key: "screen",
      title: "Screen Recording",
      desc: "Only for meetings — captures the audio of other participants (system audio).",
      status: screen,
      action: { label: "Grant", run: () => void invoke("request_screen_recording_permission").then(() => setTimeout(load, 800)) },
    },
    {
      key: "model",
      title: "Set up a transcription model",
      desc: "Download a local model, or connect a cloud provider (OpenAI, Groq…) under AI Providers & Models.",
      status: modelReady,
      action: { label: "Set up", run: () => onOpenSettings("ai") },
    },
    {
      key: "hotkey",
      title: "Dictation hotkey",
      desc: `Currently: ${HOTKEY_LABELS[hotkey] || hotkey}. Press it anywhere to start dictating.`,
      status: hotkey ? "done" : "unknown",
      action: { label: "Change", run: () => onOpenSettings("hotkeys") },
    },
  ];

  const doneCount = steps.filter((s) => s.status === "done").length;
  const allDone = steps.every((s) => s.status === "done");

  return (
    <VStack gap={4} style={{ padding: 24, height: "100%", overflowY: "auto" }}>
      <VStack gap={2}>
        <Text style={{ fontSize: "28px", fontWeight: "bold", color: "var(--color-text-primary)" }}>Getting Started</Text>
        <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
          A quick setup so dictation and meetings work smoothly. {doneCount}/{steps.length} complete.
        </Text>
      </VStack>

      {allDone && (
        <div style={{ padding: "14px 18px", borderRadius: 12, backgroundColor: "rgba(16,185,129,0.12)", border: "1px solid #10b981", color: "#10b981", fontSize: 14, fontWeight: 600 }}>
          🎉 You're all set — press your hotkey to start dictating.
        </div>
      )}

      <VStack gap={3}>
        {steps.map((s, i) => (
          <div key={s.key} style={{ display: "flex", alignItems: "center", gap: 14, padding: 16, borderRadius: 12, backgroundColor: "var(--color-background-surface)", border: `1px solid ${s.status === "done" ? "rgba(16,185,129,0.4)" : "var(--color-border)"}` }}>
            <div style={{
              width: 28, height: 28, borderRadius: "50%", flexShrink: 0,
              display: "flex", alignItems: "center", justifyContent: "center",
              backgroundColor: s.status === "done" ? "#10b981" : "var(--color-background-surface-hover)",
              color: s.status === "done" ? "#fff" : "var(--color-text-secondary)",
              border: s.status === "done" ? "none" : "1px solid var(--color-border-strong)",
              fontSize: 13, fontWeight: 700,
            }}>
              {s.status === "done" ? "✓" : i + 1}
            </div>
            <VStack gap={0} style={{ flex: 1 }}>
              <Text style={{ fontSize: 15, fontWeight: 700, color: "var(--color-text-primary)" }}>{s.title}</Text>
              <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>{s.desc}</Text>
            </VStack>
            {s.status !== "done" && s.action && (
              <Button variant={s.status === "todo" ? "primary" : "secondary"} size="sm" label={s.action.label} onClick={s.action.run} />
            )}
          </div>
        ))}
      </VStack>

      <Text style={{ fontSize: 11, color: "var(--color-text-secondary)", textAlign: "center", paddingTop: 4 }}>
        After granting a permission in System Settings, this page updates automatically. Some permissions require quitting and reopening Voco to take effect.
      </Text>
    </VStack>
  );
}
