import { Card } from "@astryxdesign/core/Card";
import { Text } from "@astryxdesign/core/Text";
import { HStack } from "@astryxdesign/core/Layout";
import SpeakerBadge from "./SpeakerBadge";

export interface TranscriptSegment {
  id: string;
  meeting_id: string;
  speaker_id: string | null;
  speaker_name: string | null;
  start_time: number;
  end_time: number;
  text: string;
  created_at: string;
}

interface SegmentCardProps {
  segment: TranscriptSegment;
  onRenameSpeaker: (speakerId: string, newName: string) => void;
  style?: React.CSSProperties;
}

export default function SegmentCard({ segment, onRenameSpeaker, style }: SegmentCardProps) {
  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
  };

  return (
    <Card style={{
      padding: "16px",
      backgroundColor: "var(--color-background-surface)",
      border: "1px solid var(--color-border)",
      borderRadius: "12px",
      display: "flex",
      flexDirection: "column",
      gap: "10px",
      transition: "transform 0.2s ease, box-shadow 0.2s ease",
      boxShadow: "0 2px 6px rgba(0, 0, 0, 0.05)",
      ...style
    }}
    onMouseEnter={(e) => {
      e.currentTarget.style.transform = "translateY(-1px)";
      e.currentTarget.style.boxShadow = "0 4px 12px rgba(0, 0, 0, 0.08)";
    }}
    onMouseLeave={(e) => {
      e.currentTarget.style.transform = "none";
      e.currentTarget.style.boxShadow = "0 2px 6px rgba(0, 0, 0, 0.05)";
    }}
    >
      <HStack style={{ justifyContent: "space-between", alignItems: "center", width: "100%" }}>
        {segment.speaker_id ? (
          <SpeakerBadge
            speakerId={segment.speaker_id}
            speakerName={segment.speaker_name}
            onRenameSpeaker={onRenameSpeaker}
          />
        ) : (
          <div style={{
            fontSize: "11px",
            fontWeight: "bold",
            padding: "2px 8px",
            borderRadius: "6px",
            backgroundColor: "rgba(144, 144, 168, 0.1)",
            color: "var(--color-text-secondary)",
            border: "1px solid var(--color-border)"
          }}>
            Unknown Speaker
          </div>
        )}

        <Text style={{
          fontSize: "11px",
          fontFamily: "monospace",
          color: "var(--color-text-secondary)",
          backgroundColor: "rgba(255, 255, 255, 0.02)",
          padding: "2px 6px",
          borderRadius: "4px",
          border: "1px solid var(--color-border)"
        }}>
          {formatTime(segment.start_time)} – {formatTime(segment.end_time)}
        </Text>
      </HStack>

      <Text style={{
        fontSize: "14px",
        lineHeight: "1.5",
        color: "var(--color-text-primary)",
        fontWeight: "400",
        whiteSpace: "pre-wrap"
      }}>
        {segment.text}
      </Text>
    </Card>
  );
}
