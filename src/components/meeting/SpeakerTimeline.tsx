import React from "react";
import { Card } from "@astryxdesign/core/Card";
import { Text } from "@astryxdesign/core/Text";

export interface TimelineSegment {
  id: string;
  speaker_id: string | null;
  speaker_name: string | null;
  start_time: number;
  end_time: number;
  text: string;
}

interface SpeakerTimelineProps {
  segments: TimelineSegment[];
  duration: number; // in seconds
  style?: React.CSSProperties;
}

export default function SpeakerTimeline({ segments, duration, style }: SpeakerTimelineProps) {
  const getSpeakerColor = (speakerId: string | null | undefined) => {
    if (!speakerId) return "rgba(144, 144, 168, 0.4)";
    let hash = 0;
    for (let i = 0; i < speakerId.length; i++) {
      hash = speakerId.charCodeAt(i) + ((hash << 5) - hash);
    }
    const index = Math.abs(hash % 8) + 1;
    return `var(--color-speaker-${index})`;
  };


  // Group segments by speaker to show a list of speakers involved
  const uniqueSpeakers = Array.from(
    new Map(
      segments
        .filter(s => s.speaker_id)
        .map(s => [s.speaker_id, s.speaker_name || `Speaker ${s.speaker_id}`])
    ).entries()
  );

  const formatTime = (totalSeconds: number) => {
    const mins = Math.floor(totalSeconds / 60);
    const secs = Math.floor(totalSeconds % 60);
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  const totalDuration = duration > 0 ? duration : Math.max(...segments.map(s => s.end_time), 1);

  return (
    <Card style={{
      padding: "16px",
      backgroundColor: "var(--color-background-surface)",
      border: "1px solid var(--color-border)",
      borderRadius: "12px",
      display: "flex",
      flexDirection: "column",
      gap: "12px",
      boxShadow: "0 2px 8px rgba(0, 0, 0, 0.1)",
      ...style
    }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ fontSize: "14px", fontWeight: "600", color: "var(--color-text-primary)" }}>
          Speaker Timeline
        </Text>
        <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
          Duration: {formatTime(totalDuration)}
        </Text>
      </div>

      {/* The timeline track */}
      <div style={{
        position: "relative",
        height: "36px",
        backgroundColor: "rgba(255, 255, 255, 0.03)",
        border: "1px solid var(--color-border)",
        borderRadius: "8px",
        overflow: "hidden",
        display: "flex",
        alignItems: "center"
      }}>
        {segments.map((seg) => {
          // Calculate percentage width and position
          const leftPercent = Math.max(0, Math.min(100, (seg.start_time / totalDuration) * 100));
          const widthPercent = Math.max(0.5, Math.min(100, ((seg.end_time - seg.start_time) / totalDuration) * 100));

          if (leftPercent >= 100) return null;

          return (
            <div
              key={seg.id}
              title={`${seg.speaker_name || "Unknown Speaker"}: ${formatTime(seg.start_time)} - ${formatTime(seg.end_time)}\n"${seg.text.slice(0, 60)}..."`}
              style={{
                position: "absolute",
                left: `${leftPercent}%`,
                width: `${widthPercent}%`,
                height: "80%",
                backgroundColor: getSpeakerColor(seg.speaker_id),
                borderRadius: "4px",
                opacity: 0.85,
                cursor: "pointer",
                transition: "opacity 0.2s ease, transform 0.1s ease",
                boxShadow: "0 1px 3px rgba(0,0,0,0.2)",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.opacity = "1";
                e.currentTarget.style.transform = "scaleY(1.1)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.opacity = "0.85";
                e.currentTarget.style.transform = "none";
              }}
            />
          );
        })}

        {segments.length === 0 && (
          <div style={{
            width: "100%",
            display: "flex",
            justifyContent: "center",
            alignItems: "center",
            color: "var(--color-text-secondary)",
            fontSize: "12px",
            fontStyle: "italic"
          }}>
            Awaiting transcription segments...
          </div>
        )}
      </div>

      {/* Axis markers */}
      <div style={{
        display: "flex",
        justifyContent: "space-between",
        fontSize: "10px",
        color: "var(--color-text-secondary)",
        padding: "0 4px",
        marginTop: "-4px",
        fontFamily: "monospace"
      }}>
        <span>0:00</span>
        <span>{formatTime(totalDuration / 2)}</span>
        <span>{formatTime(totalDuration)}</span>
      </div>

      {/* Speaker legend */}
      {uniqueSpeakers.length > 0 && (
        <div style={{
          display: "flex",
          flexWrap: "wrap",
          gap: "12px",
          marginTop: "4px",
          paddingTop: "8px",
          borderTop: "1px solid var(--color-border)"
        }}>
          {uniqueSpeakers.map(([id, name]) => (
            <div key={id} style={{ display: "flex", alignItems: "center", gap: "6px" }}>
              <span style={{
                width: "10px",
                height: "10px",
                borderRadius: "50%",
                backgroundColor: getSpeakerColor(id),
                display: "inline-block"
              }} />
              <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
                {name}
              </Text>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}
