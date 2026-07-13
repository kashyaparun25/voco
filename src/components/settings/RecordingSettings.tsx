import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Button, Toggle } from "../ui";
import { showToast } from "../../hooks/useToast";

const selectStyle: React.CSSProperties = {
  padding: "10px 14px",
  borderRadius: "8px",
  backgroundColor: "var(--color-background-elevated)",
  color: "var(--color-text-primary)",
  border: "1px solid var(--color-border-strong)",
  width: "100%",
  fontSize: "14px",
  cursor: "pointer",
  outline: "none",
};

const labelStyle: React.CSSProperties = {
  fontSize: "13px",
  fontWeight: 600,
  color: "var(--color-text-secondary)",
};

// Transcription languages. English is the default (predictable — no stray-script
// auto-detection); "auto" lets the engine detect; "__custom__" reveals an ISO-code
// field. Applies to Whisper + OpenAI/Groq APIs; the Audio8 model auto-detects.
const STT_LANGS: { value: string; label: string }[] = [
  { value: "en", label: "English (default)" },
  { value: "auto", label: "Auto-detect" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
  { value: "it", label: "Italian" },
  { value: "pt", label: "Portuguese" },
  { value: "hi", label: "Hindi" },
  { value: "zh", label: "Chinese" },
  { value: "ja", label: "Japanese" },
  { value: "ko", label: "Korean" },
  { value: "ru", label: "Russian" },
  { value: "ar", label: "Arabic" },
  { value: "__custom__", label: "Custom (ISO code)…" },
];

export default function RecordingSettings() {
  const [devices, setDevices] = useState<string[]>([]);
  const [micDevice, setMicDevice] = useState<string>("");
  const [saveAudio, setSaveAudio] = useState<boolean>(false);
  const [recordingsDir, setRecordingsDir] = useState<string>("");
  const [sttLang, setSttLang] = useState<string>("en");
  const [showCustomLang, setShowCustomLang] = useState<boolean>(false);

  useEffect(() => {
    (async () => {
      try {
        const list = await invoke<string[]>("list_audio_devices");
        if (Array.isArray(list)) setDevices(list);
      } catch (err) {
        console.warn("RecordingSettings: list_audio_devices failed:", err);
      }
      try {
        const dev = await invoke<string | null>("get_setting", { key: "active_audio_device" });
        if (dev) setMicDevice(dev);
      } catch { /* ignore */ }
      try {
        const sa = await invoke<string | null>("get_setting", { key: "save_audio" });
        if (sa != null) setSaveAudio(sa === "true" || sa === "1");
      } catch { /* ignore */ }
      try {
        const dir = await invoke<string>("get_recordings_dir");
        if (dir) setRecordingsDir(dir);
      } catch { /* ignore */ }
      try {
        const l = await invoke<string | null>("get_setting", { key: "stt_language" });
        const val = l && l.trim() !== "" ? l : "en";
        setSttLang(val);
        setShowCustomLang(!STT_LANGS.some((o) => o.value === val));
      } catch { /* ignore */ }
    })();
  }, []);

  const persist = async (key: string, value: string) => {
    try {
      await invoke("set_setting", { key, value });
    } catch (err) {
      console.warn(`RecordingSettings: failed to save "${key}":`, err);
    }
  };

  return (
    <VStack gap={4} style={{ width: "100%" }}>
      <VStack gap={1}>
        <Text style={{ fontSize: "20px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
          Recordings
        </Text>
        <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
          Choose your input device and control how meeting audio is stored.
        </Text>
      </VStack>

      {/* Default microphone */}
      <VStack gap={2} style={{ maxWidth: 420 }}>
        <Text style={labelStyle}>Default microphone</Text>
        <select
          value={micDevice}
          onChange={(e) => {
            const next = e.target.value;
            setMicDevice(next);
            void invoke("set_audio_device", { device: next }).catch((err) =>
              console.warn("set_audio_device failed:", err)
            );
          }}
          style={selectStyle}
        >
          <option value="">System Default</option>
          {devices.map((d) => (
            <option key={d} value={d}>
              {d}
            </option>
          ))}
        </select>
        <Text style={{ fontSize: 11, color: "var(--color-text-secondary)" }}>
          System audio (other participants) is captured from your default output automatically.
        </Text>
      </VStack>

      {/* Transcription language */}
      <VStack gap={2} style={{ maxWidth: 420 }}>
        <Text style={labelStyle}>Transcription language</Text>
        <select
          value={showCustomLang ? "__custom__" : sttLang}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "__custom__") {
              setShowCustomLang(true);
            } else {
              setShowCustomLang(false);
              setSttLang(v);
              void persist("stt_language", v);
            }
          }}
          style={selectStyle}
        >
          {STT_LANGS.map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
        {showCustomLang && (
          <input
            type="text"
            placeholder="ISO code, e.g. nl, pl, tr"
            value={sttLang === "auto" ? "" : sttLang}
            onChange={(e) => {
              const v = e.target.value.trim().toLowerCase();
              setSttLang(v);
              void persist("stt_language", v);
            }}
            style={{ ...selectStyle, cursor: "text" }}
          />
        )}
        <Text style={{ fontSize: 11, color: "var(--color-text-secondary)" }}>
          Forces the language for Whisper and cloud STT (OpenAI/Groq). Default English avoids
          mixed-script output. The embedded Audio8 model auto-detects and ignores this.
        </Text>
      </VStack>

      {/* Save meeting audio */}
      <HStack style={{ justifyContent: "space-between", alignItems: "center", maxWidth: 480 }}>
        <VStack gap={1} style={{ flex: 1 }}>
          <Text style={{ fontSize: "14px", fontWeight: "600", color: "var(--color-text-primary)" }}>
            Save meeting audio
          </Text>
          <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
            Keep the recording on disk after transcription (enables playback &amp; re-transcription).
          </Text>
        </VStack>
        <Toggle
          checked={saveAudio}
          onChange={() => {
            const next = !saveAudio;
            setSaveAudio(next);
            void persist("save_audio", String(next));
          }}
        />
      </HStack>

      {/* Audio format (informational) */}
      <VStack gap={2} style={{ maxWidth: 420 }}>
        <Text style={labelStyle}>Audio format</Text>
        <div style={{ ...selectStyle, cursor: "default", display: "flex", alignItems: "center", color: "var(--color-text-secondary)" }}>
          WAV · 16-bit PCM · 16 kHz mono
        </div>
      </VStack>

      {/* Storage location */}
      <VStack gap={2} style={{ maxWidth: 560 }}>
        <Text style={labelStyle}>Storage location</Text>
        <HStack gap={2} style={{ alignItems: "center", width: "100%" }}>
          <div
            style={{
              flex: 1,
              padding: "10px 12px",
              borderRadius: 8,
              backgroundColor: "var(--color-background-elevated)",
              border: "1px solid var(--color-border-strong)",
              color: "var(--color-text-secondary)",
              fontSize: 12,
              fontFamily: "monospace",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
            title={recordingsDir}
          >
            {recordingsDir || "…"}
          </div>
          <Button
            variant="secondary"
            size="sm"
            label="Reveal in Finder"
            onClick={() => {
              void invoke("reveal_recordings_dir").catch((err) => {
                console.warn("reveal_recordings_dir failed:", err);
                showToast("Couldn't open the folder", "error");
              });
            }}
          />
        </HStack>
      </VStack>
    </VStack>
  );
}
