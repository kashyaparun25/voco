import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Toggle } from "../ui";
import EngineSelector from "./EngineSelector";
import GoogleCalendarSettings from "./GoogleCalendarSettings";

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

const subheadStyle: React.CSSProperties = {
  fontSize: "15px",
  fontWeight: 700,
  color: "var(--color-text-primary)",
};

function ToggleRow({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: () => void;
}) {
  return (
    <HStack style={{ justifyContent: "space-between", alignItems: "center", maxWidth: 480 }}>
      <VStack gap={1} style={{ flex: 1 }}>
        <Text style={{ fontSize: "14px", fontWeight: "600", color: "var(--color-text-primary)" }}>
          {label}
        </Text>
        {description ? (
          <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>{description}</Text>
        ) : null}
      </VStack>
      <Toggle checked={checked} onChange={onChange} />
    </HStack>
  );
}

export default function MeetingSettings() {
  const [autoSummarize, setAutoSummarize] = useState<boolean>(true);
  const [autoDetectSpeakers, setAutoDetectSpeakers] = useState<boolean>(true);
  const [maxSpeakers, setMaxSpeakers] = useState<string>("4");
  const [finalizeEngine, setFinalizeEngine] = useState<string>("moss");

  useEffect(() => {
    const load = async () => {
      try {
        const asum = await invoke<string | null>("get_setting", { key: "auto_summarize" });
        if (asum != null) setAutoSummarize(asum === "true");
        const ads = await invoke<string | null>("get_setting", { key: "auto_detect_speakers" });
        if (ads != null) setAutoDetectSpeakers(ads === "true");
        const ms = await invoke<string | null>("get_setting", { key: "max_speakers" });
        if (ms) setMaxSpeakers(ms);
        const fe = await invoke<string | null>("get_setting", { key: "meeting_finalize_engine" });
        if (fe) setFinalizeEngine(fe);
      } catch (err) {
        console.warn("MeetingSettings: failed to load settings:", err);
      }
    };
    void load();
  }, []);

  const persist = async (key: string, value: string) => {
    try {
      await invoke("set_setting", { key, value });
    } catch (err) {
      console.warn(`MeetingSettings: failed to save "${key}":`, err);
    }
  };

  return (
    <VStack gap={4} style={{ width: "100%" }}>
      <VStack gap={1}>
        <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
          Transcription, summarization, and speaker diarization for meeting recordings.
        </Text>
      </VStack>

      {/* Meeting transcription engine */}
      <VStack gap={2} style={{ width: "100%" }}>
        <Text style={subheadStyle}>Transcription (Speech-to-Text)</Text>
        <EngineSelector providerKey="meeting_stt_provider" modelKey="meeting_stt_model" category="stt" />
        <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
          Tip: choosing "moss-transcribe-diarize" here uses MOSS for everything — no live
          captions while recording; the full transcript with speaker labels is generated
          in one pass when the meeting ends.
        </Text>
      </VStack>

      {/* Finalize pass: what runs over the full recording after stop */}
      <VStack gap={2} style={{ maxWidth: 480 }}>
        <Text style={subheadStyle}>Final pass (after recording ends)</Text>
        <select
          value={finalizeEngine}
          onChange={(e) => {
            setFinalizeEngine(e.target.value);
            void persist("meeting_finalize_engine", e.target.value);
          }}
          style={selectStyle}
        >
          <option value="moss">MOSS Transcribe + Diarize — rewrites the transcript with accurate speaker labels (English/Chinese)</option>
          <option value="pyannote">Speaker relabel only (pyannote) — keeps the live transcript text</option>
        </select>
        <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
          MOSS re-transcribes the whole recording in one pass, so text and speaker labels
          come from the same model. Requires the "MOSS Transcribe+Diarize 0.9B" model
          (987 MB) from the model list; if it isn't downloaded, the pass falls back to
          pyannote automatically.
        </Text>
      </VStack>

      {/* Auto-summarize + summary LLM engine */}
      <ToggleRow
        label="Auto-summarize meetings"
        description="Generate an AI summary automatically when a meeting ends."
        checked={autoSummarize}
        onChange={() => {
          const next = !autoSummarize;
          setAutoSummarize(next);
          void persist("auto_summarize", String(next));
        }}
      />

      {autoSummarize && (
        <VStack gap={2} style={{ width: "100%" }}>
          <Text style={subheadStyle}>Summary (Language Model)</Text>
          <EngineSelector providerKey="default_llm_provider" modelKey="summary_llm_model" category="llm" />
        </VStack>
      )}

      <ToggleRow
        label="Auto-detect speakers"
        description="Automatically diarize and separate speakers."
        checked={autoDetectSpeakers}
        onChange={() => {
          const next = !autoDetectSpeakers;
          setAutoDetectSpeakers(next);
          void persist("auto_detect_speakers", String(next));
        }}
      />

      {autoDetectSpeakers && (
        <VStack gap={2} style={{ maxWidth: 200 }}>
          <Text style={labelStyle}>Max Speakers</Text>
          <select
            value={maxSpeakers}
            onChange={(e) => {
              setMaxSpeakers(e.target.value);
              void persist("max_speakers", e.target.value);
            }}
            style={selectStyle}
          >
            {Array.from({ length: 9 }, (_, i) => i + 2).map((n) => (
              <option key={n} value={String(n)}>
                {n}
              </option>
            ))}
          </select>
        </VStack>
      )}

      <div style={{ height: 1, backgroundColor: "var(--color-border)", margin: "8px 0" }} />
      <GoogleCalendarSettings />
    </VStack>
  );
}
