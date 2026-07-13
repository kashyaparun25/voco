import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Button } from "../ui";

/**
 * Per-app AI enhancement prompts.
 *
 * Assign a custom cleanup/formatting prompt per application (matched against
 * the frontmost app captured when dictation starts). If the app you're
 * dictating into matches one of these, its prompt overrides the global one.
 * Stored as JSON in `dictation_app_prompts`.
 */

interface Rule {
  app: string;
  prompt: string;
}

const inputStyle: React.CSSProperties = {
  padding: "8px 12px",
  borderRadius: 8,
  backgroundColor: "var(--color-background-elevated)",
  color: "var(--color-text-primary)",
  border: "1px solid var(--color-border-strong)",
  fontSize: 13,
  width: "100%",
  outline: "none",
  boxSizing: "border-box",
  fontFamily: "inherit",
};

export default function PerAppPrompts() {
  const [rules, setRules] = useState<Rule[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const raw = await invoke<string | null>("get_setting", { key: "dictation_app_prompts" });
        if (raw) {
          const parsed = JSON.parse(raw);
          if (Array.isArray(parsed)) setRules(parsed.filter((r) => r && typeof r.app === "string"));
        }
      } catch (err) {
        console.warn("PerAppPrompts: failed to load:", err);
      } finally {
        setLoaded(true);
      }
    })();
  }, []);

  const persist = async (next: Rule[]) => {
    const clean = next.filter((r) => r.app.trim() !== "" && r.prompt.trim() !== "");
    try {
      await invoke("set_setting", { key: "dictation_app_prompts", value: JSON.stringify(clean) });
    } catch (err) {
      console.warn("PerAppPrompts: failed to save:", err);
    }
  };

  const update = (i: number, field: keyof Rule, value: string) =>
    setRules((prev) => prev.map((r, idx) => (idx === i ? { ...r, [field]: value } : r)));

  const addRow = () => setRules((prev) => [...prev, { app: "", prompt: "" }]);

  const removeRow = (i: number) =>
    setRules((prev) => {
      const next = prev.filter((_, idx) => idx !== i);
      void persist(next);
      return next;
    });

  return (
    <VStack
      gap={3}
      style={{
        padding: 16,
        borderRadius: 12,
        border: "1px dashed var(--color-border-strong)",
        backgroundColor: "var(--color-background-surface)",
      }}
    >
      <VStack gap={1}>
        <Text style={{ fontSize: 13, fontWeight: 700, color: "var(--color-text-primary)" }}>
          Per-app prompts (optional)
        </Text>
        <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>
          Override the enhancement prompt for specific apps. Matched against the app name you're
          dictating into (e.g. “Slack”, “Mail”, “Code”, “Terminal”).
        </Text>
      </VStack>

      {loaded &&
        rules.map((r, i) => (
          <VStack key={i} gap={2} style={{ width: "100%", paddingBottom: 8, borderBottom: "1px solid var(--color-border)" }}>
            <HStack gap={2} style={{ width: "100%", alignItems: "center" }}>
              <input
                style={{ ...inputStyle, flex: 1 }}
                placeholder="App name (e.g. Slack)"
                value={r.app}
                onChange={(e) => update(i, "app", e.target.value)}
                onBlur={() => void persist(rules)}
              />
              <button
                onClick={() => removeRow(i)}
                title="Remove"
                style={{
                  width: 32,
                  height: 32,
                  flexShrink: 0,
                  borderRadius: 8,
                  border: "1px solid var(--color-border)",
                  backgroundColor: "transparent",
                  color: "var(--color-recording, #ef4444)",
                  cursor: "pointer",
                  fontSize: 16,
                  lineHeight: 1,
                }}
              >
                ×
              </button>
            </HStack>
            <textarea
              style={{ ...inputStyle, resize: "vertical" }}
              rows={2}
              placeholder="Prompt for this app (e.g. Format as a concise Slack message, no greeting.)"
              value={r.prompt}
              onChange={(e) => update(i, "prompt", e.target.value)}
              onBlur={() => void persist(rules)}
            />
          </VStack>
        ))}

      <HStack>
        <Button variant="secondary" label="+ Add app rule" onClick={addRow} style={{ cursor: "pointer", fontSize: 13 }} />
      </HStack>
    </VStack>
  );
}
