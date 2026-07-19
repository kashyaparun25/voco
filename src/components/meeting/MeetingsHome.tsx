import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DatabaseMeeting } from "./MeetingList";
import GlobalSearch from "../transcript/GlobalSearch";
import AskBar from "./AskBar";

/** Google Calendar event as returned by `list_upcoming_meetings`. */
export interface UpcomingEvent {
  id: string;
  title: string;
  start: string;
  end: string;
  attendees: string[];
}

/** Past meetings shown before the quiet "Show all" toggle expands the rest. */
const RECENT_LIMIT = 15;

interface MeetingsHomeProps {
  meetings: DatabaseMeeting[];
  activeMeetingId: string | null;
  /** Elapsed seconds for the live meeting (shown in the pinned live row). */
  liveSeconds: number;
  upcoming: UpcomingEvent[];
  calendarConnected: boolean;
  /** True while a meeting recording is in progress (disables new-recording actions). */
  recordingBusy: boolean;
  onNewNote: () => void;
  onImport: () => void;
  onSelectMeeting: (id: string) => void;
  onStartUpcoming: (title: string) => void;
  onSelectSearchResult: (meetingId: string, segmentId: string) => void;
}

const PlusIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
  </svg>
);

const DocIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 0 0-3.375-3.375h-1.5A1.125 1.125 0 0 1 13.5 7.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 0 0-9-9Z" />
  </svg>
);

/** Local-date key so events/meetings group by calendar day. */
function dayKey(d: Date): string {
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

function formatClock(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  try {
    return d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  } catch {
    return "";
  }
}

function formatElapsed(totalSecs: number): string {
  const hrs = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;
  if (hrs > 0) return `${hrs}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
  return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
}

/** "Today" / "Yesterday" / "July 15" (adds year when not the current one). */
function relativeDayLabel(d: Date): string {
  const now = new Date();
  const today = dayKey(now);
  const yesterday = dayKey(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1));
  const key = dayKey(d);
  if (key === today) return "Today";
  if (key === yesterday) return "Yesterday";
  try {
    const opts: Intl.DateTimeFormatOptions =
      d.getFullYear() === now.getFullYear()
        ? { month: "long", day: "numeric" }
        : { month: "long", day: "numeric", year: "numeric" };
    return d.toLocaleDateString(undefined, opts);
  } catch {
    return d.toDateString();
  }
}

interface DayGroup {
  key: string;
  date: Date;
  isToday: boolean;
  events: UpcomingEvent[];
}

export default function MeetingsHome({
  meetings,
  activeMeetingId,
  liveSeconds,
  upcoming,
  calendarConnected,
  recordingBusy,
  onNewNote,
  onImport,
  onSelectMeeting,
  onStartUpcoming,
  onSelectSearchResult,
}: MeetingsHomeProps) {
  const [showAllPast, setShowAllPast] = useState(false);

  // Upcoming events grouped by day; today's group always exists (and always
  // renders first) so we can show a "No events today" placeholder.
  const dayGroups = useMemo<DayGroup[]>(() => {
    const groups = new Map<string, DayGroup>();
    const now = new Date();
    const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const todayK = dayKey(todayStart);
    groups.set(todayK, { key: todayK, date: todayStart, isToday: true, events: [] });
    for (const ev of upcoming) {
      const start = new Date(ev.start);
      if (isNaN(start.getTime())) continue;
      const dStart = new Date(start.getFullYear(), start.getMonth(), start.getDate());
      const k = dayKey(dStart);
      const existing = groups.get(k);
      if (existing) existing.events.push(ev);
      else groups.set(k, { key: k, date: dStart, isToday: false, events: [ev] });
    }
    const list = Array.from(groups.values());
    // Today always leads, then the following days in order.
    list.sort((a, b) => (a.isToday ? -1 : b.isToday ? 1 : a.date.getTime() - b.date.getTime()));
    for (const g of list) {
      g.events.sort((a, b) => new Date(a.start).getTime() - new Date(b.start).getTime());
    }
    return list;
  }, [upcoming]);

  // The live meeting is pinned in the upcoming card, so keep it out of
  // the past list below.
  const liveMeeting = activeMeetingId ? meetings.find((m) => m.id === activeMeetingId) : undefined;

  // Past meetings grouped by relative day (newest first), trimmed to the
  // most recent few unless expanded.
  const { meetingGroups, hiddenCount } = useMemo(() => {
    const sorted = meetings
      .filter((m) => m.id !== activeMeetingId)
      .sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
    const visible = showAllPast ? sorted : sorted.slice(0, RECENT_LIMIT);
    const groups: Array<{ label: string; items: DatabaseMeeting[] }> = [];
    for (const m of visible) {
      const d = new Date(m.created_at);
      const label = isNaN(d.getTime()) ? "Earlier" : relativeDayLabel(d);
      const last = groups[groups.length - 1];
      if (last && last.label === label) last.items.push(m);
      else groups.push({ label, items: [m] });
    }
    return { meetingGroups: groups, hiddenCount: sorted.length - visible.length };
  }, [meetings, activeMeetingId, showAllPast]);

  // AI-suggested subtitles (saved by the backend as `ai_title::<id>` after
  // notes generation). Batch-loaded for the visible rows, cached in a map.
  const [aiTitles, setAiTitles] = useState<Record<string, string>>({});
  const aiTitlesRequestedRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    const requested = aiTitlesRequestedRef.current;
    const ids = meetingGroups
      .flatMap((g) => g.items.map((m) => m.id))
      .filter((id) => !requested.has(id));
    if (ids.length === 0) return;
    for (const id of ids) requested.add(id);
    let cancelled = false;
    void Promise.all(
      ids.map(async (id) => {
        try {
          const v = await invoke<string | null>("get_setting", { key: `ai_title::${id}` });
          return [id, v || ""] as const;
        } catch {
          return [id, ""] as const;
        }
      })
    ).then((entries) => {
      if (cancelled) return;
      const found = entries.filter(([, title]) => title);
      if (found.length === 0) return;
      setAiTitles((prev) => {
        const next = { ...prev };
        for (const [id, title] of found) next[id] = title;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
  }, [meetingGroups]);

  // Live update when the backend suggests a title after generation.
  useEffect(() => {
    const unlisten = listen<{ meeting_id: string; title: string }>(
      "meeting-title-suggested",
      (e) => {
        const { meeting_id, title } = e.payload ?? {};
        if (meeting_id && title) {
          setAiTitles((prev) => ({ ...prev, [meeting_id]: title }));
        }
      }
    );
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  return (
    <>
      <div className="mtg-home">
        <div className="mtg-home-inner">
          {/* Top-right actions */}
          <div className="mtg-home-topbar">
            <button className="mtg-btn-quiet" onClick={onImport} disabled={recordingBusy}>
              Import audio
            </button>
            <button className="mtg-btn-primary" onClick={onNewNote} disabled={recordingBusy}>
              <PlusIcon />
              New note
            </button>
          </div>

          <h1 className="mtg-h1 mtg-serif">Coming up</h1>

          {/* Upcoming calendar card — the hero of the page */}
          <div className="mtg-upcoming-card">
            {/* A meeting being recorded right now outranks everything */}
            {activeMeetingId && (
              <button className="mtg-live-row" onClick={() => onSelectMeeting(activeMeetingId)}>
                <span className="mtg-rec-dot" />
                <span className="mtg-live-body">
                  <span className="mtg-live-title">{liveMeeting?.title || "Meeting in progress"}</span>
                  <span className="mtg-live-sub">Recording · {formatElapsed(liveSeconds)}</span>
                </span>
                <span className="mtg-live-open">Open</span>
              </button>
            )}
            {dayGroups.map((group) => {
              const monthName = (() => {
                try { return group.date.toLocaleDateString(undefined, { month: "long" }); } catch { return ""; }
              })();
              const weekday = (() => {
                try { return group.date.toLocaleDateString(undefined, { weekday: "short" }); } catch { return ""; }
              })();
              return (
                <div key={group.key} className="mtg-day">
                  <div className="mtg-day-date">
                    <span className="mtg-day-numeral">
                      {group.date.getDate()}
                      {group.isToday && <span className="mtg-today-dot" />}
                    </span>
                    <span className="mtg-day-meta">
                      <span>{monthName}</span>
                      <span>{weekday}</span>
                    </span>
                  </div>
                  <div className="mtg-day-events">
                    {group.events.length === 0 ? (
                      <div className="mtg-day-empty">
                        {calendarConnected
                          ? "No events today"
                          : "Connect Google Calendar in Settings to see upcoming meetings"}
                      </div>
                    ) : (
                      group.events.map((ev) => (
                        <div key={ev.id} className="mtg-event">
                          <span className="mtg-event-bar" />
                          <div className="mtg-event-body">
                            <div className="mtg-event-title">{ev.title || "Untitled event"}</div>
                            <div className="mtg-event-time">
                              {formatClock(ev.start)}
                              {formatClock(ev.end) ? ` – ${formatClock(ev.end)}` : ""}
                              {ev.attendees.length > 0 ? ` · ${ev.attendees.length + 1} people` : ""}
                            </div>
                          </div>
                          <button
                            className="mtg-event-record"
                            onClick={() => onStartUpcoming(ev.title || "Meeting")}
                            disabled={recordingBusy}
                          >
                            Record
                          </button>
                        </div>
                      ))
                    )}
                  </div>
                </div>
              );
            })}
          </div>

          {/* Search across all transcripts */}
          <GlobalSearch onSelectResult={onSelectSearchResult} />

          {/* Past meetings (secondary) */}
          {meetingGroups.length === 0 ? (
            <div className="mtg-empty-list">
              No meeting notes yet — record a meeting or import an audio file to get started.
            </div>
          ) : (
            <>
              {meetingGroups.map((group) => (
                <div key={group.label}>
                  <div className="mtg-group-header">{group.label}</div>
                  {group.items.map((m) => {
                    const time = (() => {
                      const d = new Date(m.created_at);
                      if (isNaN(d.getTime())) return "";
                      try { return d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" }); } catch { return ""; }
                    })();
                    return (
                      <button key={m.id} className="mtg-row" onClick={() => onSelectMeeting(m.id)}>
                        <span className="mtg-row-icon">
                          <DocIcon />
                        </span>
                        <span className="mtg-row-title">{m.title || "Untitled Meeting"}</span>
                        <span
                          className="mtg-row-sub"
                          style={
                            aiTitles[m.id]
                              ? {
                                  flexShrink: 1,
                                  minWidth: 0,
                                  overflow: "hidden",
                                  textOverflow: "ellipsis",
                                  whiteSpace: "nowrap",
                                }
                              : undefined
                          }
                        >
                          {aiTitles[m.id] || "Me"}
                        </span>
                        <span className="mtg-row-time">{time}</span>
                      </button>
                    );
                  })}
                </div>
              ))}
              {(hiddenCount > 0 || showAllPast) && (
                <button className="mtg-showall" onClick={() => setShowAllPast((v) => !v)}>
                  {showAllPast ? "Show fewer" : `Show all · ${hiddenCount} more`}
                </button>
              )}
            </>
          )}
        </div>
      </div>

      {/* Floating "Ask anything" bar — answers from recent meeting summaries */}
      <AskBar meetingId={null} />
    </>
  );
}
