import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DatabaseMeeting } from "./MeetingList";
import { TranscriptSegment } from "../transcript/SegmentCard";
import TranscriptSheet from "./TranscriptSheet";
import AskBar from "./AskBar";
import TemplateGallery, { type CustomTemplate } from "./TemplateGallery";
import {
  parseMarkdown,
  CORE_TEMPLATE_VALUES,
  SUMMARY_TEMPLATES,
  type SummaryLength,
  type SummaryStyle,
} from "./SummaryView";

export type ExportFormat = "txt" | "srt" | "vtt" | "json" | "markdown";

const EXPORT_FORMATS: Array<{ id: ExportFormat; label: string }> = [
  { id: "markdown", label: "Markdown (.md)" },
  { id: "txt", label: "Plain Text (.txt)" },
  { id: "srt", label: "Subtitles (.srt)" },
  { id: "vtt", label: "WebVTT (.vtt)" },
  { id: "json", label: "JSON (.json)" },
];

/** Unified row model for the template picker (built-ins + customs). */
interface TemplateEntry {
  value: string;
  label: string;
  emoji: string;
  custom?: CustomTemplate;
}

function parseJsonSetting<T>(raw: string | null | undefined): T | null {
  if (!raw) return null;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

const truncateLabel = (s: string, max = 14) =>
  s.length > max ? `${s.slice(0, max).trimEnd()}…` : s;

/** "Friday, Jul 18 · 2:30 PM" line for the attendees popover. */
function attendeeDateLine(iso: string | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  try {
    const day = d.toLocaleDateString(undefined, { weekday: "long", month: "short", day: "numeric" });
    const time = d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
    return `${day} · ${time}`;
  } catch {
    return d.toDateString();
  }
}

const TPL_MENU_WIDTH = 340;
const ATT_POP_WIDTH = 360;

/** Critical box styles for the template picker. Inline so no cascade, @layer
 *  or global stylesheet rule can break them (the menu previously rendered
 *  transparent/unpositioned when app-level CSS beat the layered rules). */
const tplMenuBaseStyle: CSSProperties = {
  position: "fixed",
  width: TPL_MENU_WIDTH,
  maxHeight: "min(560px, 72vh)",
  overflowY: "auto",
  zIndex: 1000,
  borderRadius: 14,
  boxSizing: "border-box",
  background: "var(--color-background-elevated, #1e1e2e)",
  backdropFilter: "blur(24px)",
  WebkitBackdropFilter: "blur(24px)",
  border: "1px solid var(--color-border-strong, rgba(255,255,255,0.12))",
  boxShadow: "0 12px 32px rgba(0, 0, 0, 0.35)",
  padding: 6,
  margin: 0,
  display: "flex",
  flexDirection: "column",
};

/** Critical box styles for the attendees/speakers popover — same hardening
 *  rules as the template picker (inline, portal, fixed positioning). */
const attPopBaseStyle: CSSProperties = {
  position: "fixed",
  width: ATT_POP_WIDTH,
  maxHeight: "min(480px, 70vh)",
  overflowY: "auto",
  zIndex: 1000,
  borderRadius: 14,
  boxSizing: "border-box",
  background: "var(--color-background-elevated, #1e1e2e)",
  backdropFilter: "blur(24px)",
  WebkitBackdropFilter: "blur(24px)",
  border: "1px solid var(--color-border-strong, rgba(255,255,255,0.12))",
  boxShadow: "0 12px 32px rgba(0, 0, 0, 0.35)",
  padding: 6,
  margin: 0,
  display: "flex",
  flexDirection: "column",
};

interface MeetingNotePageProps {
  meeting: DatabaseMeeting | undefined;
  meetingId: string;
  /** True when this meeting is the actively recording one. */
  isActive: boolean;
  isPaused: boolean;
  /** Elapsed seconds for the live meeting. */
  seconds: number;
  segments: TranscriptSegment[];
  summary: string | null;
  summaryLoading: boolean;
  streamingText?: string;
  isStreaming?: boolean;
  isDiarizing: boolean;
  diarizationTurns: number | null;
  hasRecording: boolean;
  onBack: () => void;
  onStop: () => void;
  onPause: () => void;
  onResume: () => void;
  onReprocess: () => void;
  onDelete: () => void;
  onExport: (format: ExportFormat) => void;
  onGenerate: (length: SummaryLength, style: SummaryStyle) => void;
  onRegenerate: (length: SummaryLength, style: SummaryStyle) => void;
  onRenameSpeaker: (speakerId: string, newName: string) => void;
  /** Called after rename_meeting succeeds — parent refetches the list. */
  onRenamed: () => void;
  /** Called after set_meeting_summary succeeds — parent updates its state. */
  onSummarySaved: (summary: string) => void;
  /** From global search — opens the transcript sheet at this segment. */
  scrollToSegmentId?: string | null;
  onScrolledToSegment?: () => void;
}

/* ── Icons ─────────────────────────────────────────────────────────── */

const BackIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 16, height: 16 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M10.5 19.5 3 12m0 0 7.5-7.5M3 12h18" />
  </svg>
);

const DotsIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 24 24" style={{ width: 16, height: 16 }}>
    <circle cx="5" cy="12" r="1.8" />
    <circle cx="12" cy="12" r="1.8" />
    <circle cx="19" cy="12" r="1.8" />
  </svg>
);

const ChevronDownIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 11, height: 11 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m19.5 8.25-7.5 7.5-7.5-7.5" />
  </svg>
);

const WaveIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 15, height: 15 }}>
    <path strokeLinecap="round" d="M4 10v4m4-8v12m4-10v8m4-11v14m4-10v6" />
  </svg>
);

const SparklesIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M9.813 15.904 9 18.75l-.813-2.846a4.5 4.5 0 0 0-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 0 0 3.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 0 0 3.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 0 0-3.09 3.09ZM18.259 8.715 18 9.75l-.259-1.035a3.375 3.375 0 0 0-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 0 0 2.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 0 0 2.456 2.456L21.75 6l-1.035.259a3.375 3.375 0 0 0-2.456 2.456Z" />
  </svg>
);

const RefreshIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0 3.181 3.183a8.25 8.25 0 0 0 13.803-3.7M4.031 9.865a8.25 8.25 0 0 1 13.803-3.7l3.181 3.182m0-4.991v4.99" />
  </svg>
);

const StarIcon = ({ filled }: { filled: boolean }) => (
  <svg xmlns="http://www.w3.org/2000/svg" fill={filled ? "currentColor" : "none"} viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M11.48 3.499a.562.562 0 0 1 1.04 0l2.125 5.111a.563.563 0 0 0 .475.345l5.518.442c.499.04.701.663.321.988l-4.204 3.602a.563.563 0 0 0-.182.557l1.285 5.385a.562.562 0 0 1-.84.61l-4.725-2.885a.562.562 0 0 0-.586 0L6.982 20.54a.562.562 0 0 1-.84-.61l1.285-5.386a.562.562 0 0 0-.182-.557l-4.204-3.602a.562.562 0 0 1 .321-.988l5.518-.442a.563.563 0 0 0 .475-.345L11.48 3.5Z" />
  </svg>
);

const TrashIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
  </svg>
);

const CheckIcon = ({ size = 14 }: { size?: number }) => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2.2} stroke="currentColor" style={{ width: size, height: size }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
  </svg>
);

const PlusIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
  </svg>
);

const CalendarIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 0 1 2.25-2.25h13.5A2.25 2.25 0 0 1 21 7.5v11.25m-18 0A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75m-18 0v-7.5A2.25 2.25 0 0 1 5.25 9h13.5A2.25 2.25 0 0 1 21 11.25v7.5" />
  </svg>
);

const UsersIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M15 19.128a9.38 9.38 0 0 0 2.625.372 9.337 9.337 0 0 0 4.121-.952 4.125 4.125 0 0 0-7.533-2.493M15 19.128v-.003c0-1.113-.285-2.16-.786-3.07M15 19.128v.106A12.318 12.318 0 0 1 8.624 21c-2.331 0-4.512-.645-6.374-1.766l-.001-.109a6.375 6.375 0 0 1 11.964-3.07M12 6.375a3.375 3.375 0 1 1-6.75 0 3.375 3.375 0 0 1 6.75 0Zm8.25 2.25a2.625 2.625 0 1 1-5.25 0 2.625 2.625 0 0 1 5.25 0Z" />
  </svg>
);

const PencilIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 12, height: 12 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m16.862 4.487 1.687-1.688a1.875 1.875 0 1 1 2.652 2.652L10.582 16.07a4.5 4.5 0 0 1-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 0 1 1.13-1.897l8.932-8.931Zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0 1 15.75 21H5.25A2.25 2.25 0 0 1 3 18.75V8.25A2.25 2.25 0 0 1 5.25 6H10" />
  </svg>
);

const GridIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M3.75 6A2.25 2.25 0 0 1 6 3.75h2.25A2.25 2.25 0 0 1 10.5 6v2.25a2.25 2.25 0 0 1-2.25 2.25H6a2.25 2.25 0 0 1-2.25-2.25V6ZM3.75 15.75A2.25 2.25 0 0 1 6 13.5h2.25a2.25 2.25 0 0 1 2.25 2.25V18a2.25 2.25 0 0 1-2.25 2.25H6A2.25 2.25 0 0 1 3.75 18v-2.25ZM13.5 6a2.25 2.25 0 0 1 2.25-2.25H18A2.25 2.25 0 0 1 20.25 6v2.25A2.25 2.25 0 0 1 18 10.5h-2.25a2.25 2.25 0 0 1-2.25-2.25V6ZM13.5 15.75a2.25 2.25 0 0 1 2.25-2.25H18a2.25 2.25 0 0 1 2.25 2.25V18A2.25 2.25 0 0 1 18 20.25h-2.25A2.25 2.25 0 0 1 13.5 18v-2.25Z" />
  </svg>
);

const ListIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M3.75 6.75h16.5M3.75 12h16.5M3.75 17.25h16.5" />
  </svg>
);

const DownloadIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75V16.5M16.5 12 12 16.5m0 0L7.5 12m4.5 4.5V3" />
  </svg>
);

const Spinner = ({ size = 12 }: { size?: number }) => (
  <span
    style={{
      width: size,
      height: size,
      borderRadius: "50%",
      border: "2px solid currentColor",
      borderTopColor: "transparent",
      display: "inline-block",
      animation: "voco-spin 0.7s linear infinite",
      flexShrink: 0,
    }}
  />
);

/* ── Helpers ───────────────────────────────────────────────────────── */

function noteDateLabel(iso: string | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  const now = new Date();
  const startOf = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round((startOf(now) - startOf(d)) / 86400000);
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  try {
    const opts: Intl.DateTimeFormatOptions =
      d.getFullYear() === now.getFullYear()
        ? { month: "short", day: "numeric" }
        : { month: "short", day: "numeric", year: "numeric" };
    return d.toLocaleDateString(undefined, opts);
  } catch {
    return d.toDateString();
  }
}

function formatElapsed(totalSecs: number): string {
  const hrs = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;
  if (hrs > 0) return `${hrs}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
  return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
}

/** Generic click-away dropdown used by the chips, floating bar and overflow menu. */
function Menu({
  open,
  onClose,
  direction,
  children,
}: {
  open: boolean;
  onClose: () => void;
  direction: "down" | "down-right" | "up";
  children: ReactNode;
}) {
  if (!open) return null;
  const cls =
    direction === "up" ? "mtg-menu-up" : direction === "down-right" ? "mtg-menu-down-right" : "mtg-menu-down";
  return (
    <>
      <div className="mtg-menu-backdrop" onClick={onClose} />
      <div className={`mtg-menu ${cls}`}>{children}</div>
    </>
  );
}

/* ── Component ─────────────────────────────────────────────────────── */

export default function MeetingNotePage({
  meeting,
  meetingId,
  isActive,
  isPaused,
  seconds,
  segments,
  summary,
  summaryLoading,
  streamingText,
  isStreaming,
  isDiarizing,
  diarizationTurns,
  hasRecording,
  onBack,
  onStop,
  onPause,
  onResume,
  onReprocess,
  onDelete,
  onExport,
  onGenerate,
  onRegenerate,
  onRenameSpeaker,
  onRenamed,
  onSummarySaved,
  scrollToSegmentId,
  onScrolledToSegment,
}: MeetingNotePageProps) {
  const [sheetOpen, setSheetOpen] = useState(false);
  const [templateOpen, setTemplateOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [overflowOpen, setOverflowOpen] = useState(false);

  // Template picker anchoring: position: fixed with coordinates measured from
  // the Enhanced segment at open time — no positioned-ancestor dependence.
  const segCellRef = useRef<HTMLDivElement>(null);
  const [tplPos, setTplPos] = useState<{ top: number; left: number }>({ top: 80, left: 24 });

  const measureTplPos = () => {
    const r = segCellRef.current?.getBoundingClientRect();
    if (!r) return;
    setTplPos({
      top: r.bottom + 6,
      left: Math.max(12, Math.min(r.left, window.innerWidth - TPL_MENU_WIDTH - 12)),
    });
  };

  const toggleTemplateMenu = () => {
    if (templateOpen) {
      setTemplateOpen(false);
      return;
    }
    measureTplPos();
    setTemplateOpen(true);
  };

  // Keep the fixed menu glued to the segment across window resizes.
  useEffect(() => {
    if (!templateOpen) return;
    const update = () => measureTplPos();
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, [templateOpen]);

  // Generation options (template is persisted; backend reads it at generate time).
  const [template, setTemplate] = useState("general");
  const [length, setLength] = useState<SummaryLength>("medium");
  const [style, setStyle] = useState<SummaryStyle>("bullets");

  // Custom templates + favorites (both persisted as JSON-encoded settings).
  const [customTemplates, setCustomTemplates] = useState<CustomTemplate[]>([]);
  const [favorites, setFavorites] = useState<string[]>([]);

  // Full template gallery ("All templates…").
  const [galleryOpen, setGalleryOpen] = useState(false);

  // "New / edit template" modal. `editingTplId` set = editing an existing
  // custom template; null = creating a new one.
  const [newTplOpen, setNewTplOpen] = useState(false);
  const [editingTplId, setEditingTplId] = useState<string | null>(null);
  const [tplName, setTplName] = useState("");
  const [tplEmoji, setTplEmoji] = useState("📝");
  const [tplInstructions, setTplInstructions] = useState("");

  const busy = summaryLoading || !!isStreaming;
  const showStreaming = busy && !!streamingText && streamingText.length > 0;

  // "My notes" ↔ "Enhanced" segmented view.
  const [view, setView] = useState<"mine" | "enhanced">("mine");
  const [myNotes, setMyNotes] = useState("");
  const notesRef = useRef<HTMLTextAreaElement>(null);
  const saveTimerRef = useRef<number | undefined>(undefined);
  const pendingSaveRef = useRef<{ key: string; value: string } | null>(null);
  const notesLoadedRef = useRef(false);

  // Default view per meeting: the user's own notes until an AI summary
  // exists (or is being written) — then the Enhanced view.
  useEffect(() => {
    setView(summary || busy ? "enhanced" : "mine");
    // Only re-evaluate when switching meetings; generation flips it explicitly.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meetingId]);

  /* ── Inline title editing ─────────────────────────────────────────── */
  const [titleEditing, setTitleEditing] = useState(false);
  const [titleValue, setTitleValue] = useState("");
  const [titleSuggestions, setTitleSuggestions] = useState<string[]>([]);
  const [titleSuggestLoading, setTitleSuggestLoading] = useState(false);
  const [titleError, setTitleError] = useState<string | null>(null);
  const titleTaRef = useRef<HTMLTextAreaElement>(null);

  /* ── AI-notes editing ─────────────────────────────────────────────── */
  const [notesEditing, setNotesEditing] = useState(false);
  const [notesDraft, setNotesDraft] = useState("");
  const [notesSaving, setNotesSaving] = useState(false);
  const notesTaRef = useRef<HTMLTextAreaElement>(null);
  // True once the user saved manual edits to this meeting's notes in this
  // session — regenerating then asks for confirmation before overwriting.
  const handEditedRef = useRef(false);

  // Both editors auto-exit when switching meetings…
  useEffect(() => {
    setTitleEditing(false);
    setNotesEditing(false);
    setTitleError(null);
    handEditedRef.current = false;
  }, [meetingId]);

  // …and the notes editor also exits when leaving the Enhanced view.
  useEffect(() => {
    if (view !== "enhanced") setNotesEditing(false);
  }, [view]);

  const startTitleEdit = () => {
    if (titleEditing) return;
    setTitleValue(meeting?.title || "");
    setTitleError(null);
    setTitleSuggestions([]);
    setTitleEditing(true);
    // Calendar event titles around the meeting time as suggestions
    // (fails soft to [] when Google Calendar isn't connected).
    if (meeting?.created_at) {
      const current = (meeting.title || "").trim().toLowerCase();
      invoke<string[]>("list_event_titles_around", { when: meeting.created_at })
        .then((titles) => {
          const seen = new Set<string>();
          const out: string[] = [];
          for (const raw of Array.isArray(titles) ? titles : []) {
            const t = raw.trim();
            const k = t.toLowerCase();
            if (!t || k === current || seen.has(k)) continue;
            seen.add(k);
            out.push(t);
          }
          setTitleSuggestions(out.slice(0, 6));
        })
        .catch(() => {});
    }
  };

  const commitTitle = async () => {
    const t = titleValue.trim();
    setTitleEditing(false);
    setTitleError(null);
    if (!t || t === (meeting?.title || "")) return;
    try {
      await invoke("rename_meeting", { meetingId, title: t });
      onRenamed();
    } catch (err) {
      console.error("Failed to rename meeting", err);
    }
  };

  const suggestTitleWithAi = async () => {
    if (titleSuggestLoading) return;
    setTitleSuggestLoading(true);
    setTitleError(null);
    try {
      const t = await invoke<string>("suggest_meeting_title", { meetingId });
      // Fill the input only — the user still commits with Enter/blur.
      if (t && t.trim()) setTitleValue(t.trim());
    } catch (err) {
      setTitleError(String(err));
    } finally {
      setTitleSuggestLoading(false);
    }
  };

  // Auto-grow the borderless title editor to fit its content.
  useEffect(() => {
    const el = titleTaRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = `${el.scrollHeight}px`;
    }
  }, [titleValue, titleEditing]);

  const startNotesEdit = () => {
    if (busy || !summary || notesEditing) return;
    setNotesDraft(summary);
    setView("enhanced");
    setNotesEditing(true);
  };

  const cancelNotesEdit = () => setNotesEditing(false);

  const saveNotesEdit = async () => {
    if (notesSaving) return;
    setNotesSaving(true);
    try {
      await invoke("set_meeting_summary", { meetingId, summary: notesDraft });
      onSummarySaved(notesDraft);
      handEditedRef.current = true;
      setNotesEditing(false);
    } catch (err) {
      console.error("Failed to save edited notes", err);
    } finally {
      setNotesSaving(false);
    }
  };

  // Auto-grow the notes editor to fit its content.
  useEffect(() => {
    const el = notesTaRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = `${el.scrollHeight}px`;
    }
  }, [notesDraft, notesEditing]);

  const flushNotesSave = () => {
    window.clearTimeout(saveTimerRef.current);
    const pending = pendingSaveRef.current;
    pendingSaveRef.current = null;
    if (pending) {
      void invoke("set_setting", { key: pending.key, value: pending.value }).catch(() => {});
    }
  };

  // Load the user's typed notes for this meeting; save any unsaved edits
  // for the previous meeting first.
  useEffect(() => {
    notesLoadedRef.current = false;
    setMyNotes("");
    let cancelled = false;
    (async () => {
      try {
        const v = await invoke<string | null>("get_setting", { key: `meeting_notes::${meetingId}` });
        if (!cancelled) setMyNotes(v || "");
      } catch { /* first time — no notes yet */ }
      if (!cancelled) notesLoadedRef.current = true;
    })();
    return () => {
      cancelled = true;
      flushNotesSave();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meetingId]);

  const handleNotesChange = (value: string) => {
    setMyNotes(value);
    if (!notesLoadedRef.current) return;
    pendingSaveRef.current = { key: `meeting_notes::${meetingId}`, value };
    window.clearTimeout(saveTimerRef.current);
    saveTimerRef.current = window.setTimeout(flushNotesSave, 500);
  };

  // Auto-grow the borderless textarea to fit its content.
  useEffect(() => {
    const el = notesRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = `${el.scrollHeight}px`;
    }
  }, [myNotes, view]);

  useEffect(() => {
    (async () => {
      try {
        const t = await invoke<string | null>("get_setting", { key: "summary_template" });
        if (t) setTemplate(t);
      } catch { /* ignore */ }
      try {
        const raw = await invoke<string | null>("get_setting", { key: "custom_templates" });
        const list = parseJsonSetting<CustomTemplate[]>(raw);
        if (Array.isArray(list)) setCustomTemplates(list);
      } catch { /* first run — nothing saved yet */ }
      try {
        const raw = await invoke<string | null>("get_setting", { key: "favorite_templates" });
        const list = parseJsonSetting<string[]>(raw);
        if (Array.isArray(list)) setFavorites(list);
      } catch { /* first run — nothing saved yet */ }
    })();
  }, []);

  const changeTemplate = (value: string) => {
    setTemplate(value);
    void invoke("set_setting", { key: "summary_template", value }).catch(() => {});
  };

  const saveCustomTemplates = (list: CustomTemplate[]) => {
    setCustomTemplates(list);
    void invoke("set_setting", { key: "custom_templates", value: JSON.stringify(list) }).catch(() => {});
  };

  const saveFavorites = (list: string[]) => {
    setFavorites(list);
    void invoke("set_setting", { key: "favorite_templates", value: JSON.stringify(list) }).catch(() => {});
  };

  const toggleFavorite = (value: string) => {
    saveFavorites(
      favorites.includes(value) ? favorites.filter((f) => f !== value) : [...favorites, value]
    );
  };

  // A search hit targets a transcript segment — surface the sheet.
  useEffect(() => {
    if (scrollToSegmentId) setSheetOpen(true);
  }, [scrollToSegmentId]);

  // AI-suggested descriptive subtitle: saved by the backend after notes
  // generation (setting `ai_title::<id>`, event "meeting-title-suggested").
  const [aiSubtitle, setAiSubtitle] = useState<string | null>(null);
  useEffect(() => {
    setAiSubtitle(null);
    let cancelled = false;
    (async () => {
      try {
        const v = await invoke<string | null>("get_setting", { key: `ai_title::${meetingId}` });
        if (!cancelled && v) setAiSubtitle(v);
      } catch { /* none yet */ }
    })();
    const unlisten = listen<{ meeting_id: string; title: string }>(
      "meeting-title-suggested",
      (e) => {
        if (!cancelled && e.payload?.meeting_id === meetingId && e.payload.title) {
          setAiSubtitle(e.payload.title);
        }
      }
    );
    return () => {
      cancelled = true;
      void unlisten.then((f) => f());
    };
  }, [meetingId]);

  // Kicking off generation always lands the user on the Enhanced view.
  const handleGenerate = () => {
    setView("enhanced");
    onGenerate(length, style);
  };
  const handleRegenerate = () => {
    // Only guard when the current summary was hand-edited this session.
    if (
      handEditedRef.current &&
      !window.confirm("Regenerating will overwrite your manual edits. Continue?")
    ) {
      return;
    }
    handEditedRef.current = false;
    setNotesEditing(false);
    setView("enhanced");
    onRegenerate(length, style);
  };

  // Built-ins + customs as one list; favorites float into their own section.
  const allTemplates: TemplateEntry[] = [
    ...SUMMARY_TEMPLATES,
    ...customTemplates.map((c) => ({
      value: `custom:${c.id}`,
      label: c.name,
      emoji: c.emoji || "📝",
      custom: c,
    })),
  ];
  const favoriteEntries = allTemplates.filter((t) => favorites.includes(t.value));
  // The compact dropdown lists only the core built-ins plus the current
  // selection; everything else is reachable through the gallery.
  const regularEntries = allTemplates.filter(
    (t) =>
      (CORE_TEMPLATE_VALUES.includes(t.value) || t.value === template) &&
      !favorites.includes(t.value)
  );

  const selectedEntry = allTemplates.find((t) => t.value === template);
  const templateLabel = selectedEntry?.label ?? "Enhanced";
  const enhancedLabel =
    template === "general" || !selectedEntry ? "Enhanced" : truncateLabel(selectedEntry.label);

  // Selecting a template persists it; if notes already exist, regenerate so
  // the new structure applies immediately. Shared by the dropdown rows and
  // the gallery cards.
  const selectTemplate = (value: string) => {
    changeTemplate(value);
    setTemplateOpen(false);
    setGalleryOpen(false);
    if (summary && !busy && segments.length > 0) handleRegenerate();
  };

  const deleteCustomTemplate = (c: CustomTemplate) => {
    if (!window.confirm(`Delete the “${c.name}” template?`)) return;
    const value = `custom:${c.id}`;
    saveCustomTemplates(customTemplates.filter((x) => x.id !== c.id));
    if (favorites.includes(value)) saveFavorites(favorites.filter((f) => f !== value));
    if (template === value) changeTemplate("general");
  };

  const openNewTemplateModal = () => {
    setEditingTplId(null);
    setTplName("");
    setTplEmoji("📝");
    setTplInstructions("");
    setTemplateOpen(false);
    setNewTplOpen(true);
  };

  const openEditTemplateModal = (c: CustomTemplate) => {
    setEditingTplId(c.id);
    setTplName(c.name);
    setTplEmoji(c.emoji || "📝");
    setTplInstructions(c.instructions);
    setTemplateOpen(false);
    setNewTplOpen(true);
  };

  const openGallery = () => {
    setTemplateOpen(false);
    setOverflowOpen(false);
    setGalleryOpen(true);
  };

  const saveNewTemplate = () => {
    const name = tplName.trim();
    if (!name) return;
    const emoji = tplEmoji.trim() || "📝";
    const instructions = tplInstructions.trim();
    if (editingTplId) {
      saveCustomTemplates(
        customTemplates.map((x) =>
          x.id === editingTplId ? { ...x, name, emoji, instructions } : x
        )
      );
    } else {
      const tpl: CustomTemplate = { id: crypto.randomUUID(), name, emoji, instructions };
      saveCustomTemplates([...customTemplates, tpl]);
      changeTemplate(`custom:${tpl.id}`);
    }
    setNewTplOpen(false);
  };

  // Escape closes the topmost layer: modal → gallery → dropdown.
  useEffect(() => {
    if (!templateOpen && !newTplOpen && !galleryOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      if (newTplOpen) setNewTplOpen(false);
      else if (galleryOpen) setGalleryOpen(false);
      else setTemplateOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [templateOpen, newTplOpen, galleryOpen]);

  /* ── Attendees / speakers popover ─────────────────────────────────── */

  // Unique speakers from the transcript, ordered by first appearance.
  const speakers = useMemo(() => {
    const seen = new Map<string, string>();
    for (const seg of segments) {
      if (seg.speaker_id && !seen.has(seg.speaker_id)) {
        seen.set(seg.speaker_id, seg.speaker_name || "Unknown speaker");
      }
    }
    return Array.from(seen, ([id, name]) => ({ id, name }));
  }, [segments]);

  const attChipRef = useRef<HTMLButtonElement>(null);
  const [attOpen, setAttOpen] = useState(false);
  const [attPos, setAttPos] = useState<{ top: number; left: number }>({ top: 80, left: 24 });
  // Calendar attendee names as rename suggestions; null = still loading.
  const [attSuggestions, setAttSuggestions] = useState<string[] | null>(null);
  const [editingSpeakerId, setEditingSpeakerId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  // ≥1 rename committed in this popover session → regenerate notes on close.
  const renamedRef = useRef(false);

  const measureAttPos = () => {
    const r = attChipRef.current?.getBoundingClientRect();
    if (!r) return;
    setAttPos({
      top: r.bottom + 6,
      left: Math.max(12, Math.min(r.left, window.innerWidth - ATT_POP_WIDTH - 12)),
    });
  };

  const openAttendees = () => {
    measureAttPos();
    renamedRef.current = false;
    setEditingSpeakerId(null);
    setAttOpen(true);
    // Fetch calendar attendee suggestions once per popover session. Fails
    // soft: no calendar connected → empty list → no suggestion area.
    setAttSuggestions(null);
    if (meeting?.created_at) {
      invoke<string[]>("list_event_attendees_around", { when: meeting.created_at })
        .then((names) => setAttSuggestions(Array.isArray(names) ? names : []))
        .catch(() => setAttSuggestions([]));
    } else {
      setAttSuggestions([]);
    }
  };

  const closeAttendees = () => {
    setAttOpen(false);
    setEditingSpeakerId(null);
    // Renames landed while open → refresh the AI notes so they stop saying
    // "Speaker 1". Only when notes already exist; never during generation.
    if (renamedRef.current && summary && !busy && segments.length > 0) {
      handleRegenerate();
    }
    renamedRef.current = false;
  };

  const toggleAttendees = () => {
    if (attOpen) closeAttendees();
    else openAttendees();
  };

  const startSpeakerEdit = (id: string, currentName: string) => {
    setEditingSpeakerId(id);
    setEditValue(currentName);
  };

  const commitSpeakerRename = (speakerId: string, raw: string) => {
    const name = raw.trim();
    const current = speakers.find((s) => s.id === speakerId)?.name ?? "";
    setEditingSpeakerId(null);
    if (!name || name === current) return;
    // Existing rename flow: parent invokes rename_speaker + reloads transcript.
    onRenameSpeaker(speakerId, name);
    renamedRef.current = true;
  };

  // Keep the fixed popover glued to the chip across window resizes; Escape
  // closes it (the inline editor's own Escape stops propagation first).
  useEffect(() => {
    if (!attOpen) return;
    const update = () => measureAttPos();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      closeAttendees();
    };
    window.addEventListener("resize", update);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("keydown", onKey);
    };
    // Re-attach on state the close handler reads, so it never runs stale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attOpen, summary, busy, segments.length]);

  const renderBody = () => {
    if (showStreaming) {
      return (
        <div className="mtg-note-body">
          {parseMarkdown(streamingText!)}
          <span className="mtg-cursor" />
        </div>
      );
    }
    if (busy) {
      return (
        <div className="mtg-note-body" style={{ paddingTop: 8 }}>
          <div className="mtg-skeleton" style={{ width: "85%" }} />
          <div className="mtg-skeleton" style={{ width: "95%" }} />
          <div className="mtg-skeleton" style={{ width: "70%" }} />
          <div className="mtg-skeleton" style={{ width: "40%" }} />
        </div>
      );
    }
    if (summary) {
      if (notesEditing) {
        return (
          <div className="mtg-notes-editor">
            <textarea
              ref={notesTaRef}
              className="mtg-notes-edit-ta"
              value={notesDraft}
              onChange={(e) => setNotesDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.stopPropagation();
                  cancelNotesEdit();
                }
              }}
              autoFocus
              spellCheck={false}
            />
            <div className="mtg-notes-editbar">
              <button className="mtg-btn-quiet" onClick={cancelNotesEdit} disabled={notesSaving}>
                Cancel
              </button>
              <button className="mtg-btn-primary" onClick={() => void saveNotesEdit()} disabled={notesSaving}>
                {notesSaving ? "Saving…" : "Save"}
              </button>
            </div>
          </div>
        );
      }
      return (
        <div className="mtg-note-bodywrap">
          <button className="mtg-notes-editbtn" title="Edit notes" onClick={startNotesEdit}>
            <PencilIcon />
          </button>
          <div className="mtg-note-body">{parseMarkdown(summary)}</div>
        </div>
      );
    }
    if (isActive) {
      return (
        <div className="mtg-ghost">
          <span>Notes will appear here after the meeting — open the transcript below to follow along.</span>
        </div>
      );
    }
    return (
      <div className="mtg-ghost">
        <span className="mtg-ghost-title mtg-serif">No notes yet</span>
        <span style={{ fontSize: 13, maxWidth: 380, lineHeight: 1.5 }}>
          Generate AI notes from the transcript — key decisions, action items and takeaways.
        </span>
        <button
          className="mtg-btn-primary"
          style={{ marginTop: 4 }}
          onClick={handleGenerate}
          disabled={segments.length === 0}
        >
          <SparklesIcon />
          Generate notes
        </button>
        {segments.length === 0 && (
          <span style={{ fontSize: 12 }}>This meeting has no transcript to summarize.</span>
        )}
      </div>
    );
  };

  // One row of the template picker. A div with role="button" — rows contain
  // nested interactive controls (star / trash), which <button> can't hold.
  const templateRow = (t: TemplateEntry) => {
    const selected = t.value === template;
    const fav = favorites.includes(t.value);
    return (
      <div
        key={t.value}
        className="mtg-tpl-row"
        role="button"
        tabIndex={0}
        onClick={() => selectTemplate(t.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            selectTemplate(t.value);
          }
        }}
      >
        <span className="mtg-tpl-emoji" aria-hidden="true">{t.emoji}</span>
        <span className="mtg-tpl-name">{t.label}</span>
        <span className="mtg-tpl-row-actions">
          {t.custom && (
            <button
              className="mtg-tpl-rowbtn mtg-tpl-trash"
              title="Delete template"
              onClick={(e) => {
                e.stopPropagation();
                deleteCustomTemplate(t.custom!);
              }}
            >
              <TrashIcon />
            </button>
          )}
          <button
            className={`mtg-tpl-rowbtn mtg-tpl-star${fav ? " mtg-tpl-star-on" : ""}`}
            title={fav ? "Remove from favorites" : "Add to favorites"}
            aria-pressed={fav}
            onClick={(e) => {
              e.stopPropagation();
              toggleFavorite(t.value);
            }}
          >
            <StarIcon filled={fav} />
          </button>
          {selected && (
            <span className="mtg-tpl-check" aria-label="Selected">
              <CheckIcon />
            </span>
          )}
        </span>
      </div>
    );
  };

  // Rendered through a portal: position: fixed inside the app tree can still
  // be hijacked by an ancestor with transform/filter, and clipped by
  // overflow — the portal removes both failure modes. Target #voco-theme-root
  // (display: contents, generates no box) rather than body so the theme's
  // CSS variables still resolve on the menu.
  const templateMenu = (openState: boolean, close: () => void) => {
    if (!openState) return null;
    return createPortal(
      <>
        <div
          style={{ position: "fixed", inset: 0, zIndex: 999 }}
          onClick={close}
        />
        <div
          className="mtg-tpl-menu"
          style={{ ...tplMenuBaseStyle, top: tplPos.top, left: tplPos.left }}
          role="menu"
          aria-label="Enhanced notes templates"
        >
          {/* Header: sparkle + title + regenerate / active check */}
          <div className="mtg-tpl-head">
            <SparklesIcon />
            <span className="mtg-tpl-head-title">Enhanced notes</span>
            <span className="mtg-tpl-head-actions">
              <button
                className={`mtg-tpl-rowbtn mtg-tpl-regen${busy ? " mtg-tpl-regen-spin" : ""}`}
                title="Regenerate notes"
                disabled={busy || segments.length === 0 || !summary}
                onClick={handleRegenerate}
              >
                <RefreshIcon />
              </button>
              {view === "enhanced" && !!summary && !busy && (
                <span className="mtg-tpl-check" title="Enhanced notes ready">
                  <CheckIcon />
                </span>
              )}
            </span>
          </div>

          {/* Favorites */}
          {favoriteEntries.length > 0 && (
            <>
              <div className="mtg-tpl-label">Favorites</div>
              {favoriteEntries.map(templateRow)}
            </>
          )}

          {/* All remaining templates */}
          <div className="mtg-tpl-label">Templates</div>
          {regularEntries.map(templateRow)}

          <div className="mtg-menu-divider" />
          <button className="mtg-tpl-row mtg-tpl-new" onClick={openGallery}>
            <span className="mtg-tpl-emoji">
              <GridIcon />
            </span>
            <span className="mtg-tpl-name">All templates…</span>
          </button>
          <button className="mtg-tpl-row mtg-tpl-new" onClick={openNewTemplateModal}>
            <span className="mtg-tpl-emoji">
              <PlusIcon />
            </span>
            <span className="mtg-tpl-name">New template</span>
          </button>

          {/* Length + format */}
          <div className="mtg-menu-divider" />
          <div className="mtg-tpl-opts">
            <label className="mtg-tpl-opt">
              <span>Length</span>
              <select
                className="mtg-menu-select"
                value={length}
                onChange={(e) => setLength(e.target.value as SummaryLength)}
              >
                <option value="short">Short</option>
                <option value="medium">Medium</option>
                <option value="long">Long</option>
              </select>
            </label>
            <label className="mtg-tpl-opt">
              <span>Format</span>
              <select
                className="mtg-menu-select"
                value={style}
                onChange={(e) => setStyle(e.target.value as SummaryStyle)}
              >
                <option value="bullets">Bullet Points</option>
                <option value="paragraphs">Paragraphs</option>
                <option value="action">Action-Oriented</option>
              </select>
            </label>
          </div>
        </div>
      </>,
      document.getElementById("voco-theme-root") ?? document.body
    );
  };

  const exportMenu = (openState: boolean, close: () => void, direction: "down-right" | "up") => (
    <Menu open={openState} onClose={close} direction={direction}>
      {EXPORT_FORMATS.map((f) => (
        <button
          key={f.id}
          className="mtg-menu-item"
          onClick={() => {
            close();
            onExport(f.id);
          }}
        >
          {f.label}
        </button>
      ))}
    </Menu>
  );

  return (
    <div className="mtg-pane">
      <div className="mtg-note">
        <div className="mtg-note-inner">
          {/* Top bar: back + overflow */}
          <div className="mtg-note-topbar">
            <button className="mtg-iconbtn" onClick={onBack} title="Back to meetings">
              <BackIcon />
            </button>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              {isDiarizing && (
                <span className="mtg-chip mtg-chip-accent voco-pill-enter">
                  <Spinner />
                  Enhancing transcript…
                </span>
              )}
              <div className="mtg-menu-wrap">
                <button
                  className="mtg-iconbtn"
                  onClick={() => setOverflowOpen((v) => !v)}
                  title="More actions"
                >
                  <DotsIcon />
                </button>
                <Menu open={overflowOpen} onClose={() => setOverflowOpen(false)} direction="down-right">
                  {hasRecording && !isActive && (
                    <button
                      className="mtg-menu-item"
                      onClick={() => {
                        setOverflowOpen(false);
                        onReprocess();
                      }}
                    >
                      Reprocess recording
                    </button>
                  )}
                  <button className="mtg-menu-item" onClick={openGallery}>
                    Browse templates…
                  </button>
                  {summary && !busy && (
                    <button
                      className="mtg-menu-item"
                      onClick={() => {
                        setOverflowOpen(false);
                        startNotesEdit();
                      }}
                    >
                      Edit notes
                    </button>
                  )}
                  <div className="mtg-menu-label">Export</div>
                  {EXPORT_FORMATS.map((f) => (
                    <button
                      key={f.id}
                      className="mtg-menu-item"
                      onClick={() => {
                        setOverflowOpen(false);
                        onExport(f.id);
                      }}
                    >
                      {f.label}
                    </button>
                  ))}
                  <div className="mtg-menu-divider" />
                  <button
                    className="mtg-menu-item mtg-menu-item-danger"
                    onClick={() => {
                      setOverflowOpen(false);
                      onDelete();
                    }}
                  >
                    Delete meeting
                  </button>
                </Menu>
              </div>
            </div>
          </div>

          {/* Title — click to rename inline */}
          {titleEditing ? (
            <div
              className="mtg-title-editwrap"
              style={aiSubtitle ? { marginBottom: 0 } : undefined}
            >
              <textarea
                ref={titleTaRef}
                className="mtg-title-input mtg-serif"
                rows={1}
                value={titleValue}
                onChange={(e) => setTitleValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void commitTitle();
                  } else if (e.key === "Escape") {
                    e.stopPropagation();
                    setTitleEditing(false);
                    setTitleError(null);
                  }
                }}
                onBlur={() => void commitTitle()}
                autoFocus
                spellCheck={false}
                aria-label="Meeting title"
              />
              <div className="mtg-title-sugg">
                {titleSuggestions.map((t) => (
                  <button
                    key={t}
                    className="mtg-att-sugg-chip"
                    // mousedown (not click) so the input's blur-commit
                    // can't fire first; fills the input, user commits.
                    onMouseDown={(e) => {
                      e.preventDefault();
                      setTitleValue(t);
                    }}
                  >
                    {t}
                  </button>
                ))}
                <button
                  className="mtg-att-sugg-chip"
                  disabled={titleSuggestLoading}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    void suggestTitleWithAi();
                  }}
                >
                  {titleSuggestLoading ? <Spinner size={10} /> : <span aria-hidden="true">✨</span>}
                  Suggest with AI
                </button>
              </div>
              {titleError && <div className="mtg-title-error">{titleError}</div>}
            </div>
          ) : (
            <h1
              className="mtg-note-title mtg-serif mtg-title-h1"
              style={aiSubtitle ? { marginBottom: 0 } : undefined}
              onClick={startTitleEdit}
              title="Rename meeting"
            >
              {meeting?.title || (isActive ? "New meeting" : "Untitled Meeting")}
              <span className="mtg-title-pencil">
                <PencilIcon />
              </span>
            </h1>
          )}
          {aiSubtitle && (
            <p
              className="mtg-serif"
              style={{
                fontSize: 15,
                fontStyle: "italic",
                color: "var(--color-text-secondary)",
                margin: "2px 0 14px",
                lineHeight: 1.4,
                overflowWrap: "break-word",
              }}
            >
              {aiSubtitle}
            </p>
          )}

          {/* Chips row */}
          <div className="mtg-chips">
            {/* "My notes" ↔ "Enhanced" segmented toggle */}
            <div className="mtg-seg" role="tablist" aria-label="Note view">
              <span className={`mtg-seg-thumb${view === "enhanced" ? " mtg-seg-thumb-right" : ""}`} />
              <button
                className={`mtg-seg-btn${view === "mine" ? " mtg-seg-btn-active" : ""}`}
                role="tab"
                aria-selected={view === "mine"}
                onClick={() => setView("mine")}
              >
                <ListIcon />
                My notes
              </button>
              <div className="mtg-seg-cell mtg-menu-wrap" ref={segCellRef}>
                <button
                  className={`mtg-seg-btn${view === "enhanced" ? " mtg-seg-btn-active" : ""}`}
                  role="tab"
                  aria-selected={view === "enhanced"}
                  onClick={() => setView("enhanced")}
                >
                  <SparklesIcon />
                  {enhancedLabel}
                </button>
                <button
                  className="mtg-seg-caret"
                  onClick={toggleTemplateMenu}
                  title={`Template: ${templateLabel}`}
                  aria-label="Notes template options"
                >
                  <ChevronDownIcon />
                </button>
                {templateMenu(templateOpen, () => setTemplateOpen(false))}
              </div>
            </div>
            {/* Date + speakers chip → attendees popover */}
            <button
              ref={attChipRef}
              className="mtg-chip"
              onClick={toggleAttendees}
              title="Attendees & speakers"
              aria-expanded={attOpen}
            >
              {noteDateLabel(meeting?.created_at) && (
                <>
                  <CalendarIcon />
                  {noteDateLabel(meeting?.created_at)}
                  <span aria-hidden="true">·</span>
                </>
              )}
              <UsersIcon />
              {speakers.length > 0
                ? `${speakers.length} speaker${speakers.length === 1 ? "" : "s"}`
                : "Me"}
            </button>
            {attOpen &&
              createPortal(
                <>
                  <div
                    style={{ position: "fixed", inset: 0, zIndex: 999 }}
                    onClick={closeAttendees}
                  />
                  <div
                    className="mtg-att-pop"
                    style={{ ...attPopBaseStyle, top: attPos.top, left: attPos.left }}
                    role="dialog"
                    aria-label="Attendees and speakers"
                  >
                    {/* Calendar line — informational only */}
                    <div className="mtg-att-cal">
                      <CalendarIcon />
                      <span className="mtg-att-cal-text">
                        {attendeeDateLine(meeting?.created_at) || "No calendar event"}
                      </span>
                    </div>

                    {speakers.length > 0 ? (
                      <>
                        <div className="mtg-att-label">Speakers</div>
                        {speakers.map((s) =>
                          editingSpeakerId === s.id ? (
                            <div key={s.id} className="mtg-att-edit">
                              <input
                                className="mtg-att-input"
                                value={editValue}
                                onChange={(e) => setEditValue(e.target.value)}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter") {
                                    e.preventDefault();
                                    commitSpeakerRename(s.id, editValue);
                                  } else if (e.key === "Escape") {
                                    e.stopPropagation();
                                    setEditingSpeakerId(null);
                                  }
                                }}
                                onBlur={() => commitSpeakerRename(s.id, editValue)}
                                autoFocus
                                spellCheck={false}
                              />
                              {attSuggestions && attSuggestions.length > 0 && (
                                <div className="mtg-att-sugg">
                                  {attSuggestions.slice(0, 6).map((n) => (
                                    <button
                                      key={n}
                                      className="mtg-att-sugg-chip"
                                      // mousedown (not click) so the input's
                                      // blur-commit can't fire first.
                                      onMouseDown={(e) => {
                                        e.preventDefault();
                                        commitSpeakerRename(s.id, n);
                                      }}
                                    >
                                      {n}
                                    </button>
                                  ))}
                                </div>
                              )}
                            </div>
                          ) : (
                            <button
                              key={s.id}
                              className="mtg-att-row"
                              onClick={() => startSpeakerEdit(s.id, s.name)}
                              title="Rename speaker"
                            >
                              <span className="mtg-att-avatar" aria-hidden="true">
                                {(s.name.trim()[0] || "?").toUpperCase()}
                              </span>
                              <span className="mtg-att-name">{s.name}</span>
                              <span className="mtg-att-pencil">
                                <PencilIcon />
                              </span>
                            </button>
                          )
                        )}
                      </>
                    ) : (
                      <div className="mtg-att-empty">
                        No speakers yet — they appear once the transcript has audio.
                      </div>
                    )}
                  </div>
                </>,
                document.getElementById("voco-theme-root") ?? document.body
              )}
            <div className="mtg-menu-wrap">
              <button className="mtg-chip" onClick={() => setExportOpen((v) => !v)}>
                <DownloadIcon />
                Export
                <ChevronDownIcon />
              </button>
              {exportMenu(exportOpen, () => setExportOpen(false), "down-right")}
            </div>
            {!isDiarizing && diarizationTurns !== null && (
              <span className="mtg-chip voco-pill-enter">{diarizationTurns} speaker turns</span>
            )}
          </div>

          {/* Live-recording banner */}
          {isActive && (
            <div className="mtg-banner">
              <span className="mtg-rec-dot" />
              <span>Recording in progress — notes will be generated when you stop.</span>
              <button className="mtg-banner-stop" onClick={onStop}>
                Stop
              </button>
            </div>
          )}

          {/* Note body: the user's own notes or the AI (Enhanced) notes */}
          {view === "mine" ? (
            <div key="mine" className="mtg-view-enter">
              <textarea
                ref={notesRef}
                className="mtg-mynotes"
                placeholder="Write notes"
                value={myNotes}
                onChange={(e) => handleNotesChange(e.target.value)}
                spellCheck={false}
              />
            </div>
          ) : (
            <div key="enhanced" className="mtg-view-enter">
              {renderBody()}
            </div>
          )}
        </div>
      </div>

      {/* Floating "Ask anything" bar (hosts transcript toggle + live controls) */}
      <AskBar
        meetingId={meetingId}
        live={isActive}
        leading={
          <>
            <button
              className="mtg-fbtn"
              onClick={() => setSheetOpen((v) => !v)}
              title="Show transcript"
            >
              <WaveIcon />
              Transcript
            </button>
            <div className="mtg-floatbar-divider" />
          </>
        }
        liveControls={
          <>
            <span className="mtg-floatbar-time" style={{ flex: 1 }}>
              {formatElapsed(seconds)}
            </span>
            {isPaused ? (
              <button className="mtg-fbtn mtg-fbtn-resume" onClick={onResume}>
                Resume
              </button>
            ) : (
              <button className="mtg-fbtn" onClick={onPause}>
                Pause
              </button>
            )}
            <button className="mtg-fbtn mtg-fbtn-stop" onClick={onStop}>
              Stop
            </button>
          </>
        }
        extraChips={
          busy ? (
            <span className="mtg-ask-chip mtg-ask-chip-quiet">
              <Spinner />
              Writing notes…
            </span>
          ) : !summary ? (
            <button
              className="mtg-ask-chip mtg-ask-chip-primary"
              onClick={handleGenerate}
              disabled={segments.length === 0}
            >
              <SparklesIcon />
              Generate notes
            </button>
          ) : undefined
        }
      />

      {/* Full template gallery ("All templates…") */}
      <TemplateGallery
        open={galleryOpen}
        onClose={() => setGalleryOpen(false)}
        customTemplates={customTemplates}
        favorites={favorites}
        selected={template}
        onSelect={selectTemplate}
        onToggleFavorite={toggleFavorite}
        onEditCustom={openEditTemplateModal}
        onDeleteCustom={deleteCustomTemplate}
        onNewTemplate={openNewTemplateModal}
      />

      {/* New / edit custom template modal. Portaled with inline critical
          styles so it stacks above the gallery (z 1200 > 1101) and stays
          immune to cascade/positioning breakage. */}
      {newTplOpen &&
        createPortal(
          <div
            style={{
              position: "fixed",
              inset: 0,
              zIndex: 1200,
              background: "rgba(0, 0, 0, 0.45)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
            onClick={() => setNewTplOpen(false)}
          >
            <div
              className="mtg-modal"
              style={{
                width: 440,
                maxWidth: "calc(100vw - 48px)",
                maxHeight: "80vh",
                overflowY: "auto",
                boxSizing: "border-box",
                margin: 0,
                padding: 24,
                borderRadius: 16,
                background: "var(--color-background-elevated, #1e1e2e)",
                border: "1px solid var(--color-border-strong, rgba(255,255,255,0.12))",
                boxShadow: "0 16px 48px rgba(0, 0, 0, 0.4)",
              }}
              onClick={(e) => e.stopPropagation()}
            >
              <h2 className="mtg-serif mtg-tpl-modal-title">
                {editingTplId ? "Edit template" : "New template"}
              </h2>
            <div className="mtg-tpl-form">
              <div className="mtg-tpl-form-row">
                <label className="mtg-tpl-field" style={{ flex: 1 }}>
                  <span>Name</span>
                  <input
                    className="mtg-tpl-input"
                    value={tplName}
                    onChange={(e) => setTplName(e.target.value)}
                    placeholder="e.g. Client Kickoff"
                    autoFocus
                  />
                </label>
                <label className="mtg-tpl-field" style={{ width: 64 }}>
                  <span>Emoji</span>
                  <input
                    className="mtg-tpl-input"
                    style={{ textAlign: "center" }}
                    value={tplEmoji}
                    onChange={(e) => setTplEmoji(e.target.value)}
                  />
                </label>
              </div>
              <label className="mtg-tpl-field">
                <span>Instructions</span>
                <textarea
                  className="mtg-tpl-input mtg-tpl-textarea"
                  value={tplInstructions}
                  onChange={(e) => setTplInstructions(e.target.value)}
                  rows={6}
                  placeholder={
                    "The section structure the AI should follow, e.g.\n## Overview\n## Key Decisions\n## Risks\n## Action Items"
                  }
                  spellCheck={false}
                />
              </label>
              <div className="mtg-tpl-form-actions">
                <button className="mtg-btn-quiet" onClick={() => setNewTplOpen(false)}>
                  Cancel
                </button>
                <button
                  className="mtg-btn-primary"
                  onClick={saveNewTemplate}
                  disabled={!tplName.trim()}
                >
                  {editingTplId ? "Save changes" : "Save template"}
                </button>
              </div>
            </div>
          </div>
        </div>,
        document.getElementById("voco-theme-root") ?? document.body
      )}

      {/* Transcript bottom sheet */}
      <TranscriptSheet
        open={sheetOpen}
        onClose={() => setSheetOpen(false)}
        meetingId={meetingId}
        segments={segments}
        onRenameSpeaker={onRenameSpeaker}
        isLive={isActive}
        isPaused={isPaused}
        seconds={seconds}
        onPause={onPause}
        onResume={onResume}
        scrollToSegmentId={scrollToSegmentId}
        onScrolledToSegment={onScrolledToSegment}
      />
    </div>
  );
}
