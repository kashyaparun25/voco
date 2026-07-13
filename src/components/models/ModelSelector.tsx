import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import ModelCard, { ModelInfo } from "./ModelCard";
import ModelDownloader from "./ModelDownloader";
import ModelRecommendation from "./ModelRecommendation";

export default function ModelSelector() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [activeTab, setActiveTab] = useState<"stt" | "llm" | "vad">("stt");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Load the real local (embedded) models from the backend. Models served by
  // remote providers (Ollama/OpenAI/Groq/…) are listed live inside each
  // provider's model picker, not here — this section is only for local
  // downloads that live on disk.
  const fetchModels = async () => {
    try {
      setLoading(true);
      const list = await invoke<ModelInfo[]>("list_models");
      setModels(list.map((m) => ({ ...m, is_external: false })));
      setError(null);
    } catch (err) {
      console.error("Failed to load models:", err);
      setError("Failed to load models from backend");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchModels();

    // Listen to model download progress events
    let unlistenProgress: (() => void) | undefined;
    let unlistenComplete: (() => void) | undefined;

    // Real backend event: { model_id, downloaded_bytes, total_bytes, percent }.
    // `percent` is 0..100; derive it from bytes if absent, and tolerate the
    // legacy { id, progress } shape.
    listen<any>("model-download-progress", (event) => {
      const payload = event.payload;
      if (!payload) return;

      const id = payload.model_id ?? payload.id;
      if (id === undefined) return;

      let progress: number | undefined;
      if (typeof payload.percent === "number") {
        progress = payload.percent / 100;
      } else if (typeof payload.progress === "number") {
        progress = payload.progress;
      } else if (typeof payload.downloaded_bytes === "number" && payload.total_bytes) {
        progress = payload.downloaded_bytes / payload.total_bytes;
      }
      if (progress === undefined) return;

      progress = Math.max(0, Math.min(1, progress));
      setModels((prev) =>
        prev.map((m) =>
          m.id === id
            ? { ...m, progress, is_downloaded: progress >= 1.0 ? true : m.is_downloaded }
            : m
        )
      );
    }).then((unsub) => {
      unlistenProgress = unsub;
    });

    listen<any>("model-download-complete", (event) => {
      const payload = event.payload;
      const id = typeof payload === "string" ? payload : payload?.id || payload?.model_id;
      if (id) {
        setModels((prev) =>
          prev.map((m) =>
            m.id === id
              ? { ...m, progress: 1.0, is_downloaded: true }
              : m
          )
        );
      }
      // Re-fetch to get official state
      fetchModels();
    }).then((unsub) => {
      unlistenComplete = unsub;
    });

    return () => {
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
    };
  }, []);

  const handleDownload = async (id: string) => {
    try {
      // Optimistically show a starting state. Real progress arrives via the
      // `model-download-progress` events wired up in the effect above; if no
      // event ever arrives, the bar simply stays at this "starting" value
      // (graceful) until `download_model` resolves.
      setModels((prev) =>
        prev.map((m) => (m.id === id ? { ...m, progress: 0.01 } : m))
      );

      // Await the download; when it resolves, mark complete (covers backends
      // that finish without emitting a final 100% event).
      await invoke("download_model", { id });
      setModels((prev) =>
        prev.map((m) =>
          m.id === id ? { ...m, progress: 1.0, is_downloaded: true } : m
        )
      );
    } catch (err) {
      console.error("Failed to start download:", err);
      setError(`Failed to download model: ${err}`);
      // Revert optimistic state
      fetchModels();
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await invoke("delete_model", { id });
      setModels((prev) =>
        prev.map((m) =>
          m.id === id ? { ...m, is_downloaded: false, progress: 0.0 } : m
        )
      );
      setError(null);
    } catch (err) {
      console.error("Failed to delete model:", err);
      setError(`Failed to delete model: ${err}`);
    }
  };

  const filteredModels = models.filter((m) => m.category === activeTab);
  const downloadingModels = models.filter((m) => m.progress > 0 && m.progress < 1.0 && !m.is_downloaded);

  return (
    <VStack gap={4} style={{ width: "100%" }}>
      <VStack gap={1}>
        <Text style={{ fontSize: "16px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
          Local Model Downloads
        </Text>
        <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
          Download on-device models for offline speech recognition and summarization.
          Cloud/server models are chosen directly in each provider's picker above.
        </Text>
      </VStack>

      {error && (
        <div style={{
          padding: "12px 16px",
          backgroundColor: "rgba(239, 68, 68, 0.1)",
          border: "1px solid var(--color-recording, #ef4444)",
          borderRadius: "8px",
          color: "var(--color-recording, #ef4444)",
          fontSize: "14px"
        }}>
          {error}
        </div>
      )}

      {/* RAM-based recommendation banner */}
      <ModelRecommendation />

      {/* Model Downloader Component showing active downloads */}
      <ModelDownloader downloadingModels={downloadingModels} />

      {/* Tab Selectors */}
      <HStack gap={2} style={{ borderBottom: "1px solid var(--color-border)", paddingBottom: 8 }}>
        <button
          style={{
            background: "none",
            border: "none",
            color: activeTab === "stt" ? "var(--color-accent)" : "var(--color-text-secondary)",
            borderBottom: activeTab === "stt" ? "2px solid var(--color-accent)" : "none",
            padding: "8px 16px",
            fontSize: "14px",
            fontWeight: "600",
            cursor: "pointer",
            marginBottom: "-9px",
            transition: "all 0.15s ease"
          }}
          onClick={() => setActiveTab("stt")}
        >
          Speech-to-Text
        </button>
        <button
          style={{
            background: "none",
            border: "none",
            color: activeTab === "llm" ? "var(--color-accent)" : "var(--color-text-secondary)",
            borderBottom: activeTab === "llm" ? "2px solid var(--color-accent)" : "none",
            padding: "8px 16px",
            fontSize: "14px",
            fontWeight: "600",
            cursor: "pointer",
            marginBottom: "-9px",
            transition: "all 0.15s ease"
          }}
          onClick={() => setActiveTab("llm")}
        >
          Large Language Models (LLM)
        </button>
        <button
          style={{
            background: "none",
            border: "none",
            color: activeTab === "vad" ? "var(--color-accent)" : "var(--color-text-secondary)",
            borderBottom: activeTab === "vad" ? "2px solid var(--color-accent)" : "none",
            padding: "8px 16px",
            fontSize: "14px",
            fontWeight: "600",
            cursor: "pointer",
            marginBottom: "-9px",
            transition: "all 0.15s ease"
          }}
          onClick={() => setActiveTab("vad")}
        >
          Voice Activity (VAD)
        </button>
      </HStack>

      {loading && models.length === 0 ? (
        <VStack style={{ alignItems: "center", padding: 40, width: "100%" }}>
          <Text style={{ color: "var(--color-text-secondary)" }}>Loading models...</Text>
        </VStack>
      ) : filteredModels.length === 0 ? (
        <VStack style={{ alignItems: "center", padding: 40, width: "100%" }}>
          <Text style={{ color: "var(--color-text-secondary)" }}>No models available in this category.</Text>
        </VStack>
      ) : (
        <div style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(300px, 1fr))",
          gap: "16px",
          width: "100%"
        }}>
          {filteredModels.map((model) => (
            <ModelCard
              key={model.id}
              model={model}
              onDownload={handleDownload}
              onDelete={handleDelete}
            />
          ))}
        </div>
      )}
    </VStack>
  );
}
