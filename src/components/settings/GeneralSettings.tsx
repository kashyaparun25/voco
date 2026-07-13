import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { TextInput, Toggle } from "../ui";

const LANGUAGES = [
  { value: "en", label: "English" },
  { value: "auto", label: "Auto-detect" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
  { value: "it", label: "Italian" },
  { value: "pt", label: "Portuguese" },
  { value: "hi", label: "Hindi" },
  { value: "zh", label: "Chinese" },
  { value: "ja", label: "Japanese" },
];

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

function ToggleRow({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <HStack style={{ justifyContent: "space-between", alignItems: "center", width: "100%" }}>
      <VStack gap={1} style={{ flex: 1 }}>
        <Text style={{ fontSize: "14px", fontWeight: "600", color: "var(--color-text-primary)" }}>
          {label}
        </Text>
        {description ? (
          <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>{description}</Text>
        ) : null}
      </VStack>
      <Toggle checked={checked} onChange={() => onChange(!checked)} />
    </HStack>
  );
}

export default function GeneralSettings() {
  const [language, setLanguage] = useState<string>("en");
  const [modelsDir, setModelsDir] = useState<string>("");
  const [launchAtLogin, setLaunchAtLogin] = useState<boolean>(false);

  useEffect(() => {
    const load = async () => {
      try {
        const lang = await invoke<string | null>("get_setting", { key: "language" });
        if (lang) setLanguage(lang);

        const dir = await invoke<string | null>("get_setting", { key: "models_dir" });
        if (dir) setModelsDir(dir);

        const lal = await invoke<string | null>("get_setting", { key: "launch_at_login" });
        if (lal != null) setLaunchAtLogin(lal === "true");
      } catch (err) {
        console.warn("GeneralSettings: failed to load settings (Tauri unavailable?):", err);
      }
    };
    void load();
  }, []);

  const persist = async (key: string, value: string) => {
    try {
      await invoke("set_setting", { key, value });
    } catch (err) {
      console.warn(`GeneralSettings: failed to save "${key}":`, err);
    }
  };

  return (
    <VStack gap={4} style={{ width: "100%" }}>
      <VStack gap={1}>
        <Text style={{ fontSize: "20px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
          General
        </Text>
        <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
          App-wide preferences for language, storage, and startup behavior.
        </Text>
      </VStack>

      <VStack gap={2} style={{ maxWidth: 420 }}>
        <Text style={{ fontSize: "13px", fontWeight: "600", color: "var(--color-text-secondary)" }}>
          Language
        </Text>
        <select
          value={language}
          onChange={(e) => {
            setLanguage(e.target.value);
            void persist("language", e.target.value);
          }}
          style={selectStyle}
        >
          {LANGUAGES.map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
        </select>
      </VStack>

      <VStack gap={2}>
        <TextInput
          label="Models Directory"
          value={modelsDir || "~/.voco/models"}
          isDisabled
          style={{ width: "100%" }}
        />
      </VStack>

      <VStack gap={3} style={{ maxWidth: 480, marginTop: 4 }}>
        <ToggleRow
          label="Launch at login"
          description="Start Voco automatically when you log in (coming soon)."
          checked={launchAtLogin}
          onChange={(next) => {
            setLaunchAtLogin(next);
            void persist("launch_at_login", String(next));
          }}
        />
      </VStack>
    </VStack>
  );
}
