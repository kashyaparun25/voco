import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Button } from "../ui";

/**
 * Custom Dictionary editor.
 *
 * Each entry maps a spoken/misheard form ("from") to the exact text you want
 * ("to") — names, acronyms, product names, unusual spellings. Stored as a JSON
 * array in the `custom_dictionary` setting; the backend applies it (whole-word,
 * case-insensitive, casing-preserving) to every dictation before pasting.
 */

interface Entry {
  from: string;
  to: string;
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
};

const labelStyle: React.CSSProperties = {
  fontSize: 11,
  fontWeight: 600,
  color: "var(--color-text-secondary)",
};

export default function CustomDictionary() {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const raw = await invoke<string | null>("get_setting", { key: "custom_dictionary" });
        if (raw) {
          const parsed = JSON.parse(raw);
          if (Array.isArray(parsed)) {
            setEntries(parsed.filter((e) => e && typeof e.from === "string"));
          }
        }
      } catch (err) {
        console.warn("CustomDictionary: failed to load:", err);
      } finally {
        setLoaded(true);
      }
    })();
  }, []);

  // Persist the current (cleaned) list. Called after edits.
  const persist = async (next: Entry[]) => {
    const clean = next.filter((e) => e.from.trim() !== "");
    try {
      await invoke("set_setting", {
        key: "custom_dictionary",
        value: JSON.stringify(clean),
      });
    } catch (err) {
      console.warn("CustomDictionary: failed to save:", err);
    }
  };

  const update = (i: number, field: keyof Entry, value: string) => {
    setEntries((prev) => prev.map((e, idx) => (idx === i ? { ...e, [field]: value } : e)));
  };

  const addRow = () => setEntries((prev) => [...prev, { from: "", to: "" }]);

  const removeRow = (i: number) => {
    setEntries((prev) => {
      const next = prev.filter((_, idx) => idx !== i);
      void persist(next);
      return next;
    });
  };

  return (
    <VStack
      gap={3}
      style={{
        padding: 16,
        borderRadius: 12,
        border: "1px solid var(--color-border)",
        backgroundColor: "var(--color-background-surface)",
      }}
    >
      <VStack gap={1}>
        <Text style={{ fontSize: 14, fontWeight: 700, color: "var(--color-text-primary)" }}>
          Custom Dictionary
        </Text>
        <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>
          Auto-replace spoken/misheard words with the exact text you want — names, acronyms, product
          names, unusual spellings. Applied to every dictation.
        </Text>
      </VStack>

      {loaded && entries.length > 0 && (
        <VStack gap={2} style={{ width: "100%" }}>
          <HStack gap={2} style={{ width: "100%" }}>
            <Text style={{ ...labelStyle, flex: 1 }}>When I say (heard as)</Text>
            <Text style={{ ...labelStyle, flex: 1 }}>Write it as</Text>
            <div style={{ width: 32 }} />
          </HStack>
          {entries.map((e, i) => (
            <HStack key={i} gap={2} style={{ width: "100%", alignItems: "center" }}>
              <input
                style={{ ...inputStyle, flex: 1 }}
                placeholder="e.g. cubernetes"
                value={e.from}
                onChange={(ev) => update(i, "from", ev.target.value)}
                onBlur={() => void persist(entries)}
              />
              <input
                style={{ ...inputStyle, flex: 1 }}
                placeholder="e.g. Kubernetes"
                value={e.to}
                onChange={(ev) => update(i, "to", ev.target.value)}
                onBlur={() => void persist(entries)}
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
          ))}
        </VStack>
      )}

      {loaded && entries.length === 0 && (
        <Text style={{ fontSize: 12, color: "var(--color-text-secondary)", fontStyle: "italic" }}>
          No entries yet.
        </Text>
      )}

      <HStack>
        <Button
          variant="secondary"
          label="+ Add word"
          onClick={addRow}
          style={{ cursor: "pointer", fontSize: 13 }}
        />
      </HStack>
    </VStack>
  );
}
