import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";

interface DayBucket { label: string; date: string; words: number }
interface AppCount { app: string; count: number }
export interface DictationStats {
  wpm: number;
  today_words: number;
  today_saved_seconds: number;
  today_sessions: number;
  total_words: number;
  total_saved_seconds: number;
  current_streak: number;
  best_streak: number;
  transcriptions: number;
  avg_words: number;
  activity7: DayBucket[];
  activity30: DayBucket[];
  active_days7: number;
  words7: number;
  peak_hour: number | null;
  top_apps: AppCount[];
  ai_enhanced_pct: number;
  longest_words: number;
  most_words_day: number;
  most_transcriptions_day: number;
}

const n = (v: number) => v.toLocaleString();

function fmtDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m`;
  return `${s}s`;
}

function fmtHour(h: number | null): string {
  if (h == null) return "—";
  const to12 = (x: number) => {
    const period = x < 12 ? "AM" : "PM";
    const hr = x % 12 === 0 ? 12 : x % 12;
    return `${hr} ${period}`;
  };
  return `${to12(h)}–${to12((h + 1) % 24)}`;
}

const card: React.CSSProperties = {
  backgroundColor: "var(--color-background-surface)",
  border: "1px solid var(--color-border)",
  borderRadius: 14,
  padding: 20,
};
const eyebrow: React.CSSProperties = {
  fontSize: 11,
  fontWeight: 700,
  letterSpacing: "0.06em",
  textTransform: "uppercase",
  color: "var(--color-text-secondary)",
};

function StatCard({ eyebrowText, value, sub, valueColor }: { eyebrowText: string; value: string; sub?: React.ReactNode; valueColor?: string }) {
  return (
    <div style={card}>
      <Text style={eyebrow}>{eyebrowText}</Text>
      <Text style={{ fontSize: 34, fontWeight: 800, color: valueColor || "var(--color-text-primary)", lineHeight: 1.1, marginTop: 8, display: "block" }}>
        {value}
      </Text>
      {sub ? <div style={{ marginTop: 6, fontSize: 12, color: "var(--color-text-secondary)" }}>{sub}</div> : null}
    </div>
  );
}

function MilestoneRow({ label, achievedCount, items }: { label: string; achievedCount: number; items: { text: string; done: boolean }[] }) {
  return (
    <HStack gap={3} style={{ alignItems: "center", flexWrap: "wrap" }}>
      <Text style={{ width: 96, fontSize: 12, color: "var(--color-text-secondary)" }}>{label}</Text>
      <HStack gap={2} style={{ flexWrap: "wrap" }}>
        {items.map((it) => (
          <span
            key={it.text}
            style={{
              fontSize: 12,
              fontWeight: 600,
              padding: "3px 10px",
              borderRadius: 999,
              border: `1px solid ${it.done ? "var(--color-accent)" : "var(--color-border)"}`,
              backgroundColor: it.done ? "rgba(124,58,237,0.15)" : "transparent",
              color: it.done ? "var(--color-accent-text, var(--color-accent))" : "var(--color-text-secondary)",
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
            }}
          >
            {it.done ? "✓" : "○"} {it.text}
          </span>
        ))}
      </HStack>
      <div style={{ flex: 1 }} />
      <Text style={{ fontSize: 12, color: "var(--color-accent-text, var(--color-accent))" }}>{achievedCount}/{items.length}</Text>
    </HStack>
  );
}

function InsightTile({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ ...card, padding: 14 }}>
      <Text style={{ fontSize: 11, color: "var(--color-text-secondary)" }}>{label}</Text>
      <Text style={{ fontSize: 15, fontWeight: 700, color: "var(--color-text-primary)", marginTop: 4, display: "block" }}>{value}</Text>
    </div>
  );
}

export default function StatsPage() {
  const [stats, setStats] = useState<DictationStats | null>(null);
  const [range, setRange] = useState<"7" | "30">("7");
  const [editingWpm, setEditingWpm] = useState(false);
  const [wpmDraft, setWpmDraft] = useState("40");

  const load = useCallback(async () => {
    try {
      const s = await invoke<DictationStats>("get_dictation_stats");
      setStats(s);
      setWpmDraft(String(s.wpm));
    } catch (err) {
      console.warn("StatsPage: failed to load stats:", err);
    }
  }, []);

  useEffect(() => {
    void load();
    let un: (() => void) | undefined;
    listen("dictation-history-updated", () => void load())
      .then((u) => { un = u; })
      .catch(() => {});
    const onFocus = () => void load();
    window.addEventListener("focus", onFocus);
    const timer = window.setInterval(() => void load(), 15000);
    return () => {
      if (un) un();
      window.removeEventListener("focus", onFocus);
      window.clearInterval(timer);
    };
  }, [load]);

  const saveWpm = async () => {
    const v = parseInt(wpmDraft, 10);
    setEditingWpm(false);
    if (!Number.isFinite(v) || v <= 0) return;
    try {
      await invoke("set_typing_wpm", { wpm: v });
      void load();
    } catch { /* ignore */ }
  };

  const reset = async () => {
    if (!confirm("Reset all stats? This permanently deletes your dictation history.")) return;
    try {
      await invoke("reset_dictation_stats");
      void load();
    } catch { /* ignore */ }
  };

  if (!stats) {
    return (
      <VStack style={{ padding: 24, alignItems: "center" }}>
        <Text style={{ color: "var(--color-text-secondary)" }}>Loading stats…</Text>
      </VStack>
    );
  }

  const s = stats;
  const activity = range === "7" ? s.activity7 : s.activity30;
  const maxWords = Math.max(1, ...activity.map((b) => b.words));
  const rangeTotal = activity.reduce((a, b) => a + b.words, 0);
  const rangeActive = activity.filter((b) => b.words > 0).length;

  const wordsMs = [1000, 10000, 50000, 100000, 500000, 1000000];
  const wordsLabels = ["1K", "10K", "50K", "100K", "500K", "1M"];
  const trMs = [50, 100, 500, 1000, 5000, 10000];
  const trLabels = ["50", "100", "500", "1K", "5K", "10K"];
  const streakMs = [7, 14, 30, 60, 100, 365];
  const streakLabels = ["7 days", "14 days", "30 days", "60 days", "100 days", "1 year"];

  const wordsItems = wordsMs.map((m, i) => ({ text: wordsLabels[i], done: s.total_words >= m }));
  const trItems = trMs.map((m, i) => ({ text: trLabels[i], done: s.transcriptions >= m }));
  const streakItems = streakMs.map((m, i) => ({ text: streakLabels[i], done: s.best_streak >= m }));
  const totalAchieved =
    wordsItems.filter((x) => x.done).length + trItems.filter((x) => x.done).length + streakItems.filter((x) => x.done).length;
  const totalMilestones = wordsItems.length + trItems.length + streakItems.length;

  return (
    <VStack gap={4} style={{ padding: 24, height: "100%", overflowY: "auto" }}>
      {/* Today hero card */}
      <div style={{ ...card, padding: 24, background: "linear-gradient(135deg, var(--color-background-surface), var(--color-background-elevated))" }}>
        <HStack style={{ justifyContent: "space-between", alignItems: "flex-start" }}>
          <VStack gap={1}>
            <Text style={{ fontSize: 28, fontWeight: 800, color: "var(--color-text-primary)" }}>Today</Text>
            <Text style={{ fontSize: 13, color: "var(--color-text-secondary)" }}>
              {s.today_words > 0 ? "On a roll. Keep it going." : "Start dictating to build your streak."}
            </Text>
          </VStack>
          {s.current_streak > 0 && (
            <span style={{ fontSize: 13, fontWeight: 700, color: "#f97316", backgroundColor: "rgba(249,115,22,0.15)", border: "1px solid rgba(249,115,22,0.4)", padding: "4px 12px", borderRadius: 999 }}>
              🔥 {s.current_streak} day{s.current_streak === 1 ? "" : "s"}
            </span>
          )}
        </HStack>
        <HStack gap={6} style={{ marginTop: 18 }}>
          {[
            { v: n(s.today_words), l: "words" },
            { v: fmtDuration(s.today_saved_seconds), l: "saved" },
            { v: n(s.today_sessions), l: "sessions" },
          ].map((x) => (
            <VStack key={x.l} gap={0}>
              <Text style={{ fontSize: 26, fontWeight: 800, color: "var(--color-text-primary)" }}>{x.v}</Text>
              <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>{x.l}</Text>
            </VStack>
          ))}
        </HStack>
      </div>

      {/* Stat cards */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
        <StatCard
          eyebrowText="⏱ Time Saved"
          value={fmtDuration(s.total_saved_seconds)}
          sub={
            editingWpm ? (
              <HStack gap={1} style={{ alignItems: "center" }}>
                Based on
                <input
                  autoFocus
                  value={wpmDraft}
                  onChange={(e) => setWpmDraft(e.target.value.replace(/[^0-9]/g, ""))}
                  onBlur={saveWpm}
                  onKeyDown={(e) => { if (e.key === "Enter") void saveWpm(); }}
                  style={{ width: 44, padding: "2px 6px", borderRadius: 6, border: "1px solid var(--color-border-strong)", background: "var(--color-background-elevated)", color: "var(--color-text-primary)", fontSize: 12 }}
                />
                WPM typing
              </HStack>
            ) : (
              <span onClick={() => setEditingWpm(true)} style={{ cursor: "pointer" }}>
                Based on {s.wpm} WPM typing ✎
              </span>
            )
          }
        />
        <StatCard eyebrowText="📝 Total Words" value={n(s.total_words)} sub={<span style={{ color: "var(--color-accent-text, var(--color-accent))" }}>+{n(s.today_words)} today</span>} />
        <StatCard eyebrowText="🔥 Current Streak" value={`${s.current_streak} days`} valueColor="#f97316" sub={`Best: ${s.best_streak} days`} />
        <StatCard eyebrowText="📄 Transcriptions" value={n(s.transcriptions)} sub={`Avg: ${n(s.avg_words)} words each`} />
      </div>

      {/* Activity chart */}
      <div style={card}>
        <HStack style={{ justifyContent: "space-between", alignItems: "center" }}>
          <Text style={eyebrow}>📊 Activity</Text>
          <HStack gap={0} style={{ border: "1px solid var(--color-border-strong)", borderRadius: 8, overflow: "hidden" }}>
            {(["7", "30"] as const).map((r) => (
              <button
                key={r}
                onClick={() => setRange(r)}
                style={{
                  padding: "5px 12px",
                  border: "none",
                  cursor: "pointer",
                  fontSize: 12,
                  fontWeight: 600,
                  backgroundColor: range === r ? "var(--color-accent)" : "var(--color-background-elevated)",
                  color: range === r ? "#fff" : "var(--color-text-secondary)",
                }}
              >
                {r} days
              </button>
            ))}
          </HStack>
        </HStack>

        <div style={{ display: "flex", alignItems: "flex-end", gap: range === "7" ? 10 : 3, height: 160, marginTop: 20, paddingBottom: 4 }}>
          {activity.map((b, i) => (
            <div key={b.date} style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "flex-end", height: "100%" }}>
              <div
                title={`${b.words} words`}
                style={{
                  width: "100%",
                  maxWidth: range === "7" ? 48 : 18,
                  height: `${(b.words / maxWords) * 100}%`,
                  minHeight: b.words > 0 ? 4 : 2,
                  backgroundColor: b.words > 0 ? "var(--color-accent)" : "var(--color-border)",
                  borderRadius: 6,
                  transition: "height 0.2s ease",
                }}
              />
              {(range === "7" || i % 5 === 0) && (
                <Text style={{ fontSize: 10, color: "var(--color-text-secondary)", marginTop: 6 }}>{b.label}</Text>
              )}
            </div>
          ))}
        </div>
        <Text style={{ fontSize: 12, color: "var(--color-text-secondary)", marginTop: 8 }}>
          <strong style={{ color: "var(--color-text-primary)" }}>{n(rangeTotal)} words</strong> across {rangeActive} active days
        </Text>
      </div>

      {/* Milestones */}
      <div style={card}>
        <HStack style={{ justifyContent: "space-between", alignItems: "center", marginBottom: 14 }}>
          <Text style={eyebrow}>🚩 Milestones</Text>
          <Text style={{ fontSize: 12, color: "var(--color-accent-text, var(--color-accent))" }}>{totalAchieved}/{totalMilestones}</Text>
        </HStack>
        <VStack gap={3}>
          <MilestoneRow label="Words" achievedCount={wordsItems.filter((x) => x.done).length} items={wordsItems} />
          <MilestoneRow label="Transcriptions" achievedCount={trItems.filter((x) => x.done).length} items={trItems} />
          <MilestoneRow label="Streak" achievedCount={streakItems.filter((x) => x.done).length} items={streakItems} />
        </VStack>
      </div>

      {/* Insights */}
      <VStack gap={2}>
        <Text style={eyebrow}>💡 Insights</Text>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
          <InsightTile label="Top Apps" value={s.top_apps.length ? s.top_apps.map((a) => a.app).join(", ") : "—"} />
          <InsightTile label="AI Enhanced" value={`${s.ai_enhanced_pct}%`} />
          <InsightTile label="Peak Time" value={fmtHour(s.peak_hour)} />
          <InsightTile label="Avg Length" value={`${n(s.avg_words)} words`} />
        </div>
      </VStack>

      {/* Personal records */}
      <VStack gap={2}>
        <Text style={eyebrow}>🏆 Personal Records</Text>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 12 }}>
          <InsightTile label="Longest Transcription" value={`${n(s.longest_words)} words`} />
          <InsightTile label="Most Words in a Day" value={n(s.most_words_day)} />
          <InsightTile label="Most in a Day" value={`${n(s.most_transcriptions_day)} transcriptions`} />
        </div>
      </VStack>

      <HStack style={{ justifyContent: "center", paddingTop: 8, paddingBottom: 8 }}>
        <button onClick={reset} style={{ background: "none", border: "none", cursor: "pointer", color: "var(--color-text-secondary)", fontSize: 12 }}>
          🗑 Reset All Stats
        </button>
      </HStack>
    </VStack>
  );
}
