import { useState, useEffect, useRef, type ReactNode } from "react";
import { AppShell } from "@astryxdesign/core/AppShell";
import { SideNav, SideNavHeading, SideNavItem } from "@astryxdesign/core/SideNav";
import { Card } from "@astryxdesign/core/Card";
import { Button } from "../components/ui";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Divider } from "@astryxdesign/core/Divider";
import { TextInput } from "../components/ui";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// Custom Meeting Components (Granola-style meetings section)
import type { DatabaseMeeting } from "../components/meeting/MeetingList";
import MeetingsHome, { type UpcomingEvent } from "../components/meeting/MeetingsHome";
import MeetingNotePage from "../components/meeting/MeetingNotePage";
import ScreenRecordingOnboarding from "../components/meeting/ScreenRecordingOnboarding";
import ProviderList from "../components/providers/ProviderList";
import type { SummaryLength, SummaryStyle } from "../components/meeting/SummaryView";
import WaveformCanvas from "../components/waveform/WaveformCanvas";

// Settings panels & theme picker (built by parallel agents)
import ThemeSettings from "../components/settings/ThemeSettings";
import GeneralSettings from "../components/settings/GeneralSettings";
import ModelSelector from "../components/models/ModelSelector";
import CustomModelAdder from "../components/models/CustomModelAdder";
import DictationSettings from "../components/settings/DictationSettings";
import MeetingSettings from "../components/settings/MeetingSettings";
import RecordingSettings from "../components/settings/RecordingSettings";
import HotkeySettings from "../components/settings/HotkeySettings";
import CustomDictionary from "../components/settings/CustomDictionary";
import StatsPage from "../components/stats/StatsPage";
import StatsPill from "../components/stats/StatsPill";
import FileTranscriptionPage from "../components/filetranscription/FileTranscriptionPage";
import GettingStartedPage from "../components/onboarding/GettingStartedPage";

// Hooks & utilities
import { useDictation } from "../hooks/useDictation";
import { showToast } from "../hooks/useToast";
import { useStreamingSummary } from "../hooks/useStreamingSummary";
import FirstRunOnboarding from "../components/onboarding/FirstRunOnboarding";
import DictationHistory from "../components/dictation/DictationHistory";
import { ThemeId } from "../lib/themes";

interface MainWindowProps {
  activeThemeId: string;
  onSelectTheme: (id: ThemeId) => void;
}

const MicIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 18, height: 18 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M12 18.75a6 6 0 0 0 6-6v-1.5m-6 7.5a6 6 0 0 1-6-6v-1.5m6 7.5v3.75m-3.75 0h7.5M12 15.75a3 3 0 0 1-3-3V4.5a3 3 0 1 1 6 0v8.25a3 3 0 0 1-3 3Z" />
  </svg>
);

const CalendarIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 18, height: 18 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 0 1 2.25-2.25h13.5A2.25 2.25 0 0 1 21 7.5v11.25m-18 0A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75m-18 0v-7.5A2.25 2.25 0 0 1 5.25 9h13.5A2.25 2.25 0 0 1 21 11.25v7.5" />
  </svg>
);

const RocketIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 18, height: 18 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M15.59 14.37a6 6 0 0 1-5.84 7.38v-4.8m5.84-2.58a14.98 14.98 0 0 0 6.16-12.12A14.98 14.98 0 0 0 9.631 8.41m5.96 5.96a14.926 14.926 0 0 1-5.841 2.58m-.119-8.54a6 6 0 0 0-7.381 5.84h4.8m2.581-5.84a14.927 14.927 0 0 0-2.58 5.84m2.699 2.7c-.103.021-.207.041-.311.06a15.09 15.09 0 0 1-2.448-2.448 14.9 14.9 0 0 1 .06-.312m-2.24 2.39a4.493 4.493 0 0 0-1.757 4.306 4.493 4.493 0 0 0 4.306-1.758M16.5 9a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Z" />
  </svg>
);

const FileAudioIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 18, height: 18 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 0 0-3.375-3.375h-1.5A1.125 1.125 0 0 1 13.5 7.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 0 0-9-9Z" />
    <path strokeLinecap="round" strokeLinejoin="round" d="M9 15.75a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Zm0 0V11l3 .75" />
  </svg>
);

const StatsIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 18, height: 18 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 0 1 3 19.875v-6.75ZM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V8.625ZM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V4.125Z" />
  </svg>
);

const DictionaryIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 18, height: 18 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M12 6.042A8.967 8.967 0 0 0 6 3.75c-1.052 0-2.062.18-3 .512v14.25A8.987 8.987 0 0 1 6 18c2.305 0 4.408.867 6 2.292m0-14.25a8.966 8.966 0 0 1 6-2.292c1.052 0 2.062.18 3 .512v14.25A8.987 8.987 0 0 0 18 18a8.967 8.967 0 0 0-6 2.292m0-14.25v14.25" />
  </svg>
);

const SettingsIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 18, height: 18 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.43l-1.003.828c-.293.241-.438.613-.43.992a7.723 7.723 0 0 1 0 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.954.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.43l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28Z" />
    <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
  </svg>
);

/** Voco brand mark: an audio waveform whose envelope traces a "V" — voice made visible. */
const LogoIcon = () => (
  <svg
    width={28}
    height={28}
    viewBox="0 0 1024 1024"
    xmlns="http://www.w3.org/2000/svg"
    style={{ borderRadius: 8, boxShadow: "0 2px 4px rgba(0, 0, 0, 0.2)", display: "block" }}
    aria-label="Voco"
    role="img"
  >
    <defs>
      <linearGradient id="vocoTile" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stopColor="#3ee7d7" />
        <stop offset="0.55" stopColor="#159fd8" />
        <stop offset="1" stopColor="#0b60c9" />
      </linearGradient>
      <linearGradient id="vocoBar" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stopColor="#ffffff" />
        <stop offset="1" stopColor="#cdf3ff" />
      </linearGradient>
      <linearGradient id="vocoGloss" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stopColor="#ffffff" stopOpacity="0.42" />
        <stop offset="0.6" stopColor="#ffffff" stopOpacity="0" />
      </linearGradient>
    </defs>
    <rect x="16" y="16" width="992" height="992" rx="232" fill="url(#vocoTile)" />
    <rect x="16" y="16" width="992" height="520" rx="232" fill="url(#vocoGloss)" />
    <g fill="url(#vocoBar)">
      <rect x="115" y="230" width="86" height="564" rx="43" />
      <rect x="233" y="333" width="86" height="358" rx="43" />
      <rect x="351" y="418" width="86" height="188" rx="43" />
      <rect x="469" y="470" width="86" height="84" rx="43" />
      <rect x="587" y="418" width="86" height="188" rx="43" />
      <rect x="705" y="333" width="86" height="358" rx="43" />
      <rect x="823" y="230" width="86" height="564" rx="43" />
    </g>
  </svg>
);

/** Turn a backend hotkey spec into a friendly label. */
function formatHotkeyLabel(spec: string): string {
  const map: Record<string, string> = {
    LeftOption: "Left Option ⌥",
    RightOption: "Right Option ⌥",
    "double:LeftOption": "Double-tap ⌥",
    "double:RightOption": "Double-tap Right ⌥",
    Fn: "Fn / Globe 🌐",
    "double:Fn": "Double-tap Fn 🌐",
    LeftControl: "Left Control ⌃",
    "CommandOrControl+Shift+Space": "⌘ + Shift + Space",
    "Alt+Space": "⌥ + Space",
  };
  return map[spec] || spec;
}

export default function MainWindow({ activeThemeId, onSelectTheme }: MainWindowProps) {
  const [activeTab, setActiveTab] = useState<"dictation" | "meetings" | "files" | "stats" | "dictionary" | "getting-started" | "settings">("dictation");

  // Streaming AI summary (live tokens with non-streaming fallback).
  const streamingSummary = useStreamingSummary();

  // ── Dictation Mode (real backend via useDictation) ──────────────────────
  const dictation = useDictation();
  const [dictationSeconds, setDictationSeconds] = useState(0);

  // Friendly label for the currently-configured dictation hotkey.
  const [hotkeyLabel, setHotkeyLabel] = useState("⌘ + Shift + Space");
  useEffect(() => {
    (async () => {
      try {
        const spec = await invoke<string | null>("get_setting", { key: "dictation_hotkey" });
        setHotkeyLabel(formatHotkeyLabel(spec || "CommandOrControl+Shift+Space"));
      } catch { /* ignore */ }
    })();
  }, [activeTab]);

  useEffect(() => {
    let interval: any;
    if (dictation.isRecording) {
      interval = setInterval(() => setDictationSeconds((p) => p + 1), 1000);
    } else {
      setDictationSeconds(0);
    }
    return () => clearInterval(interval);
  }, [dictation.isRecording]);

  const handleToggleDictation = async () => {
    // The pill is shown/hidden by the backend dictation service, so it works
    // for every trigger (hotkey, tray, button) — no pill calls needed here.
    if (dictation.isRecording) {
      await dictation.stop();
    } else {
      await dictation.start();
    }
  };

  // ── Meetings Mode State ─────────────────────────────────────────────────
  const [meetings, setMeetings] = useState<DatabaseMeeting[]>([]);
  const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
  const [selectedHasRecording, setSelectedHasRecording] = useState(false);
  const [settingsSection, setSettingsSection] = useState<
    "appearance" | "general" | "dictation" | "meetings" | "recordings" | "hotkeys" | "ai"
  >("appearance");
  const [segments, setSegments] = useState<any[]>([]);
  const [meetingRecording, setMeetingRecording] = useState(false);
  const [meetingPaused, setMeetingPaused] = useState(false);
  const [meetingSeconds, setMeetingSeconds] = useState(0);
  const [activeMeetingId, setActiveMeetingId] = useState<string | null>(null);
  const [summary, setSummary] = useState<string | null>(null);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [isOnboardingOpen, setIsOnboardingOpen] = useState(false);
  const [newMeetingTitle, setNewMeetingTitle] = useState("");
  const [isCreatingMeeting, setIsCreatingMeeting] = useState(false);
  const [scrollToSegmentId, setScrollToSegmentId] = useState<string | null>(null);
  const [isFirstRunOpen, setIsFirstRunOpen] = useState(false);

  // First-run onboarding (once), guarded by localStorage.
  useEffect(() => {
    try {
      if (localStorage.getItem("voco-onboarding-done") !== "true") {
        setIsFirstRunOpen(true);
      }
    } catch { /* storage unavailable */ }
  }, []);

  // ── Backend-driven navigation (tray "Settings"/"Start Meeting", etc.) ────
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<string>("navigate", (event) => {
      const target = event.payload;
      if (["meetings", "settings", "dictation", "dictionary", "stats", "files", "getting-started"].includes(target)) {
        setActiveTab(target as typeof activeTab);
      }
    })
      .then((un) => { unlisten = un; })
      .catch(() => { /* Tauri unavailable */ });
    return () => { if (unlisten) unlisten(); };
  }, []);

  // ── Meetings data loading ────────────────────────────────────────────────
  const fetchMeetings = async () => {
    try {
      const list = await invoke<DatabaseMeeting[]>("get_meetings");
      setMeetings(list);
    } catch (err) {
      console.error("Failed to fetch meetings:", err);
    }
  };

  const fetchTranscript = async (meetingId: string) => {
    try {
      const list = await invoke<any[]>("get_meeting_transcript", { meetingId });
      setSegments(list);
    } catch (err) {
      console.error("Failed to fetch transcript:", err);
    }
  };

  useEffect(() => {
    fetchMeetings();
    const checkActiveMeeting = async () => {
      try {
        const activeId = await invoke<string | null>("get_setting", { key: "active_meeting_id" });
        if (activeId && activeId !== "") {
          setActiveMeetingId(activeId);
          setSelectedMeetingId(activeId);
          setMeetingRecording(true);
          fetchTranscript(activeId);
        }
      } catch (err) {
        console.warn("Failed to check active meeting setting", err);
      }
    };
    checkActiveMeeting();
  }, []);

  // Poll transcript + tick duration for the active meeting. The transcript is
  // produced by the real backend meeting service (capture → STT → diarize → store).
  useEffect(() => {
    let interval: any;
    if (selectedMeetingId) {
      interval = setInterval(() => {
        fetchTranscript(selectedMeetingId);
        if (selectedMeetingId === activeMeetingId && meetingRecording && !meetingPaused) {
          setMeetingSeconds((prev) => {
            const nextSecs = prev + 1;
            invoke("update_meeting_duration", { meetingId: activeMeetingId, duration: nextSecs }).catch(() => {});
            return nextSecs;
          });
        }
      }, 1000);
    }
    return () => clearInterval(interval);
  }, [selectedMeetingId, activeMeetingId, meetingRecording, meetingPaused]);

  // ── Diarization + transcript-reload events ───────────────────────────────
  const [diarizationTurns, setDiarizationTurns] = useState<number | null>(null);
  const [isDiarizing, setIsDiarizing] = useState(false);

  // ── Google Calendar: upcoming meetings + reminders/auto-start ────────────
  const [upcoming, setUpcoming] = useState<UpcomingEvent[]>([]);
  const [calendarConnected, setCalendarConnected] = useState(false);
  const notifiedRef = useRef<Set<string>>(new Set());
  const startedRef = useRef<Set<string>>(new Set());

  const notifyMeeting = async (title: string, willAutoStart: boolean) => {
    try {
      const notif = await import("@tauri-apps/plugin-notification");
      let granted = await notif.isPermissionGranted();
      if (!granted) granted = (await notif.requestPermission()) === "granted";
      if (!granted) return;
      notif.sendNotification({
        title: willAutoStart ? "Recording meeting" : "Meeting starting",
        body: willAutoStart ? `Auto-recording "${title}".` : `"${title}" is starting — open Voco to record.`,
      });
    } catch (err) {
      console.warn("notifyMeeting failed:", err);
    }
  };

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      let connected = false;
      try { connected = (await invoke<{ connected: boolean }>("google_status")).connected; } catch { /* ignore */ }
      if (!cancelled) setCalendarConnected(connected);
      if (!connected) { if (!cancelled) setUpcoming([]); return; }
      let mtgs: typeof upcoming = [];
      try { mtgs = await invoke("list_upcoming_meetings"); } catch { return; }
      if (cancelled) return;
      setUpcoming(mtgs);
      const g = async (k: string) => { try { return await invoke<string | null>("get_setting", { key: k }); } catch { return null; } };
      const notify = (await g("meeting_notify_enabled")) === "true";
      const autostart = (await g("meeting_autostart_enabled")) === "true";
      const mins = parseInt((await g("meeting_notify_before_min")) || "1", 10) || 0;
      if (!notify && !autostart) return;
      const now = Date.now();
      for (const m of mtgs) {
        const start = new Date(m.start).getTime();
        if (isNaN(start)) continue;
        if (autostart && now >= start && now < start + 3 * 60000 && !startedRef.current.has(m.id) && !meetingRecording) {
          startedRef.current.add(m.id);
          void notifyMeeting(m.title, true);
          void startMeeting(m.title);
        } else if (notify && now >= start - mins * 60000 && now < start + 2 * 60000 && !notifiedRef.current.has(m.id)) {
          notifiedRef.current.add(m.id);
          void notifyMeeting(m.title, false);
        }
      }
    };
    void tick();
    const timer = window.setInterval(tick, 30000);
    return () => { cancelled = true; window.clearInterval(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meetingRecording]);

  useEffect(() => {
    let unTranscript: (() => void) | undefined;
    let unDiar: (() => void) | undefined;
    let unDiarizing: (() => void) | undefined;
    let unStatus: (() => void) | undefined;
    let unError: (() => void) | undefined;

    // Backend-driven meeting status. The recording thread emits "Idle" whenever
    // it stops — a user stop, or an unexpected one (capture device died). Reset
    // the UI so it can never get stuck showing a phantom running timer.
    listen<string>("meeting-status", (event) => {
      const status = event.payload;
      if (status === "Idle") {
        setMeetingRecording(false);
        setMeetingPaused(false);
        setActiveMeetingId(null);
        fetchMeetings();
      } else if (status === "Recording") {
        setMeetingPaused(false);
      } else if (status === "Paused") {
        setMeetingPaused(true);
      }
    }).then((un) => { unStatus = un; }).catch(() => { /* Tauri unavailable */ });

    // Non-fatal problems the recording thread wants the user to see (transcription
    // failed / credits exhausted, capture device ended, …).
    listen<{ meeting_id: string; message: string }>("meeting-error", (event) => {
      const msg = event.payload?.message || "Meeting recording error";
      showToast(msg, "error");
    }).then((un) => { unError = un; }).catch(() => { /* Tauri unavailable */ });

    listen<{ meeting_id: string; reload?: boolean }>("meeting-transcript-update", (event) => {
      const { meeting_id, reload } = event.payload || ({} as any);
      // Re-fetch when the backend signals a reload for the meeting we're viewing.
      if (reload && meeting_id && meeting_id === selectedMeetingId) {
        fetchTranscript(meeting_id);
      }
    }).then((un) => { unTranscript = un; }).catch(() => { /* Tauri unavailable */ });

    listen<{ meeting_id: string; turns: Array<{ start: number; end: number; speaker: string }> }>(
      "meeting-diarization",
      (event) => {
        const { meeting_id, turns } = event.payload || ({} as any);
        if (meeting_id && meeting_id === selectedMeetingId && Array.isArray(turns)) {
          setIsDiarizing(false);
          setDiarizationTurns(turns.length);
          // Neural relabeling changes speaker names in the DB — refresh.
          fetchTranscript(meeting_id);
        }
      }
    ).then((un) => { unDiar = un; }).catch(() => { /* Tauri unavailable */ });

    // Diarization progress (first run downloads models → can take a while).
    listen<{ meeting_id: string; status: string }>("meeting-diarizing", (event) => {
      const { meeting_id, status } = event.payload || ({} as any);
      if (meeting_id && meeting_id === selectedMeetingId) {
        setIsDiarizing(status === "running");
      }
    }).then((un) => { unDiarizing = un; }).catch(() => { /* Tauri unavailable */ });

    return () => {
      if (unTranscript) unTranscript();
      if (unDiar) unDiar();
      if (unDiarizing) unDiarizing();
      if (unStatus) unStatus();
      if (unError) unError();
    };
  }, [selectedMeetingId]);

  const handleSelectMeeting = (id: string) => {
    setDiarizationTurns(null);
    setIsDiarizing(false);
    setSelectedMeetingId(id);
    fetchTranscript(id);
    const found = meetings.find((m) => m.id === id);
    setSummary(found?.summary || null);
    // Does this meeting have a saved recording we could reprocess?
    setSelectedHasRecording(false);
    invoke<string | null>("get_meeting_audio_path", { meetingId: id })
      .then((p) => setSelectedHasRecording(!!p))
      .catch(() => setSelectedHasRecording(false));
  };

  // Re-run STT + diarization over a saved recording (recovers a transcript that
  // failed live, e.g. the STT provider ran out of credits mid-meeting).
  const handleReprocessMeeting = async () => {
    if (!selectedMeetingId) return;
    try {
      await invoke("reprocess_meeting", { meetingId: selectedMeetingId });
      setSegments([]);
      setIsDiarizing(true);
      showToast("Reprocessing the saved recording — the transcript will refresh as it runs.", "success");
    } catch (err) {
      showToast(typeof err === "string" ? err : "Failed to reprocess recording", "error");
    }
  };

  // Stop is idempotent from the UI's side: the backend may already have
  // stopped on its own (device died, etc.), in which case it returns
  // "Meeting is not active". Either way, always clear the UI so the user
  // is never left stuck.
  const handleStopMeeting = async () => {
    try {
      await invoke("stop_meeting");
    } catch (err) {
      console.warn("stop_meeting (already stopped?):", err);
    } finally {
      setMeetingRecording(false);
      setMeetingPaused(false);
      setActiveMeetingId(null);
      fetchMeetings();
    }
  };

  const handleDeleteMeeting = async () => {
    if (!selectedMeetingId) return;
    const title = meetings.find((m) => m.id === selectedMeetingId)?.title || "this meeting";
    if (!window.confirm(`Delete "${title}"? Its transcript and notes will be removed.`)) return;
    try {
      await invoke("delete_meeting", { meetingId: selectedMeetingId });
      setSelectedMeetingId(null);
      setSegments([]);
      setSummary(null);
      fetchMeetings();
      showToast("Meeting deleted", "success");
    } catch (err) {
      showToast(typeof err === "string" ? err : "Failed to delete meeting", "error");
    }
  };

  const startMeeting = async (title: string) => {
    setActiveTab("meetings");
    setIsCreatingMeeting(false);
    try {
      const id = await invoke<string>("start_meeting", { title });
      setActiveMeetingId(id);
      setSelectedMeetingId(id);
      setMeetingRecording(true);
      setMeetingPaused(false);
      setMeetingSeconds(0);
      setSegments([]);
      fetchMeetings();
      return id;
    } catch (err) {
      console.error("Failed to start meeting", err);
      showToast("Failed to start meeting", "error");
      return null;
    }
  };

  const handleImportAudio = async () => {
    if (meetingRecording) return;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [{ name: "Audio", extensions: ["mp3", "m4a", "wav", "flac", "aac", "ogg", "mp4"] }],
      });
      if (!selected || typeof selected !== "string") return;
      const filename = selected.split("/").pop() || "Imported Audio";
      const title = filename.replace(/\.[^.]+$/, "") || "Imported Audio";
      showToast("Importing & transcribing audio…", "success");
      const id = await invoke<string>("import_audio", { path: selected, title });
      await fetchMeetings();
      handleSelectMeeting(id);
    } catch (err) {
      console.error("Import audio failed", err);
      showToast(`Import failed: ${err}`, "error");
    }
  };

  const handleRenameSpeaker = async (speakerId: string, newName: string) => {
    try {
      // Rust signature is rename_speaker(speaker_id, name) — the arg key must
      // be `name`, not `newName` (a mismatch is rejected silently by invoke).
      await invoke("rename_speaker", { speakerId, name: newName });
      if (selectedMeetingId) fetchTranscript(selectedMeetingId);
      fetchMeetings();
    } catch (err) {
      console.error("Failed to rename speaker", err);
      showToast(`Rename failed: ${err}`, "error");
    }
  };

  const handleSummarize = async (length: SummaryLength, style: SummaryStyle) => {
    if (!selectedMeetingId) return;
    try {
      setSummaryLoading(true);
      const sum = await streamingSummary.generate(
        selectedMeetingId, length, style,
        "summarize_meeting_streaming", "summarize_meeting"
      );
      if (sum) setSummary(sum);
      else showToast("Failed to generate summary", "error");
      fetchMeetings();
    } catch (err) {
      console.error("Failed to summarize meeting:", err);
      showToast("Failed to generate summary", "error");
    } finally {
      setSummaryLoading(false);
    }
  };

  const handleRegenerateSummary = async (length: SummaryLength, style: SummaryStyle) => {
    if (!selectedMeetingId) return;
    try {
      setSummaryLoading(true);
      // Streaming command handles regeneration; fall back to regenerate_summary.
      const sum = await streamingSummary.generate(
        selectedMeetingId, length, style,
        "summarize_meeting_streaming", "regenerate_summary"
      );
      if (sum) setSummary(sum);
      else showToast("Failed to regenerate summary", "error");
      fetchMeetings();
    } catch (err) {
      console.error("Failed to regenerate summary:", err);
      showToast("Failed to regenerate summary", "error");
    } finally {
      setSummaryLoading(false);
    }
  };

  // Export the full transcript (TXT / SRT / JSON / Markdown) via the backend
  // export engine, then save through the dialog + fs plugins.
  const handleExportMeeting = async (format: "txt" | "srt" | "vtt" | "json" | "markdown") => {
    if (!selectedMeetingId) return;
    try {
      const ext = format === "markdown" ? "md" : format;
      const title = meetings.find((m) => m.id === selectedMeetingId)?.title || "meeting";
      const clean = title.toLowerCase().trim().replace(/[^a-z0-9]+/g, "_");
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: `${clean}.${ext}` });
      if (path) {
        // Write on the Rust side — the fs plugin's default scope refuses
        // arbitrary save-dialog paths, which was making every export fail.
        await invoke("export_meeting_to_path", { meetingId: selectedMeetingId, format, path });
        showToast("Transcript exported", "success");
      }
    } catch (err) {
      console.error("Failed to export meeting:", err);
      showToast(typeof err === "string" ? `Export failed: ${err}` : "Export failed", "error");
    }
  };

  const formatTime = (totalSecs: number) => {
    const mins = Math.floor(totalSecs / 60);
    const secs = totalSecs % 60;
    return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
  };

  const renderDictation = () => (
    <VStack gap={4} style={{ padding: 24, height: "100%", overflow: "hidden" }}>
      <HStack style={{ justifyContent: "space-between", alignItems: "flex-start", width: "100%" }}>
        <VStack gap={2}>
          <Text style={{ fontSize: "28px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
            Real-time Dictation
          </Text>
          <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
            Start speaking to transcribe instantly at your cursor location using local AI models.
          </Text>
        </VStack>
        <StatsPill onClick={() => setActiveTab("stats")} />
      </HStack>

      <Card style={{
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        padding: 40,
        backgroundColor: "var(--color-background-surface)",
        border: "1px solid var(--color-border)",
        borderRadius: "16px",
        gap: 24
      }}>
        {dictation.isRecording ? (
          <VStack gap={3} style={{ alignItems: "center", width: "100%" }}>
            <div style={{ width: 280, height: 56 }}>
              <WaveformCanvas active rmsProp={dictation.audioLevel} />
            </div>
            <Text style={{ fontSize: 16, fontWeight: "500", color: "var(--color-recording)" }}>
              Listening... {formatTime(dictationSeconds)}
            </Text>
            {dictation.partialText && (
              <Text style={{ fontSize: 13, color: "var(--color-text-secondary)", textAlign: "center", maxWidth: 420 }}>
                {dictation.partialText}
              </Text>
            )}
          </VStack>
        ) : (
          <VStack gap={2} style={{ alignItems: "center" }}>
            <div style={{
              width: 64,
              height: 64,
              borderRadius: "50%",
              backgroundColor: "var(--color-background-surface-hover)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              color: "var(--color-accent)",
              marginBottom: 8
            }}>
              <MicIcon />
            </div>
            <Text style={{ fontSize: 16, fontWeight: "500", color: "var(--color-text-primary)" }}>
              Press the button or hotkey to start
            </Text>
            <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>
              Hotkey: {hotkeyLabel}
            </Text>
          </VStack>
        )}

        <Button
          variant={dictation.isRecording ? "secondary" : "primary"}
          onClick={handleToggleDictation}
          label={dictation.isRecording ? "Stop Dictation" : "Start Dictation"}
          style={{
            padding: "12px 24px",
            borderRadius: "999px",
            backgroundColor: dictation.isRecording ? "var(--color-background-surface-hover)" : "var(--color-accent)",
            color: dictation.isRecording ? "var(--color-text-primary)" : "#ffffff",
            fontWeight: "bold",
            border: dictation.isRecording ? "1px solid var(--color-border-strong)" : "none",
            cursor: "pointer"
          }}
        />
      </Card>

      <Text style={{ fontSize: 12, color: "var(--color-text-secondary)", textAlign: "center", flexShrink: 0 }}>
        Transcription runs locally with Metal acceleration. Configure the model in Settings → Dictation.
      </Text>

      <DictationHistory />
    </VStack>
  );

  const renderMeetings = () => {
    const selectedMeeting = meetings.find((m) => m.id === selectedMeetingId);
    return (
      <div className="mtg-pane">
        {selectedMeetingId ? (
          <MeetingNotePage
            meeting={selectedMeeting}
            meetingId={selectedMeetingId}
            isActive={selectedMeetingId === activeMeetingId}
            isPaused={meetingPaused}
            seconds={selectedMeetingId === activeMeetingId ? meetingSeconds : selectedMeeting?.duration || 0}
            segments={segments}
            summary={summary}
            summaryLoading={summaryLoading}
            streamingText={streamingSummary.streamingText}
            isStreaming={streamingSummary.isStreaming}
            isDiarizing={isDiarizing}
            diarizationTurns={diarizationTurns}
            hasRecording={selectedHasRecording}
            onBack={() => setSelectedMeetingId(null)}
            onStop={() => void handleStopMeeting()}
            onPause={async () => {
              setMeetingPaused(true);
              try { await invoke("pause_meeting"); } catch (_) { /* backend not active */ }
            }}
            onResume={async () => {
              setMeetingPaused(false);
              try { await invoke("resume_meeting"); } catch (_) { /* backend not active */ }
            }}
            onReprocess={handleReprocessMeeting}
            onDelete={handleDeleteMeeting}
            onExport={handleExportMeeting}
            onGenerate={handleSummarize}
            onRegenerate={handleRegenerateSummary}
            onRenameSpeaker={handleRenameSpeaker}
            onRenamed={() => void fetchMeetings()}
            onSummarySaved={(s) => {
              setSummary(s);
              void fetchMeetings();
            }}
            scrollToSegmentId={scrollToSegmentId}
            onScrolledToSegment={() => setScrollToSegmentId(null)}
          />
        ) : (
          // Imports are shown here too: the home page has its own "Import
          // audio" button, so hiding source=import made fresh imports vanish
          // from the very page that created them (they only appeared under
          // File Transcription). Both pages list them now.
          <MeetingsHome
            meetings={meetings}
            activeMeetingId={activeMeetingId}
            liveSeconds={meetingSeconds}
            upcoming={upcoming}
            calendarConnected={calendarConnected}
            recordingBusy={meetingRecording}
            onNewNote={() => {
              if (meetingRecording) return;
              const done = localStorage.getItem("voco-screen-recording-onboarding-done");
              if (done !== "true") setIsOnboardingOpen(true);
              else setIsCreatingMeeting(true);
            }}
            onImport={handleImportAudio}
            onSelectMeeting={handleSelectMeeting}
            onStartUpcoming={(title) => void startMeeting(title)}
            onSelectSearchResult={(meetingId, segmentId) => {
              handleSelectMeeting(meetingId);
              setScrollToSegmentId(segmentId);
            }}
          />
        )}

        {/* New-note title prompt */}
        {isCreatingMeeting && (
          <div className="mtg-modal-backdrop" onClick={() => setIsCreatingMeeting(false)}>
            <div className="mtg-modal" onClick={(e) => e.stopPropagation()}>
              <VStack gap={4}>
                <Text className="mtg-serif" style={{ fontSize: "22px", fontWeight: 600, color: "var(--color-text-primary)" }}>
                  New note
                </Text>
                <TextInput
                  label="Meeting Title"
                  placeholder="e.g. Project Architecture Sync"
                  value={newMeetingTitle}
                  onChange={(val) => setNewMeetingTitle(val)}
                  style={{ width: "100%" }}
                />
                <HStack gap={3} style={{ justifyContent: "flex-end", marginTop: "8px" }}>
                  <Button variant="secondary" onClick={() => setIsCreatingMeeting(false)} label="Cancel" style={{ cursor: "pointer" }} />
                  <Button
                    variant="primary"
                    onClick={() => {
                      const title = newMeetingTitle.trim() || "Meeting Review";
                      setNewMeetingTitle("");
                      void startMeeting(title);
                    }}
                    label="Start Recording"
                    style={{ cursor: "pointer", backgroundColor: "var(--color-accent)", color: "#ffffff", border: "none", borderRadius: "999px" }}
                  />
                </HStack>
              </VStack>
            </div>
          </div>
        )}

        <ScreenRecordingOnboarding
          isOpen={isOnboardingOpen}
          onClose={() => setIsOnboardingOpen(false)}
          onConfirm={() => {
            setIsOnboardingOpen(false);
            localStorage.setItem("voco-screen-recording-onboarding-done", "true");
            setIsCreatingMeeting(true);
          }}
        />
      </div>
    );
  };

  const SETTINGS_SECTIONS = [
    { id: "appearance", label: "Appearance" },
    { id: "general", label: "General" },
    { id: "dictation", label: "Dictation" },
    { id: "meetings", label: "Meetings" },
    { id: "recordings", label: "Recordings" },
    { id: "hotkeys", label: "Hotkeys" },
    { id: "ai", label: "AI Providers & Models" },
  ] as const;

  const renderSettingsSection = () => {
    switch (settingsSection) {
      case "appearance":
        return <ThemeSettings activeThemeId={activeThemeId} onSelect={onSelectTheme} />;
      case "general":
        return <GeneralSettings />;
      case "dictation":
        return <DictationSettings />;
      case "meetings":
        return <MeetingSettings />;
      case "recordings":
        return <RecordingSettings />;
      case "hotkeys":
        return <HotkeySettings />;
      case "ai":
        return (
          <VStack gap={4}>
            <Text style={{ fontSize: 14, color: "var(--color-text-secondary)" }}>
              Add cloud APIs or local servers, then pick a provider and model for each task under
              Dictation and Meetings. Download on-device models for fully offline use.
            </Text>
            <ProviderList />
            <Divider style={{ backgroundColor: "var(--color-border)", height: 1, margin: "8px 0" }} />
            <CustomModelAdder />
            <ModelSelector />
          </VStack>
        );
    }
  };

  const renderSettings = () => (
    <HStack gap={0} style={{ height: "100%", width: "100%", overflow: "hidden" }}>
      {/* Settings sub-navigation */}
      <div style={{
        width: 216,
        height: "100%",
        borderRight: "1px solid var(--color-border)",
        padding: "20px 12px",
        boxSizing: "border-box",
        overflowY: "auto",
        flexShrink: 0,
      }}>
        <Text style={{ fontSize: 20, fontWeight: "bold", color: "var(--color-text-primary)", padding: "0 8px 12px", display: "block" }}>
          Settings
        </Text>
        <VStack gap={1}>
          {SETTINGS_SECTIONS.map((s) => {
            const active = settingsSection === s.id;
            return (
              <button
                key={s.id}
                onClick={() => setSettingsSection(s.id)}
                style={{
                  textAlign: "left",
                  padding: "10px 12px",
                  borderRadius: 8,
                  border: "none",
                  borderLeft: active ? "2px solid var(--color-accent)" : "2px solid transparent",
                  cursor: "pointer",
                  backgroundColor: active ? "var(--color-background-surface-hover)" : "transparent",
                  color: active ? "var(--color-text-primary)" : "var(--color-text-secondary)",
                  fontSize: 14,
                  fontWeight: active ? 600 : 500,
                  transition: "background-color 0.15s ease, color 0.15s ease",
                }}
              >
                {s.label}
              </button>
            );
          })}
        </VStack>
      </div>

      {/* Section content */}
      <div style={{ flex: 1, height: "100%", overflowY: "auto", padding: 24, boxSizing: "border-box" }}>
        {renderSettingsSection()}
      </div>
    </HStack>
  );

  const renderDictionary = () => (
    <VStack gap={4} style={{ padding: 24, height: "100%", overflowY: "auto" }}>
      <VStack gap={2}>
        <Text style={{ fontSize: "28px", fontWeight: "bold", color: "var(--color-text-primary)" }}>Dictionary</Text>
        <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
          Word replacements applied to every dictation — names, acronyms, product names, unusual spellings.
          Update these anytime; changes take effect on your next dictation.
        </Text>
      </VStack>
      <Divider style={{ backgroundColor: "var(--color-border)", height: 1 }} />
      <CustomDictionary />
    </VStack>
  );

  const renderContent = () => {
    let inner: ReactNode = null;
    switch (activeTab) {
      case "dictation": inner = renderDictation(); break;
      case "meetings": inner = renderMeetings(); break;
      case "files": inner = <FileTranscriptionPage />; break;
      case "getting-started":
        inner = (
          <GettingStartedPage
            onOpenSettings={(section) => {
              setSettingsSection(section as typeof settingsSection);
              setActiveTab("settings");
            }}
          />
        );
        break;
      case "stats": inner = <StatsPage />; break;
      case "dictionary": inner = renderDictionary(); break;
      case "settings": inner = renderSettings(); break;
      default: inner = null;
    }
    // key on activeTab so the fade re-triggers on tab switch
    return (
      <div key={activeTab} className="voco-tab-enter" style={{ height: "100%", width: "100%" }}>
        {inner}
      </div>
    );
  };

  return (
    <AppShell
      variant="elevated"
      sideNav={
        <SideNav header={<SideNavHeading heading="Voco" icon={<LogoIcon />} />}>
          <div style={{ display: "flex", flexDirection: "column", gap: 4, padding: "8px 0" }}>
            <SideNavItem label="Dictation" icon={MicIcon} isSelected={activeTab === "dictation"} onClick={() => setActiveTab("dictation")} style={{ cursor: "pointer" }} />
            <SideNavItem label="Meetings" icon={CalendarIcon} isSelected={activeTab === "meetings"} onClick={() => setActiveTab("meetings")} style={{ cursor: "pointer" }} />
            <SideNavItem label="File Transcription" icon={FileAudioIcon} isSelected={activeTab === "files"} onClick={() => setActiveTab("files")} style={{ cursor: "pointer" }} />
            <SideNavItem label="Stats" icon={StatsIcon} isSelected={activeTab === "stats"} onClick={() => setActiveTab("stats")} style={{ cursor: "pointer" }} />
            <SideNavItem label="Dictionary" icon={DictionaryIcon} isSelected={activeTab === "dictionary"} onClick={() => setActiveTab("dictionary")} style={{ cursor: "pointer" }} />
            <SideNavItem label="Getting Started" icon={RocketIcon} isSelected={activeTab === "getting-started"} onClick={() => setActiveTab("getting-started")} style={{ cursor: "pointer" }} />
            <SideNavItem label="Settings" icon={SettingsIcon} isSelected={activeTab === "settings"} onClick={() => setActiveTab("settings")} style={{ cursor: "pointer" }} />
          </div>
        </SideNav>
      }
    >
      {renderContent()}
      <FirstRunOnboarding
        isOpen={isFirstRunOpen}
        onClose={() => {
          try { localStorage.setItem("voco-onboarding-done", "true"); } catch { /* ignore */ }
          setIsFirstRunOpen(false);
        }}
        onGoToModels={() => {
          try { localStorage.setItem("voco-onboarding-done", "true"); } catch { /* ignore */ }
          setIsFirstRunOpen(false);
          setActiveTab("settings");
        }}
      />
    </AppShell>
  );
}
