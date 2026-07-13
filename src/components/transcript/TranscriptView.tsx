import { useState, useEffect, useRef } from "react";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { TextInput } from "../ui";
import { Text } from "@astryxdesign/core/Text";
import SegmentCard, { TranscriptSegment } from "./SegmentCard";

interface TranscriptViewProps {
  segments: TranscriptSegment[];
  onRenameSpeaker: (speakerId: string, newName: string) => void;
  isRecording?: boolean;
  /** When set, scroll to (and briefly highlight) this segment. */
  scrollToSegmentId?: string | null;
  /** Called after a scroll-to request has been handled. */
  onScrolledToSegment?: () => void;
  style?: React.CSSProperties;
}

export default function TranscriptView({
  segments,
  onRenameSpeaker,
  isRecording = false,
  scrollToSegmentId,
  onScrolledToSegment,
  style
}: TranscriptViewProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const containerRef = useRef<HTMLDivElement>(null);
  const [shouldAutoScroll, setShouldAutoScroll] = useState(true);
  const [highlightId, setHighlightId] = useState<string | null>(null);

  // Filter segments based on search query
  const filteredSegments = segments.filter(seg =>
    seg.text.toLowerCase().includes(searchQuery.toLowerCase()) ||
    (seg.speaker_name && seg.speaker_name.toLowerCase().includes(searchQuery.toLowerCase())) ||
    (seg.speaker_id && seg.speaker_id.toLowerCase().includes(searchQuery.toLowerCase()))
  );

  // Monitor scroll position to see if user scrolled up manually
  const handleScroll = () => {
    if (!containerRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = containerRef.current;
    // If the user is within 100px of the bottom, enable auto-scroll
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 100;
    setShouldAutoScroll(isAtBottom);
  };

  // Perform auto-scroll when segments count changes
  useEffect(() => {
    if (shouldAutoScroll && containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [segments, shouldAutoScroll]);

  // Scroll to a specific segment (e.g. from global search) and highlight it.
  useEffect(() => {
    if (!scrollToSegmentId || !containerRef.current) return;
    // Disable auto-scroll so we don't get yanked back to the bottom.
    setShouldAutoScroll(false);
    const el = containerRef.current.querySelector<HTMLElement>(
      `[data-segment-id="${CSS.escape(scrollToSegmentId)}"]`
    );
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "center" });
      setHighlightId(scrollToSegmentId);
      const t = setTimeout(() => setHighlightId(null), 2000);
      onScrolledToSegment?.();
      return () => clearTimeout(t);
    }
    onScrolledToSegment?.();
  }, [scrollToSegmentId, segments]);

  return (
    <VStack gap={3} style={{ flex: 1, minHeight: 0, height: "100%", ...style }}>
      {/* Top action bar: Search & Filter */}
      <HStack gap={3} style={{ alignItems: "center", width: "100%" }}>
        <div style={{ flex: 1 }}>
          <TextInput
            label="Search"
            placeholder="Search transcript..."
            value={searchQuery}
            onChange={(val) => setSearchQuery(val)}
            style={{
              width: "100%",
              backgroundColor: "var(--color-background-surface)"
            }}
          />
        </div>
      </HStack>

      {/* Main scrolling container */}
      <div
        ref={containerRef}
        onScroll={handleScroll}
        style={{
          flex: 1,
          overflowY: "auto",
          paddingRight: "8px",
          display: "flex",
          flexDirection: "column",
          gap: "12px",
          scrollBehavior: "smooth"
        }}
      >
        {filteredSegments.length === 0 ? (
          <div style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            padding: "48px 24px",
            textAlign: "center",
            color: "var(--color-text-secondary)",
            gap: "8px"
          }}>
            <svg
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              strokeWidth={1.5}
              stroke="currentColor"
              style={{ width: 48, height: 48, opacity: 0.3 }}
            >
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 7.5h1.5m-1.5 3h1.5m-7.5 3h7.5m-7.5 3h7.5m3-9h3.375c.621 0 1.125.504 1.125 1.125V18a2.25 2.25 0 0 1-2.25 2.25M16.5 7.5V18a2.25 2.25 0 0 0 2.25 2.25M16.5 7.5V4.875c0-.621-.504-1.125-1.125-1.125H4.125C3.504 3.75 3 4.254 3 4.875V18a2.25 2.25 0 0 0 2.25 2.25h13.5M6 7.5h3v3H6v-3Z" />
            </svg>
            <Text style={{ fontSize: "14px", fontWeight: "600", color: "var(--color-text-secondary)" }}>
              {searchQuery ? "No matches found" : "No transcript segments"}
            </Text>
            <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
              {searchQuery 
                ? "Try adjusting your search keywords." 
                : isRecording 
                  ? "Speak to start transcribing..." 
                  : "Start a meeting recording to see the transcript here."
              }
            </Text>
          </div>
        ) : (
          filteredSegments.map((seg) => (
            <div
              key={seg.id}
              data-segment-id={seg.id}
              className="voco-segment-enter"
              style={{
                borderRadius: "12px",
                outline: highlightId === seg.id ? "2px solid var(--color-accent)" : "none",
                outlineOffset: "2px",
                transition: "outline-color 0.3s ease",
              }}
            >
              <SegmentCard
                segment={seg}
                onRenameSpeaker={onRenameSpeaker}
              />
            </div>
          ))
        )}
      </div>
    </VStack>
  );
}
