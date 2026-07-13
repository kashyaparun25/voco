import React from "react";
import { Text } from "@astryxdesign/core/Text";

interface MeetingTimerProps {
  seconds: number;
  style?: React.CSSProperties;
}

export default function MeetingTimer({ seconds, style }: MeetingTimerProps) {
  const formatTime = (totalSeconds: number) => {
    const hrs = Math.floor(totalSeconds / 3600);
    const mins = Math.floor((totalSeconds % 3600) / 60);
    const secs = totalSeconds % 60;
    
    const parts = [
      ...(hrs > 0 ? [hrs.toString().padStart(2, "0")] : []),
      mins.toString().padStart(2, "0"),
      secs.toString().padStart(2, "0")
    ];
    
    return parts.join(":");
  };

  return (
    <div style={{
      display: "flex",
      alignItems: "center",
      gap: "8px",
      fontFamily: "monospace",
      fontSize: "18px",
      fontWeight: "bold",
      color: "var(--color-text-primary)",
      ...style
    }}>
      <span style={{
        width: "8px",
        height: "8px",
        borderRadius: "50%",
        backgroundColor: "var(--color-recording)",
        display: "inline-block",
        boxShadow: "0 0 8px var(--color-recording)",
        animation: "pill-pulse 1.5s infinite ease-in-out"
      }} />
      <Text style={{ fontSize: "18px", fontWeight: "600", fontFamily: "var(--font-family-body)" }}>
        {formatTime(seconds)}
      </Text>
    </div>
  );
}
