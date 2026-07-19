import { useEffect, useMemo, useRef, useState } from "react";
import { TranscriptSegment } from "../transcript/SegmentCard";
import SpeakerBadge from "../transcript/SpeakerBadge";
import AudioPlayer from "./AudioPlayer";

interface TranscriptSheetProps {
  open: boolean;
  onClose: () => void;
  meetingId: string;
  segments: TranscriptSegment[];
  onRenameSpeaker: (speakerId: string, newName: string) => void;
  /** True when this meeting is the actively recording one. */
  isLive: boolean;
  isPaused: boolean;
  /** Elapsed seconds for the live meeting. */
  seconds: number;
  onPause: () => void;
  onResume: () => void;
  /** When set, scroll to (and briefly highlight) this segment. */
  scrollToSegmentId?: string | null;
  onScrolledToSegment?: () => void;
}

const CopyIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 15, height: 15 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 17.25v3.375c0 .621-.504 1.125-1.125 1.125h-9.75a1.125 1.125 0 0 1-1.125-1.125V7.875c0-.621.504-1.125 1.125-1.125H5.25m11.9-3.664A2.251 2.251 0 0 0 15 2.25h-1.5a2.25 2.25 0 0 0-2.25 2.25h-.375c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h9.75c.621 0 1.125-.504 1.125-1.125V7.875c0-.621-.504-1.125-1.125-1.125H18a2.25 2.25 0 0 0-2.25-2.25Z" />
  </svg>
);

const CheckIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="var(--color-speaker-2, currentColor)" style={{ width: 15, height: 15 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
  </svg>
);

const MinimizeIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 15, height: 15 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M5 12h14" />
  </svg>
);

function formatElapsed(totalSecs: number): string {
  const hrs = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;
  if (hrs > 0) return `${hrs}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
  return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
}

function formatTimestamp(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
}

interface SpeakerCluster {
  key: string;
  speakerId: string | null;
  speakerName: string | null;
  segments: TranscriptSegment[];
}

/** Group consecutive segments from the same speaker into chat-bubble clusters. */
function clusterSegments(segments: TranscriptSegment[]): SpeakerCluster[] {
  const clusters: SpeakerCluster[] = [];
  for (const seg of segments) {
    const last = clusters[clusters.length - 1];
    const sameSpeaker =
      last &&
      last.speakerId === seg.speaker_id &&
      (last.speakerName ?? "") === (seg.speaker_name ?? "");
    if (sameSpeaker) {
      last.segments.push(seg);
    } else {
      clusters.push({
        key: seg.id,
        speakerId: seg.speaker_id,
        speakerName: seg.speaker_name,
        segments: [seg],
      });
    }
  }
  return clusters;
}

/**
 * TranscriptSheet — Granola-style bottom sheet that slides up inside the
 * meetings pane and shows the diarized transcript as chat bubbles.
 */
export default function TranscriptSheet({
  open,
  onClose,
  meetingId,
  segments,
  onRenameSpeaker,
  isLive,
  isPaused,
  seconds,
  onPause,
  onResume,
  scrollToSegmentId,
  onScrolledToSegment,
}: TranscriptSheetProps) {
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);
  const shouldAutoScrollRef = useRef(true);
  const [highlightId, setHighlightId] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return segments;
    return segments.filter(
      (s) =>
        s.text.toLowerCase().includes(q) ||
        (s.speaker_name ?? "").toLowerCase().includes(q)
    );
  }, [segments, query]);

  const clusters = useMemo(() => clusterSegments(filtered), [filtered]);

  // Auto-scroll to the bottom on new segments when the user is near the bottom.
  const handleScroll = () => {
    const el = contentRef.current;
    if (!el) return;
    shouldAutoScrollRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 100;
  };

  useEffect(() => {
    const el = contentRef.current;
    if (open && el && shouldAutoScrollRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [segments.length, open]);

  // Scroll to a specific segment (from global search) and highlight it.
  useEffect(() => {
    if (!open || !scrollToSegmentId || !contentRef.current) return;
    const el = contentRef.current.querySelector<HTMLElement>(
      `[data-segment-id="${CSS.escape(scrollToSegmentId)}"]`
    );
    if (el) {
      shouldAutoScrollRef.current = false;
      el.scrollIntoView({ behavior: "smooth", block: "center" });
      setHighlightId(scrollToSegmentId);
      const t = setTimeout(() => setHighlightId(null), 2000);
      onScrolledToSegment?.();
      return () => clearTimeout(t);
    }
    // Segment not loaded yet — retry when segments change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, scrollToSegmentId, segments]);

  if (!open) return null;

  const handleCopyAll = async () => {
    const text = segments
      .map((s) => {
        const speaker = s.speaker_name || (s.speaker_id ? `Speaker ${s.speaker_id}` : "Unknown");
        return `${speaker}: ${s.text}`;
      })
      .join("\n\n");
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Failed to copy transcript:", err);
    }
  };

  return (
    <>
      <div className="mtg-sheet-backdrop" onClick={onClose} />
      <div className="mtg-sheet" role="dialog" aria-label="Transcript">
        {/* Header: search, copy-all, minimize */}
        <div className="mtg-sheet-header">
          <input
            className="mtg-sheet-search"
            type="text"
            placeholder="Search transcript…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button className="mtg-iconbtn" onClick={handleCopyAll} title="Copy full transcript">
            {copied ? <CheckIcon /> : <CopyIcon />}
          </button>
          <button className="mtg-iconbtn" onClick={onClose} title="Minimize">
            <MinimizeIcon />
          </button>
        </div>

        {/* Content: audio player (saved meetings) + chat bubbles */}
        <div className="mtg-sheet-content" ref={contentRef} onScroll={handleScroll}>
          {!isLive && (
            <div style={{ marginBottom: 14 }}>
              <AudioPlayer meetingId={meetingId} />
            </div>
          )}
          {clusters.length === 0 ? (
            <div className="mtg-sheet-empty">
              <span style={{ fontWeight: 600, color: "var(--color-text-primary)" }}>
                {query ? "No matches" : "No transcript yet"}
              </span>
              <span>
                {query
                  ? "Try different keywords."
                  : isLive
                    ? "Speak to start transcribing…"
                    : "This meeting has no transcript segments."}
              </span>
            </div>
          ) : (
            clusters.map((cluster) => (
              <div key={cluster.key} className="mtg-cluster">
                <div className="mtg-cluster-label">
                  {cluster.speakerId ? (
                    <SpeakerBadge
                      speakerId={cluster.speakerId}
                      speakerName={cluster.speakerName}
                      onRenameSpeaker={onRenameSpeaker}
                    />
                  ) : (
                    <span style={{ fontSize: 11, fontWeight: 600, color: "var(--color-text-secondary)" }}>
                      {cluster.speakerName || "Unknown speaker"}
                    </span>
                  )}
                </div>
                {cluster.segments.map((seg) => (
                  <div
                    key={seg.id}
                    data-segment-id={seg.id}
                    className={`mtg-bubble${highlightId === seg.id ? " mtg-bubble-highlight" : ""}`}
                  >
                    {seg.text}
                    <div className="mtg-bubble-time">
                      {formatTimestamp(seg.start_time)} – {formatTimestamp(seg.end_time)}
                    </div>
                  </div>
                ))}
              </div>
            ))
          )}
        </div>

        {/* Footer: live controls / segment count */}
        <div className="mtg-sheet-footer">
          {isLive ? (
            <>
              <span className="mtg-rec-dot" />
              <span className="mtg-sheet-footer-time">{formatElapsed(seconds)}</span>
              {isPaused ? (
                <button className="mtg-sheet-pill mtg-sheet-pill-resume" onClick={onResume}>
                  Resume
                </button>
              ) : (
                <button className="mtg-sheet-pill" onClick={onPause}>
                  Pause
                </button>
              )}
            </>
          ) : (
            <span>
              {segments.length} segment{segments.length === 1 ? "" : "s"}
            </span>
          )}
        </div>
      </div>
    </>
  );
}
