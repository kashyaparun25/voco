import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Toggle } from "../ui";
import EngineSelector from "./EngineSelector";
import PerAppPrompts from "./PerAppPrompts";

const DICTATION_MODES = [
  { value: "PushToTalk", label: "Push to Talk" },
  { value: "Toggle", label: "Toggle" },
  { value: "AutoStop", label: "Auto Stop" },
];

const PILL_POSITIONS = ["Top", "Bottom", "TopLeft", "TopRight", "BottomLeft", "BottomRight"];

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
    <HStack style={{ justifyContent: "space-between", alignItems: "center", maxWidth: 520 }}>
      <VStack gap={1} style={{ flex: 1 }}>
        <Text style={{ fontSize: "14px", fontWeight: "600", color: "var(--color-text-primary)" }}>{label}</Text>
        {description ? (
          <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>{description}</Text>
        ) : null}
      </VStack>
      <Toggle checked={checked} onChange={onChange} />
    </HStack>
  );
}

export default function DictationSettings() {
  const [dictationMode, setDictationMode] = useState<string>("Toggle");
  const [pillPosition, setPillPosition] = useState<string>("Bottom");
  const [autoPaste, setAutoPaste] = useState<boolean>(true);
  const [autoCapitalize, setAutoCapitalize] = useState<boolean>(true);
  const [autoPunctuation, setAutoPunctuation] = useState<boolean>(true);
  const [pauseMedia, setPauseMedia] = useState<boolean>(false);
  const [aiEnhance, setAiEnhance] = useState<boolean>(false);
  const [aiPrompt, setAiPrompt] = useState<string>("");
  const [removeFillers, setRemoveFillers] = useState<boolean>(true);
  const [soundFeedback, setSoundFeedback] = useState<boolean>(false);
  const [cueStyle, setCueStyle] = useState<string>("deep_tap");
  const [cueStyles, setCueStyles] = useState<Array<[string, string]>>([["deep_tap", "Deep tap"]]);
  const [appProfiles, setAppProfiles] = useState<boolean>(true);

  useEffect(() => {
    const load = async () => {
      const get = async (k: string) => {
        try {
          return await invoke<string | null>("get_setting", { key: k });
        } catch {
          return null;
        }
      };
      const mode = await get("dictation_mode");
      if (mode) setDictationMode(mode);
      const pos = await get("pill_position");
      if (pos) setPillPosition(pos);
      const ap = await get("auto_paste");
      if (ap != null) setAutoPaste(ap === "true");
      const cap = await get("auto_capitalize");
      if (cap != null) setAutoCapitalize(cap === "true");
      const punc = await get("auto_punctuation");
      if (punc != null) setAutoPunctuation(punc === "true");
      const pm = await get("pause_media_on_dictation");
      if (pm != null) setPauseMedia(pm === "true");
      const ai = await get("dictation_ai_enhance");
      if (ai != null) setAiEnhance(ai === "true");
      const prompt = await get("dictation_ai_prompt");
      if (prompt) setAiPrompt(prompt);
      const rf = await get("remove_fillers");
      if (rf != null) setRemoveFillers(rf === "true");
      const sf = await get("sound_feedback");
      if (sf != null) setSoundFeedback(sf === "true");
      const cs = await get("sound_cue_style");
      if (cs) setCueStyle(cs);
      try {
        const styles = await invoke<Array<[string, string]>>("list_sound_cue_styles");
        if (styles?.length) setCueStyles(styles);
      } catch {
        /* backend unavailable */
      }
      const uap = await get("use_app_profiles");
      if (uap != null) setAppProfiles(uap !== "false");
    };
    void load();
  }, []);

  const persist = async (key: string, value: string) => {
    try {
      await invoke("set_setting", { key, value });
    } catch (err) {
      console.warn(`DictationSettings: failed to save "${key}":`, err);
    }
  };

  const toggle = (key: string, value: boolean, setter: (v: boolean) => void) => {
    const next = !value;
    setter(next);
    void persist(key, String(next));
  };

  return (
    <VStack gap={5} style={{ width: "100%" }}>
      <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
        Configure how real-time dictation captures, transcribes, and formats your voice.
      </Text>

      {/* Speech-to-Text engine */}
      <VStack gap={2} style={{ width: "100%" }}>
        <Text style={subheadStyle}>Speech-to-Text Engine</Text>
        <EngineSelector providerKey="default_stt_provider" modelKey="dictation_stt_model" category="stt" />
      </VStack>

      {/* Capture behavior */}
      <VStack gap={3} style={{ width: "100%" }}>
        <Text style={subheadStyle}>Capture</Text>
        <VStack gap={2}>
          <Text style={labelStyle}>Dictation Mode</Text>
          <HStack gap={0} style={{ border: "1px solid var(--color-border-strong)", borderRadius: 8, overflow: "hidden", width: "fit-content" }}>
            {DICTATION_MODES.map((m) => {
              const active = dictationMode === m.value;
              return (
                <button
                  key={m.value}
                  onClick={() => {
                    setDictationMode(m.value);
                    void persist("dictation_mode", m.value);
                  }}
                  style={{
                    padding: "8px 16px",
                    border: "none",
                    cursor: "pointer",
                    fontSize: 13,
                    fontWeight: 600,
                    backgroundColor: active ? "var(--color-accent)" : "var(--color-background-elevated)",
                    color: active ? "#ffffff" : "var(--color-text-secondary)",
                    transition: "background-color 0.15s ease",
                  }}
                >
                  {m.label}
                </button>
              );
            })}
          </HStack>
        </VStack>

        <VStack gap={2} style={{ maxWidth: 300 }}>
          <Text style={labelStyle}>Dictation Pill Position</Text>
          <select
            value={pillPosition}
            onChange={(e) => {
              setPillPosition(e.target.value);
              void persist("pill_position", e.target.value);
            }}
            style={selectStyle}
          >
            {PILL_POSITIONS.map((pos) => (
              <option key={pos} value={pos}>
                {pos}
              </option>
            ))}
          </select>
        </VStack>

        <ToggleRow
          label="Auto-paste transcription"
          description="Paste the transcribed text at the cursor when dictation ends."
          checked={autoPaste}
          onChange={() => toggle("auto_paste", autoPaste, setAutoPaste)}
        />

        <ToggleRow
          label="Pause media while dictating"
          description="Pause Apple Music / Spotify while recording, and resume afterwards."
          checked={pauseMedia}
          onChange={() => toggle("pause_media_on_dictation", pauseMedia, setPauseMedia)}
        />

        <ToggleRow
          label="Sound feedback"
          description="Play a short cue when dictation starts and stops."
          checked={soundFeedback}
          onChange={() => toggle("sound_feedback", soundFeedback, setSoundFeedback)}
        />

        {soundFeedback && (
          <div style={{ display: "flex", alignItems: "center", gap: 10, maxWidth: 480 }}>
            <select
              value={cueStyle}
              onChange={(e) => {
                setCueStyle(e.target.value);
                void persist("sound_cue_style", e.target.value);
                void invoke("preview_sound_cue", { style: e.target.value });
              }}
              style={{
                flex: 1,
                padding: "8px 12px",
                borderRadius: 8,
                backgroundColor: "var(--color-background-elevated)",
                color: "var(--color-text-primary)",
                border: "1px solid var(--color-border-strong)",
                fontSize: 13,
                cursor: "pointer",
                outline: "none",
              }}
            >
              {cueStyles.map(([id, name]) => (
                <option key={id} value={id}>
                  {name}
                </option>
              ))}
            </select>
            <button
              onClick={() => void invoke("preview_sound_cue", { style: cueStyle })}
              title="Preview start and stop cues"
              style={{
                padding: "8px 14px",
                borderRadius: 8,
                background: "transparent",
                color: "var(--color-text-secondary)",
                border: "1px solid var(--color-border-strong)",
                fontSize: 13,
                cursor: "pointer",
              }}
            >
              ▶ Preview
            </button>
          </div>
        )}
      </VStack>

      {/* Text & formatting */}
      <VStack gap={3} style={{ width: "100%" }}>
        <Text style={subheadStyle}>Text &amp; Formatting</Text>

        <ToggleRow
          label="Auto-capitalization"
          description="Capitalize sentence starts and the standalone word “i”."
          checked={autoCapitalize}
          onChange={() => toggle("auto_capitalize", autoCapitalize, setAutoCapitalize)}
        />

        <ToggleRow
          label="Auto-punctuation"
          description="Tidy spacing and ensure the text ends with punctuation."
          checked={autoPunctuation}
          onChange={() => toggle("auto_punctuation", autoPunctuation, setAutoPunctuation)}
        />

        <ToggleRow
          label="Remove filler words"
          description="Strip “um”, “uh”, “er”, “hmm” and similar from transcriptions."
          checked={removeFillers}
          onChange={() => toggle("remove_fillers", removeFillers, setRemoveFillers)}
        />

        <ToggleRow
          label="AI enhancement"
          description="Clean up grammar, punctuation & formatting with your Summary LLM provider. Adds a short delay per dictation."
          checked={aiEnhance}
          onChange={() => toggle("dictation_ai_enhance", aiEnhance, setAiEnhance)}
        />

        {aiEnhance && (
          <VStack gap={1} style={{ maxWidth: 620 }}>
            <Text style={labelStyle}>AI enhancement prompt (optional)</Text>
            <textarea
              value={aiPrompt}
              placeholder="Leave blank to use the default cleanup prompt (fix punctuation, capitalization & spelling without changing meaning)."
              onChange={(e) => setAiPrompt(e.target.value)}
              onBlur={() => void persist("dictation_ai_prompt", aiPrompt)}
              rows={3}
              style={{
                padding: "10px 12px",
                borderRadius: 8,
                backgroundColor: "var(--color-background-elevated)",
                color: "var(--color-text-primary)",
                border: "1px solid var(--color-border-strong)",
                fontSize: 13,
                width: "100%",
                outline: "none",
                resize: "vertical",
                fontFamily: "inherit",
                boxSizing: "border-box",
              }}
            />
            <Text style={{ fontSize: 11, color: "var(--color-text-secondary)" }}>
              Uses the provider selected under Meetings → Summary (Language Model).
            </Text>
          </VStack>
        )}

        {aiEnhance && (
          <ToggleRow
            label="Adapt to the current app"
            description="Auto-format based on where you're dictating — code-aware for Cursor / Claude / VS Code (incl. “at file foo dot ts” → @foo.ts), command-style for terminals, concise for Slack, prose for Mail."
            checked={appProfiles}
            onChange={() => toggle("use_app_profiles", appProfiles, setAppProfiles)}
          />
        )}

        {aiEnhance && <PerAppPrompts />}

        <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>
          Word replacements now live in the <strong>Dictionary</strong> tab in the sidebar.
        </Text>
      </VStack>
    </VStack>
  );
}
