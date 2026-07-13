import React from "react";
import { Button } from "../ui";
import { HStack } from "@astryxdesign/core/Layout";

interface MeetingControlsProps {
  isRecording: boolean;
  isPaused: boolean;
  onStart: () => void;
  onPause: () => void;
  onResume: () => void;
  onStop: () => void;
  style?: React.CSSProperties;
}

const PlayIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 24 24" style={{ width: 16, height: 16 }}>
    <path d="M8 5v14l11-7z" />
  </svg>
);

const PauseIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 24 24" style={{ width: 16, height: 16 }}>
    <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
  </svg>
);

const StopIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 24 24" style={{ width: 16, height: 16 }}>
    <path d="M6 6h12v12H6z" />
  </svg>
);

export default function MeetingControls({
  isRecording,
  isPaused,
  onStart,
  onPause,
  onResume,
  onStop,
  style
}: MeetingControlsProps) {
  return (
    <HStack gap={3} style={{ alignItems: "center", ...style }}>
      {!isRecording ? (
        <Button
          variant="primary"
          onClick={onStart}
          label="Start Meeting"
          icon={<PlayIcon />}
          style={{
            padding: "12px 24px",
            borderRadius: "999px",
            backgroundColor: "var(--color-accent)",
            color: "#ffffff",
            fontWeight: "600",
            cursor: "pointer",
            boxShadow: "0 4px 12px rgba(124, 58, 237, 0.25)",
            border: "none",
            transition: "all 0.2s ease"
          }}
        />
      ) : (
        <>
          {isPaused ? (
            <Button
              variant="primary"
              onClick={onResume}
              label="Resume"
              icon={<PlayIcon />}
              style={{
                padding: "10px 20px",
                borderRadius: "999px",
                backgroundColor: "var(--color-accent)",
                color: "#ffffff",
                fontWeight: "600",
                cursor: "pointer",
                border: "none",
                transition: "all 0.2s ease"
              }}
            />
          ) : (
            <Button
              variant="secondary"
              onClick={onPause}
              label="Pause"
              icon={<PauseIcon />}
              style={{
                padding: "10px 20px",
                borderRadius: "999px",
                backgroundColor: "var(--color-background-surface-hover)",
                color: "var(--color-text-primary)",
                fontWeight: "600",
                cursor: "pointer",
                border: "1px solid var(--color-border-strong)",
                transition: "all 0.2s ease"
              }}
            />
          )}
          
          <Button
            variant="secondary"
            onClick={onStop}
            label="Stop Meeting"
            icon={<StopIcon />}
            style={{
              padding: "10px 20px",
              borderRadius: "999px",
              backgroundColor: "rgba(239, 68, 68, 0.1)",
              color: "var(--color-recording)",
              fontWeight: "600",
              cursor: "pointer",
              border: "1px solid rgba(239, 68, 68, 0.3)",
              transition: "all 0.2s ease"
            }}
          />
        </>
      )}
    </HStack>
  );
}
