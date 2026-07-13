import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

function fmtDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m`;
  return `${s}s`;
}

/** Compact "N words · Nm today" pill; click to open the Stats page. */
export default function StatsPill({ onClick }: { onClick: () => void }) {
  const [words, setWords] = useState(0);
  const [saved, setSaved] = useState(0);
  const [sessions, setSessions] = useState(0);

  useEffect(() => {
    const load = async () => {
      try {
        const s = await invoke<{ today_words: number; today_saved_seconds: number; today_sessions: number }>("get_dictation_stats");
        setWords(s.today_words);
        setSaved(s.today_saved_seconds);
        setSessions(s.today_sessions);
      } catch { /* ignore */ }
    };
    void load();
    let un: (() => void) | undefined;
    listen("dictation-history-updated", () => void load()).then((u) => { un = u; }).catch(() => {});
    // Belt-and-suspenders: refresh on window focus and on a light interval so
    // the numbers always reflect recent dictations even if the event is missed.
    const onFocus = () => void load();
    window.addEventListener("focus", onFocus);
    const timer = window.setInterval(() => void load(), 15000);
    return () => {
      if (un) un();
      window.removeEventListener("focus", onFocus);
      window.clearInterval(timer);
    };
  }, []);

  return (
    <button
      onClick={onClick}
      title="View stats"
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 8,
        padding: "6px 14px",
        borderRadius: 999,
        backgroundColor: "var(--color-background-surface-hover)",
        border: "1px solid var(--color-border-strong)",
        color: "var(--color-text-primary)",
        cursor: "pointer",
        fontSize: 12,
        fontWeight: 600,
        whiteSpace: "nowrap",
      }}
    >
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="var(--color-accent)" strokeWidth={2} style={{ width: 14, height: 14 }}>
        <path strokeLinecap="round" d="M4 18V9M9 18V5M14 18v-6M19 18v-9" />
      </svg>
      {words.toLocaleString()} words · {fmtDuration(saved)} · {sessions}×
    </button>
  );
}
