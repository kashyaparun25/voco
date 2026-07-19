import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Settings → Logs: inspect / copy the app log for debugging.
 *  Reads ~/Library/Logs/Voco.log through the read_app_logs command. */

const LOG_PATH = "~/Library/Logs/Voco.log";
const LINE_OPTIONS = [200, 500, 2000] as const;
const AUTO_REFRESH_MS = 2000;

/* Small shared control styles (theme-var driven, self-contained). */

const btnStyle: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 5,
  padding: "5px 11px",
  borderRadius: 999,
  border: "1px solid var(--color-border, rgba(127,127,127,0.25))",
  background: "transparent",
  color: "var(--color-text-secondary)",
  fontSize: 12,
  fontWeight: 500,
  fontFamily: "inherit",
  lineHeight: 1.4,
  cursor: "pointer",
  whiteSpace: "nowrap",
};

const selectStyle: CSSProperties = {
  padding: "5px 8px",
  borderRadius: 8,
  border: "1px solid var(--color-border, rgba(127,127,127,0.25))",
  backgroundColor: "var(--color-background-surface, rgba(127,127,127,0.08))",
  color: "var(--color-text-primary)",
  fontSize: 12,
  fontFamily: "inherit",
  outline: "none",
  cursor: "pointer",
};

function lineColor(line: string): string {
  if (line.includes(" ERROR")) return "var(--color-recording, #ef4444)";
  if (line.includes(" WARN")) return "var(--color-warning, #eab308)";
  return "var(--color-text-secondary)";
}

export default function LogsViewer() {
  const [lineCount, setLineCount] = useState<number>(500);
  const [raw, setRaw] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [copied, setCopied] = useState(false);

  const paneRef = useRef<HTMLDivElement>(null);
  // Stick to the bottom on refresh unless the user scrolled up
  // (same near-bottom detection as the transcript sheet).
  const shouldAutoScrollRef = useRef(true);

  const load = useCallback(async (lines: number) => {
    try {
      const text = await invoke<string>("read_app_logs", { lines });
      setRaw(text ?? "");
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }, []);

  // Initial load + reload when the tail size changes.
  useEffect(() => {
    void load(lineCount);
  }, [load, lineCount]);

  // Optional 2s auto-refresh.
  useEffect(() => {
    if (!autoRefresh) return;
    const id = window.setInterval(() => void load(lineCount), AUTO_REFRESH_MS);
    return () => window.clearInterval(id);
  }, [autoRefresh, load, lineCount]);

  const handleScroll = () => {
    const el = paneRef.current;
    if (!el) return;
    shouldAutoScrollRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 100;
  };

  // After content updates, follow the tail if the user was near the bottom.
  useEffect(() => {
    const el = paneRef.current;
    if (el && shouldAutoScrollRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [raw, filter]);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(raw);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch (err) {
      console.error("Failed to copy logs", err);
    }
  };

  const handleClear = async () => {
    if (!window.confirm("Clear the log file? This cannot be undone.")) return;
    try {
      await invoke("clear_app_logs");
      shouldAutoScrollRef.current = true;
      await load(lineCount);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleReveal = () => {
    void invoke("reveal_app_logs").catch((err) => console.error("Failed to reveal logs", err));
  };

  const q = filter.trim().toLowerCase();
  const allLines = raw.length > 0 ? raw.split("\n") : [];
  const visibleLines = q ? allLines.filter((l) => l.toLowerCase().includes(q)) : allLines;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {/* Header: title + path + actions */}
      <div style={{ display: "flex", alignItems: "baseline", gap: 10, flexWrap: "wrap" }}>
        <span style={{ fontSize: 15, fontWeight: 700, color: "var(--color-text-primary)" }}>
          Logs
        </span>
        <span style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>{LOG_PATH}</span>
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
        <select
          style={selectStyle}
          value={lineCount}
          onChange={(e) => setLineCount(Number(e.target.value))}
          title="How many lines to read from the end of the log"
        >
          {LINE_OPTIONS.map((n) => (
            <option key={n} value={n}>
              Last {n} lines
            </option>
          ))}
        </select>

        <button
          style={{
            ...btnStyle,
            ...(autoRefresh
              ? {
                  borderColor: "var(--color-accent)",
                  color: "var(--color-accent-text, var(--color-accent))",
                  fontWeight: 600,
                }
              : null),
          }}
          onClick={() => setAutoRefresh((v) => !v)}
          aria-pressed={autoRefresh}
        >
          Auto-refresh{autoRefresh ? " · on" : ""}
        </button>

        <button style={btnStyle} onClick={() => void load(lineCount)}>
          Refresh
        </button>

        <button style={btnStyle} onClick={() => void handleCopy()} disabled={raw.length === 0}>
          {copied ? "Copied" : "Copy all"}
        </button>

        <button style={btnStyle} onClick={handleReveal}>
          Reveal in Finder
        </button>

        <button
          style={{ ...btnStyle, color: "var(--color-recording, #ef4444)" }}
          onClick={() => void handleClear()}
        >
          Clear
        </button>
      </div>

      {/* Client-side filter */}
      <input
        style={{
          width: "100%",
          boxSizing: "border-box",
          padding: "7px 12px",
          borderRadius: 999,
          border: "1px solid var(--color-border, rgba(127,127,127,0.25))",
          backgroundColor: "var(--color-background-surface, rgba(127,127,127,0.08))",
          color: "var(--color-text-primary)",
          fontSize: 12.5,
          fontFamily: "inherit",
          outline: "none",
        }}
        placeholder="Filter lines…"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        spellCheck={false}
      />

      {/* Log pane */}
      <div
        ref={paneRef}
        onScroll={handleScroll}
        style={{
          maxHeight: "60vh",
          minHeight: 160,
          overflowY: "auto",
          padding: "10px 12px",
          borderRadius: 10,
          border: "1px solid var(--color-border, rgba(127,127,127,0.25))",
          backgroundColor: "var(--color-background-elevated, rgba(0,0,0,0.15))",
          fontFamily:
            "ui-monospace, SFMono-Regular, Menlo, Monaco, 'Cascadia Mono', monospace",
          fontSize: 11.5,
          lineHeight: 1.55,
          whiteSpace: "pre-wrap",
          overflowWrap: "break-word",
          boxSizing: "border-box",
        }}
      >
        {error ? (
          <span style={{ color: "var(--color-text-secondary)", fontFamily: "inherit" }}>
            Could not read the log file — {error}
          </span>
        ) : visibleLines.length === 0 ? (
          <span style={{ color: "var(--color-text-secondary)" }}>
            {q ? "No lines match the filter." : "The log is empty."}
          </span>
        ) : (
          visibleLines.map((line, i) => (
            <div key={i} style={{ color: lineColor(line) }}>
              {line || " "}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
