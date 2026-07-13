import { useEffect, useState, useCallback } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Card } from "@astryxdesign/core/Card";
import { Text } from "@astryxdesign/core/Text";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { showToast } from "../../hooks/useToast";

/** A single dictation record as returned by the backend. */
interface Dictation {
  id: string;
  text: string;
  created_at: string;
  duration_ms: number;
  model: string | null;
  has_audio: boolean;
}

const CopyIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 15, height: 15 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 17.25v3.375c0 .621-.504 1.125-1.125 1.125h-9.75a1.125 1.125 0 0 1-1.125-1.125V7.875c0-.621.504-1.125 1.125-1.125H6.75a9.06 9.06 0 0 1 1.5.124m7.5 10.376h3.375c.621 0 1.125-.504 1.125-1.125V11.25c0-4.46-3.243-8.161-7.5-8.876a9.06 9.06 0 0 0-1.5-.124H9.375c-.621 0-1.125.504-1.125 1.125v3.5m7.5 10.375H9.375a1.125 1.125 0 0 1-1.125-1.125v-9.25m12 6.625v-1.875a3.375 3.375 0 0 0-3.375-3.375h-1.5a1.125 1.125 0 0 1-1.125-1.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H9.75" />
  </svg>
);

const PlayIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 15, height: 15 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M5.25 5.653c0-.856.917-1.398 1.667-.986l11.54 6.347a1.125 1.125 0 0 1 0 1.972l-11.54 6.347a1.125 1.125 0 0 1-1.667-.986V5.653Z" />
  </svg>
);

const TrashIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 15, height: 15 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
  </svg>
);

const actionButtonStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 6,
  padding: "6px 12px",
  borderRadius: 8,
  border: "1px solid var(--color-border-strong)",
  backgroundColor: "var(--color-background-elevated)",
  color: "var(--color-text-secondary)",
  fontSize: 12,
  fontWeight: 600,
  cursor: "pointer",
  transition: "all 0.15s ease",
};

/** Human-friendly relative time, falling back to the absolute date. */
function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const diffMs = Date.now() - d.getTime();
  const sec = Math.floor(diffMs / 1000);
  if (sec < 60) return "just now";
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  if (day < 7) return `${day}d ago`;
  return d.toLocaleString();
}

function formatDuration(ms: number): string {
  if (!ms || ms < 0) return "";
  const totalSec = Math.round(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return min > 0 ? `${min}m ${sec}s` : `${sec}s`;
}

/**
 * DictationHistory — scrollable list of past dictations.
 *
 * Backend contract (all wrapped in try/catch; degrades gracefully):
 *   - get_dictations({ limit? }) -> Dictation[] (newest first)
 *   - get_dictation_audio_path({ id }) -> string | null
 *   - delete_dictation({ id }) -> void
 *   - events: `dictation-history-updated`, `dictation-final` -> refetch
 */
export default function DictationHistory() {
  const [dictations, setDictations] = useState<Dictation[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [audioSrc, setAudioSrc] = useState<Record<string, string>>({});

  const fetchDictations = useCallback(async () => {
    try {
      const list = await invoke<Dictation[]>("get_dictations", { limit: 100 });
      if (Array.isArray(list)) setDictations(list);
    } catch (err) {
      console.warn("get_dictations unavailable:", err);
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void fetchDictations();

    let unUpdated: (() => void) | undefined;
    let unFinal: (() => void) | undefined;

    listen("dictation-history-updated", () => { void fetchDictations(); })
      .then((un) => { unUpdated = un; })
      .catch(() => { /* Tauri unavailable */ });

    listen("dictation-final", () => { void fetchDictations(); })
      .then((un) => { unFinal = un; })
      .catch(() => { /* Tauri unavailable */ });

    return () => {
      if (unUpdated) unUpdated();
      if (unFinal) unFinal();
    };
  }, [fetchDictations]);

  const handleCopy = useCallback(async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      showToast("Copied", "success");
    } catch (err) {
      console.warn("clipboard write failed:", err);
      showToast("Couldn't copy", "error");
    }
  }, []);

  const handlePlay = useCallback(async (id: string) => {
    if (audioSrc[id]) {
      setAudioSrc((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      return;
    }
    try {
      const path = await invoke<string | null>("get_dictation_audio_path", { id });
      if (path) {
        setAudioSrc((prev) => ({ ...prev, [id]: convertFileSrc(path) }));
      } else {
        showToast("No audio for this dictation", "info");
      }
    } catch (err) {
      console.warn("get_dictation_audio_path unavailable:", err);
      showToast("Audio unavailable", "error");
    }
  }, [audioSrc]);

  const handleDelete = useCallback(async (id: string) => {
    try {
      await invoke("delete_dictation", { id });
      setDictations((prev) => prev.filter((d) => d.id !== id));
      setAudioSrc((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      showToast("Dictation deleted", "success");
    } catch (err) {
      console.warn("delete_dictation failed:", err);
      showToast("Couldn't delete dictation", "error");
    }
  }, []);

  return (
    <VStack gap={3} style={{ width: "100%", flex: 1, minHeight: 0 }}>
      <HStack style={{ justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ fontSize: 14, fontWeight: "bold", color: "var(--color-text-secondary)" }}>
          History
        </Text>
        {dictations.length > 0 && (
          <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>
            {dictations.length} {dictations.length === 1 ? "entry" : "entries"}
          </Text>
        )}
      </HStack>

      {loaded && dictations.length === 0 ? (
        <div style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          padding: "32px 16px",
          borderRadius: 12,
          border: "1px dashed var(--color-border)",
          backgroundColor: "var(--color-background-surface)",
        }}>
          <Text style={{ fontSize: 13, color: "var(--color-text-secondary)" }}>
            Your dictations will appear here.
          </Text>
        </div>
      ) : (
        <VStack gap={2} style={{ width: "100%", flex: 1, minHeight: 0, overflowY: "auto" }}>
          {dictations.map((d) => (
            <Card
              key={d.id}
              style={{
                padding: 14,
                backgroundColor: "var(--color-background-surface)",
                border: "1px solid var(--color-border)",
                borderRadius: 12,
                display: "flex",
                flexDirection: "column",
                gap: 10,
              }}
            >
              <Text
                style={{
                  fontSize: 14,
                  color: "var(--color-text-primary)",
                  whiteSpace: "pre-wrap",
                  userSelect: "text",
                  lineHeight: 1.5,
                }}
              >
                {d.text || <span style={{ color: "var(--color-text-secondary)", fontStyle: "italic" }}>(no text)</span>}
              </Text>

              <HStack style={{ justifyContent: "space-between", alignItems: "center", flexWrap: "wrap", gap: 8 }}>
                <HStack gap={2} style={{ alignItems: "center", flexWrap: "wrap" }}>
                  <Text style={{ fontSize: 11, color: "var(--color-text-secondary)" }}>
                    {formatTimestamp(d.created_at)}
                  </Text>
                  {d.duration_ms > 0 && (
                    <Text style={{ fontSize: 11, color: "var(--color-text-secondary)" }}>
                      · {formatDuration(d.duration_ms)}
                    </Text>
                  )}
                  {d.model && (
                    <span style={{
                      fontSize: 10,
                      fontWeight: 600,
                      padding: "2px 8px",
                      borderRadius: 999,
                      backgroundColor: "var(--color-background-surface-hover)",
                      color: "var(--color-text-secondary)",
                      border: "1px solid var(--color-border)",
                    }}>
                      {d.model}
                    </span>
                  )}
                </HStack>

                <HStack gap={2} style={{ alignItems: "center" }}>
                  <button
                    style={actionButtonStyle}
                    onClick={() => handleCopy(d.text)}
                    onMouseEnter={(e) => (e.currentTarget.style.color = "var(--color-text-primary)")}
                    onMouseLeave={(e) => (e.currentTarget.style.color = "var(--color-text-secondary)")}
                  >
                    <CopyIcon /> Copy
                  </button>
                  {d.has_audio && (
                    <button
                      style={actionButtonStyle}
                      onClick={() => handlePlay(d.id)}
                      onMouseEnter={(e) => (e.currentTarget.style.color = "var(--color-text-primary)")}
                      onMouseLeave={(e) => (e.currentTarget.style.color = "var(--color-text-secondary)")}
                    >
                      <PlayIcon /> {audioSrc[d.id] ? "Hide" : "Play"}
                    </button>
                  )}
                  <button
                    style={actionButtonStyle}
                    onClick={() => handleDelete(d.id)}
                    onMouseEnter={(e) => (e.currentTarget.style.color = "var(--color-recording)")}
                    onMouseLeave={(e) => (e.currentTarget.style.color = "var(--color-text-secondary)")}
                  >
                    <TrashIcon /> Delete
                  </button>
                </HStack>
              </HStack>

              {audioSrc[d.id] && (
                <audio controls src={audioSrc[d.id]} style={{ width: "100%" }} />
              )}
            </Card>
          ))}
        </VStack>
      )}
    </VStack>
  );
}
