import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Button } from "../ui";
import { TextInput } from "../ui";
import { showToast } from "../../hooks/useToast";

type Category = "stt" | "llm" | "vad";

/**
 * Add a model by an arbitrary download URL (e.g. a HuggingFace GGUF/ggml/ONNX).
 * Calls the `add_custom_model` backend command, which downloads it and makes it
 * selectable everywhere alongside the built-in models.
 */
export default function CustomModelAdder({ onAdded }: { onAdded?: () => void }) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [category, setCategory] = useState<Category>("stt");
  const [busy, setBusy] = useState(false);

  const add = async () => {
    const trimmedUrl = url.trim();
    if (!/^https?:\/\//.test(trimmedUrl)) {
      showToast("Enter a valid http(s) URL", "error");
      return;
    }
    setBusy(true);
    try {
      await invoke<string>("add_custom_model", {
        name: name.trim() || "Custom Model",
        url: trimmedUrl,
        category,
      });
      showToast("Downloading custom model…", "success");
      setName("");
      setUrl("");
      onAdded?.();
    } catch (err) {
      console.error("add_custom_model failed", err);
      showToast(`Failed to add model: ${err}`, "error");
    } finally {
      setBusy(false);
    }
  };

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
        <Text style={{ fontSize: 14, fontWeight: 700, color: "var(--color-text-primary)" }}>
          Add model from URL
        </Text>
        <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>
          Paste a direct download link to a GGUF (LLM), ggml (Whisper), or ONNX file — e.g. a HuggingFace
          <span style={{ fontFamily: "monospace" }}> /resolve/main/…</span> URL.
        </Text>
      </VStack>

      <TextInput
        label="Name"
        placeholder="e.g. Qwen 7B Instruct"
        value={name}
        onChange={(v: string) => setName(v)}
        style={{ width: "100%" }}
      />
      <TextInput
        label="Download URL"
        placeholder="https://huggingface.co/…/resolve/main/model.gguf"
        value={url}
        onChange={(v: string) => setUrl(v)}
        style={{ width: "100%" }}
      />

      <HStack gap={3} style={{ alignItems: "flex-end", justifyContent: "space-between" }}>
        <VStack gap={1} style={{ flex: 1 }}>
          <Text style={{ fontSize: 12, fontWeight: 600, color: "var(--color-text-secondary)" }}>
            Type
          </Text>
          <select
            value={category}
            onChange={(e) => setCategory(e.target.value as Category)}
            style={{
              padding: "8px 12px",
              borderRadius: 8,
              backgroundColor: "var(--color-background-elevated)",
              color: "var(--color-text-primary)",
              border: "1px solid var(--color-border-strong)",
              fontSize: 13,
              cursor: "pointer",
            }}
          >
            <option value="stt">Speech-to-Text (Whisper/ggml)</option>
            <option value="llm">LLM (GGUF)</option>
            <option value="vad">VAD (ONNX)</option>
          </select>
        </VStack>
        <Button
          variant="primary"
          label={busy ? "Adding…" : "Add & Download"}
          isDisabled={busy}
          onClick={add}
          style={{
            cursor: busy ? "default" : "pointer",
            backgroundColor: "var(--color-accent)",
            color: "#ffffff",
            border: "none",
            padding: "10px 18px",
            borderRadius: 8,
            fontWeight: 600,
          }}
        />
      </HStack>
    </VStack>
  );
}
