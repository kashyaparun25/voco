import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TextInput } from "../ui";
import { Text } from "@astryxdesign/core/Text";
import { VStack } from "@astryxdesign/core/Layout";

export interface SearchResult {
  meeting_id: string;
  meeting_title: string;
  segment_id: string;
  text: string;
  start_time: number;
  speaker_name: string | null;
}

interface GlobalSearchProps {
  /** Called when a result is clicked: selects the meeting + target segment. */
  onSelectResult: (meetingId: string, segmentId: string) => void;
}

function formatTime(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

/**
 * GlobalSearch — full-text search across all meeting transcripts.
 *
 * Debounces input (~300ms), calls the backend `search_transcripts` command,
 * and lists matches grouped by meeting. Degrades to an empty state if the
 * command is unavailable.
 */
export default function GlobalSearch({ onSelectResult }: GlobalSearchProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [unavailable, setUnavailable] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);

    const trimmed = query.trim();
    if (trimmed.length === 0) {
      setResults([]);
      setLoading(false);
      return;
    }

    setLoading(true);
    debounceRef.current = setTimeout(async () => {
      try {
        const res = await invoke<SearchResult[]>("search_transcripts", { query: trimmed });
        setResults(Array.isArray(res) ? res : []);
        setUnavailable(false);
      } catch (err) {
        console.warn("search_transcripts unavailable:", err);
        setResults([]);
        setUnavailable(true);
      } finally {
        setLoading(false);
      }
    }, 300);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query]);

  // Group results by meeting, preserving order of first appearance.
  const grouped: Array<{ meetingId: string; title: string; items: SearchResult[] }> = [];
  const index = new Map<string, number>();
  for (const r of results) {
    let gi = index.get(r.meeting_id);
    if (gi === undefined) {
      gi = grouped.length;
      index.set(r.meeting_id, gi);
      grouped.push({ meetingId: r.meeting_id, title: r.meeting_title, items: [] });
    }
    grouped[gi].items.push(r);
  }

  const hasQuery = query.trim().length > 0;

  return (
    <VStack gap={2} style={{ width: "100%" }}>
      <TextInput
        label="Search all meetings"
        placeholder="Search all transcripts..."
        value={query}
        onChange={(val) => setQuery(val)}
        style={{ width: "100%", backgroundColor: "var(--color-background-surface)" }}
      />

      {hasQuery && (
        <div
          style={{
            width: "100%",
            maxHeight: "260px",
            overflowY: "auto",
            border: "1px solid var(--color-border)",
            borderRadius: "8px",
            backgroundColor: "var(--color-background-surface)",
            display: "flex",
            flexDirection: "column",
          }}
        >
          {loading ? (
            <Text style={{ padding: "12px", fontSize: "12px", color: "var(--color-text-secondary)" }}>
              Searching...
            </Text>
          ) : unavailable ? (
            <Text style={{ padding: "12px", fontSize: "12px", color: "var(--color-text-secondary)" }}>
              Search is unavailable.
            </Text>
          ) : grouped.length === 0 ? (
            <Text style={{ padding: "12px", fontSize: "12px", color: "var(--color-text-secondary)" }}>
              No matches found.
            </Text>
          ) : (
            grouped.map((group) => (
              <div key={group.meetingId}>
                <div style={{
                  padding: "6px 10px",
                  fontSize: "11px",
                  fontWeight: 700,
                  color: "var(--color-text-secondary)",
                  backgroundColor: "var(--color-background-surface-hover)",
                  borderBottom: "1px solid var(--color-border)",
                  position: "sticky",
                  top: 0,
                }}>
                  {group.title}
                </div>
                {group.items.map((item) => (
                  <button
                    key={item.segment_id}
                    onClick={() => onSelectResult(item.meeting_id, item.segment_id)}
                    className="voco-segment-enter"
                    style={{
                      display: "block",
                      width: "100%",
                      textAlign: "left",
                      padding: "8px 10px",
                      background: "transparent",
                      border: "none",
                      borderBottom: "1px solid var(--color-border)",
                      cursor: "pointer",
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "var(--color-background-surface-hover)")}
                    onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = "transparent")}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", gap: 8, marginBottom: 2 }}>
                      <span style={{ fontSize: "11px", fontWeight: 600, color: "var(--color-accent-text, var(--color-accent))" }}>
                        {item.speaker_name || "Speaker"}
                      </span>
                      <span style={{ fontSize: "10px", fontFamily: "monospace", color: "var(--color-text-secondary)" }}>
                        {formatTime(item.start_time)}
                      </span>
                    </div>
                    <div style={{
                      fontSize: "12px",
                      color: "var(--color-text-primary)",
                      lineHeight: 1.4,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      display: "-webkit-box",
                      WebkitLineClamp: 2,
                      WebkitBoxOrient: "vertical",
                    }}>
                      {item.text}
                    </div>
                  </button>
                ))}
              </div>
            ))
          )}
        </div>
      )}
    </VStack>
  );
}
