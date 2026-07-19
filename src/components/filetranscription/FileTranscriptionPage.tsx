import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Button } from "../ui";
import { showToast } from "../../hooks/useToast";
import TranscriptView from "../transcript/TranscriptView";

const AUDIO_EXTS = ["mp3", "m4a", "wav", "flac", "aac", "ogg", "mp4", "aiff", "wma"];

interface ImportItem {
  id: string;
  title: string;
  created_at: string;
  duration: number;
  source?: string;
}

function fmtDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
  } catch {
    return iso;
  }
}
function fmtDur(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export default function FileTranscriptionPage() {
  const [imports, setImports] = useState<ImportItem[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [segments, setSegments] = useState<any[]>([]);
  const [isDiarizing, setIsDiarizing] = useState(false);
  const [busy, setBusy] = useState(false);

  const fetchImports = useCallback(async (): Promise<ImportItem[]> => {
    try {
      const all = await invoke<ImportItem[]>("get_meetings");
      const list = (all || []).filter((m) => m.source === "import");
      setImports(list);
      return list;
    } catch (err) {
      console.warn("FileTranscription: fetch imports failed:", err);
      return [];
    }
  }, []);

  const fetchTranscript = useCallback(async (id: string) => {
    try {
      const list = await invoke<any[]>("get_meeting_transcript", { meetingId: id });
      setSegments(list);
    } catch (err) {
      console.warn("FileTranscription: fetch transcript failed:", err);
    }
  }, []);

  const select = useCallback((id: string) => {
    setSelectedId(id);
    setSegments([]);
    setIsDiarizing(false);
    void fetchTranscript(id);
  }, [fetchTranscript]);

  const startImport = useCallback(async (path: string) => {
    const name = path.split("/").pop() || "Audio File";
    const title = name.replace(/\.[^.]+$/, "") || "Audio File";
    setBusy(true);
    try {
      const id = await invoke<string>("import_audio", { path, title });
      showToast("Transcribing audio…", "success");
      await fetchImports();
      select(id);
    } catch (err) {
      console.error("import_audio failed", err);
      showToast(`Import failed: ${err}`, "error");
    } finally {
      setBusy(false);
    }
  }, [fetchImports, select]);

  const chooseFile = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: false, filters: [{ name: "Audio", extensions: AUDIO_EXTS }] });
      if (selected && typeof selected === "string") void startImport(selected);
    } catch (err) {
      console.error("choose file failed", err);
    }
  };

  const deleteImport = async (id: string) => {
    if (!confirm("Delete this transcription?")) return;
    try {
      await invoke("delete_meeting", { meetingId: id });
      if (selectedId === id) { setSelectedId(null); setSegments([]); }
      await fetchImports();
    } catch (err) {
      console.error("delete failed", err);
    }
  };

  useEffect(() => { void fetchImports(); }, [fetchImports]);

  // Live updates + drag-and-drop.
  useEffect(() => {
    let unTs: (() => void) | undefined;
    let unDiar: (() => void) | undefined;
    let unDrop: (() => void) | undefined;

    listen<{ meeting_id: string; reload?: boolean }>("meeting-transcript-update", (e) => {
      const { meeting_id, reload } = e.payload || ({} as any);
      if (meeting_id && meeting_id === selectedId && reload) {
        void fetchTranscript(meeting_id);
        void fetchImports(); // duration may have updated
      }
    }).then((u) => { unTs = u; }).catch(() => {});

    listen<{ meeting_id: string; status: string }>("meeting-diarizing", (e) => {
      const { meeting_id, status } = e.payload || ({} as any);
      if (meeting_id && meeting_id === selectedId) setIsDiarizing(status === "running");
    }).then((u) => { unDiar = u; }).catch(() => {});

    (async () => {
      try {
        const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
        unDrop = await getCurrentWebviewWindow().onDragDropEvent((event) => {
          if (event.payload.type === "drop") {
            const p = event.payload.paths?.find((x) => AUDIO_EXTS.some((ext) => x.toLowerCase().endsWith("." + ext)));
            if (p) void startImport(p);
          }
        });
      } catch { /* unavailable */ }
    })();

    return () => { if (unTs) unTs(); if (unDiar) unDiar(); if (unDrop) unDrop(); };
  }, [selectedId, fetchTranscript, fetchImports, startImport]);

  const handleExport = async (format: "txt" | "srt" | "vtt" | "json" | "markdown") => {
    if (!selectedId) return;
    try {
      const content = await invoke<string>("export_meeting", { meetingId: selectedId, format });
      const ext = format === "markdown" ? "md" : format;
      const title = imports.find((m) => m.id === selectedId)?.title || "transcript";
      const clean = title.toLowerCase().replace(/[^a-z0-9]+/g, "_") || "transcript";
      const { save } = await import("@tauri-apps/plugin-dialog");
      const { writeTextFile } = await import("@tauri-apps/plugin-fs");
      const path = await save({ defaultPath: `${clean}.${ext}` });
      if (path) { await writeTextFile(path, content); showToast("Transcript exported", "success"); }
    } catch (err) {
      console.error("export failed", err);
      showToast("Export failed", "error");
    }
  };

  const handleRenameSpeaker = async (speakerId: string, newName: string) => {
    try {
      await invoke("rename_speaker", { speakerId, newName });
      if (selectedId) void fetchTranscript(selectedId);
    } catch (err) { console.error("rename failed", err); }
  };

  const selected = imports.find((m) => m.id === selectedId) || null;

  return (
    <HStack style={{ height: "100%", width: "100%", overflow: "hidden" }} gap={0}>
      {/* Left: upload + history */}
      <div style={{ width: 280, height: "100%", borderRight: "1px solid var(--color-border)", padding: 16, boxSizing: "border-box", display: "flex", flexDirection: "column", gap: 14 }}>
        <Button variant="primary" fullWidth label={busy ? "Working…" : "Choose Audio File"} onClick={chooseFile} isDisabled={busy} />
        <Text style={{ fontSize: 11, color: "var(--color-text-secondary)", textAlign: "center" }}>or drag a file onto the window</Text>
        <Text style={{ fontSize: 11, fontWeight: 700, letterSpacing: "0.05em", textTransform: "uppercase", color: "var(--color-text-secondary)", marginTop: 4 }}>History</Text>
        <div style={{ flex: 1, overflowY: "auto", display: "flex", flexDirection: "column", gap: 8 }}>
          {imports.length === 0 ? (
            <Text style={{ fontSize: 12, color: "var(--color-text-secondary)", fontStyle: "italic" }}>No transcriptions yet.</Text>
          ) : (
            imports.map((m) => {
              const active = m.id === selectedId;
              return (
                <div
                  key={m.id}
                  onClick={() => select(m.id)}
                  style={{
                    padding: "10px 12px",
                    borderRadius: 10,
                    cursor: "pointer",
                    border: `1px solid ${active ? "var(--color-accent)" : "var(--color-border)"}`,
                    backgroundColor: active ? "var(--color-background-surface-hover)" : "var(--color-background-surface)",
                    position: "relative",
                  }}
                >
                  <Text style={{ fontSize: 13, fontWeight: 600, color: "var(--color-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{m.title}</Text>
                  <Text style={{ fontSize: 11, color: "var(--color-text-secondary)" }}>{fmtDate(m.created_at)} · {fmtDur(m.duration)}</Text>
                  <button
                    onClick={(e) => { e.stopPropagation(); void deleteImport(m.id); }}
                    title="Delete"
                    style={{ position: "absolute", top: 8, right: 8, background: "none", border: "none", color: "var(--color-text-secondary)", cursor: "pointer", fontSize: 14, lineHeight: 1 }}
                  >
                    ×
                  </button>
                </div>
              );
            })
          )}
        </div>
      </div>

      {/* Right: transcript / empty state */}
      <div style={{ flex: 1, height: "100%", padding: 24, boxSizing: "border-box", display: "flex", flexDirection: "column", gap: 16, overflow: "hidden" }}>
        {!selectedId ? (
          <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", textAlign: "center", gap: 16, color: "var(--color-text-secondary)" }}>
            <div style={{ width: 72, height: 72, borderRadius: 20, backgroundColor: "var(--color-background-surface)", display: "flex", alignItems: "center", justifyContent: "center", color: "var(--color-accent)" }}>
              <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 30, height: 30 }}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75V16.5m-13.5-9L12 3m0 0 4.5 4.5M12 3v13.5" />
              </svg>
            </div>
            <VStack gap={1} style={{ alignItems: "center" }}>
              <Text style={{ fontSize: 18, fontWeight: 700, color: "var(--color-text-primary)" }}>Transcribe an audio file</Text>
              <Text style={{ fontSize: 13, color: "var(--color-text-secondary)", maxWidth: 380 }}>
                Choose a file (or drag one in), or pick a past transcription from the list. Supports MP3, M4A, WAV, FLAC, AAC, OGG, MP4, AIFF.
              </Text>
            </VStack>
          </div>
        ) : (
          <>
            <HStack style={{ justifyContent: "space-between", alignItems: "center" }}>
              <VStack gap={0}>
                <Text style={{ fontSize: 18, fontWeight: 700, color: "var(--color-text-primary)" }}>{selected?.title}</Text>
                {selected && <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>{fmtDate(selected.created_at)} · {fmtDur(selected.duration)}</Text>}
              </VStack>
              <HStack gap={2} style={{ alignItems: "center" }}>
                {isDiarizing && (
                  <span style={{ display: "inline-flex", alignItems: "center", gap: 6, fontSize: 11, fontWeight: 600, padding: "3px 10px", borderRadius: 999, backgroundColor: "rgba(124,58,237,0.12)", color: "var(--color-accent-text, var(--color-accent))", border: "1px solid var(--color-accent)" }}>
                    <span style={{ width: 10, height: 10, borderRadius: "50%", border: "2px solid var(--color-accent)", borderTopColor: "transparent", display: "inline-block", animation: "voco-spin 0.7s linear infinite" }} />
                    Diarizing…
                  </span>
                )}
                <ExportMenu onExport={handleExport} />
              </HStack>
            </HStack>
            <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
              {segments.length === 0 ? (
                <VStack gap={2} style={{ flex: 1, alignItems: "center", justifyContent: "center", color: "var(--color-text-secondary)" }}>
                  <span style={{ width: 22, height: 22, borderRadius: "50%", border: "3px solid var(--color-border-strong)", borderTopColor: "var(--color-accent)", display: "inline-block", animation: "voco-spin 0.8s linear infinite" }} />
                  <Text style={{ fontSize: 13, color: "var(--color-text-secondary)" }}>Transcribing… segments appear as they're processed.</Text>
                </VStack>
              ) : (
                <TranscriptView segments={segments} onRenameSpeaker={handleRenameSpeaker} isRecording={false} />
              )}
            </div>
          </>
        )}
      </div>
    </HStack>
  );
}

function ExportMenu({ onExport }: { onExport: (format: "txt" | "srt" | "vtt" | "json" | "markdown") => void }) {
  const [open, setOpen] = useState(false);
  const formats: Array<{ id: "markdown" | "txt" | "srt" | "vtt" | "json"; label: string }> = [
    { id: "markdown", label: "Markdown (.md)" },
    { id: "txt", label: "Plain Text (.txt)" },
    { id: "srt", label: "Subtitles (.srt)" },
    { id: "vtt", label: "WebVTT (.vtt)" },
    { id: "json", label: "JSON (.json)" },
  ];
  return (
    <div style={{ position: "relative" }}>
      <Button variant="secondary" size="sm" label="Export ▾" onClick={() => setOpen(!open)} />
      {open && (
        <>
          <div onClick={() => setOpen(false)} style={{ position: "fixed", inset: 0, zIndex: 99 }} />
          <div style={{ position: "absolute", right: 0, top: 36, zIndex: 100, width: 180, backgroundColor: "var(--color-background-elevated)", border: "1px solid var(--color-border-strong)", borderRadius: 8, boxShadow: "0 6px 16px rgba(0,0,0,0.3)", padding: 4, display: "flex", flexDirection: "column" }}>
            {formats.map((f) => (
              <button key={f.id} onClick={() => { setOpen(false); onExport(f.id); }} style={{ padding: "8px 12px", textAlign: "left", background: "transparent", border: "none", color: "var(--color-text-primary)", fontSize: 13, cursor: "pointer", borderRadius: 4 }}
                onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "var(--color-background-surface-hover)")}
                onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = "transparent")}>
                {f.label}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
