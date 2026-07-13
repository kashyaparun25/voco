import React from "react";
import { Card } from "@astryxdesign/core/Card";
import { Text } from "@astryxdesign/core/Text";
import { VStack, HStack } from "@astryxdesign/core/Layout";

/** Flatten Markdown into plain text for the 2-line list preview (so headings,
 *  bullets, bold markers and table pipes don't show up as raw syntax). */
function summaryPreview(md: string): string {
  return md
    .replace(/```[\s\S]*?```/g, " ")      // code fences
    .replace(/^\s*#{1,6}\s+/gm, "")        // headings
    .replace(/^\s*[-*+]\s+/gm, "")         // bullets
    .replace(/^\s*\d+\.\s+/gm, "")         // numbered lists
    .replace(/^\s*\|.*\|\s*$/gm, "")       // table rows
    .replace(/^\s*[-:| ]+\s*$/gm, "")      // table separators / rules
    .replace(/\*\*(.*?)\*\*/g, "$1")       // bold
    .replace(/\*(.*?)\*/g, "$1")           // italics
    .replace(/`([^`]*)`/g, "$1")           // inline code
    .replace(/\s+/g, " ")
    .trim();
}

export interface DatabaseMeeting {
  id: string;
  title: string;
  created_at: string;
  duration: number; // in seconds
  summary: string | null;
  source?: string; // "recording" | "import"
}

interface MeetingListProps {
  meetings: DatabaseMeeting[];
  selectedMeetingId: string | null;
  onSelectMeeting: (id: string) => void;
  activeMeetingId: string | null;
  style?: React.CSSProperties;
}

export default function MeetingList({
  meetings,
  selectedMeetingId,
  onSelectMeeting,
  activeMeetingId,
  style
}: MeetingListProps) {
  const formatTime = (totalSeconds: number) => {
    if (!totalSeconds) return "0:00";
    const hrs = Math.floor(totalSeconds / 3600);
    const mins = Math.floor((totalSeconds % 3600) / 60);
    const secs = totalSeconds % 60;
    
    if (hrs > 0) {
      return `${hrs}h ${mins}m`;
    }
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  const formatDate = (isoString: string) => {
    try {
      const date = new Date(isoString);
      // Let's format like "July 11, 2026 at 6:30 PM"
      return new Intl.DateTimeFormat("en-US", {
        month: "short",
        day: "numeric",
        year: "numeric",
        hour: "numeric",
        minute: "2-digit",
      }).format(date);
    } catch (e) {
      return isoString;
    }
  };

  return (
    <VStack gap={3} style={{ width: "100%", ...style }}>
      <div style={{ padding: "0 4px" }}>
        <Text style={{ fontSize: "14px", fontWeight: "bold", color: "var(--color-text-secondary)" }}>
          Recent Meetings ({meetings.length})
        </Text>
      </div>

      <VStack gap={2} style={{ maxHeight: "calc(100vh - 320px)", overflowY: "auto", paddingRight: 4 }}>
        {meetings.length === 0 ? (
          <div style={{
            padding: "24px 16px",
            textAlign: "center",
            color: "var(--color-text-secondary)",
            fontSize: "13px",
            border: "1px dashed var(--color-border)",
            borderRadius: "8px"
          }}>
            No recorded meetings yet.
          </div>
        ) : (
          meetings.map((meeting) => {
            const isSelected = selectedMeetingId === meeting.id;
            const isActive = activeMeetingId === meeting.id;

            return (
              <Card
                key={meeting.id}
                onClick={() => onSelectMeeting(meeting.id)}
                style={{
                  padding: "12px 16px",
                  backgroundColor: isSelected 
                    ? "var(--color-background-surface-hover)" 
                    : "var(--color-background-surface)",
                  border: isSelected 
                    ? "1px solid var(--color-accent)" 
                    : "1px solid var(--color-border)",
                  borderRadius: "10px",
                  cursor: "pointer",
                  display: "flex",
                  flexDirection: "column",
                  gap: "8px",
                  transition: "all 0.15s ease",
                  boxShadow: isSelected ? "0 2px 8px rgba(124, 58, 237, 0.15)" : "none"
                }}
                onMouseEnter={(e) => {
                  if (!isSelected) {
                    e.currentTarget.style.borderColor = "var(--color-border-strong)";
                    e.currentTarget.style.backgroundColor = "var(--color-background-surface-hover)";
                  }
                }}
                onMouseLeave={(e) => {
                  if (!isSelected) {
                    e.currentTarget.style.borderColor = "var(--color-border)";
                    e.currentTarget.style.backgroundColor = "var(--color-background-surface)";
                  }
                }}
              >
                <HStack style={{ justifyContent: "space-between", alignItems: "flex-start", width: "100%" }}>
                  <VStack gap={1} style={{ flex: 1 }}>
                    <HStack gap={2} style={{ alignItems: "center" }}>
                      {isActive && (
                        <span style={{
                          width: "8px",
                          height: "8px",
                          borderRadius: "50%",
                          backgroundColor: "var(--color-recording)",
                          animation: "pill-pulse 1.5s infinite"
                        }} />
                      )}
                      <Text style={{
                        fontSize: "14px",
                        fontWeight: "600",
                        color: "var(--color-text-primary)"
                      }}>
                        {meeting.title || "Untitled Meeting"}
                      </Text>
                    </HStack>
                    <Text style={{ fontSize: "11px", color: "var(--color-text-secondary)" }}>
                      {formatDate(meeting.created_at)}
                    </Text>
                  </VStack>
                  <Text style={{ fontSize: "12px", fontWeight: "500", color: "var(--color-text-secondary)" }}>
                    {isActive ? "Live" : formatTime(meeting.duration)}
                  </Text>
                </HStack>

                {meeting.summary && (
                  <Text style={{
                    fontSize: "12px",
                    color: "var(--color-text-secondary)",
                    display: "-webkit-box",
                    WebkitLineClamp: 2,
                    WebkitBoxOrient: "vertical",
                    overflow: "hidden",
                    lineHeight: "1.4",
                    borderLeft: "2px solid var(--color-accent)",
                    paddingLeft: "8px",
                    marginTop: "4px"
                  }}>
                    {summaryPreview(meeting.summary)}
                  </Text>
                )}
              </Card>
            );
          })
        )}
      </VStack>
    </VStack>
  );
}
