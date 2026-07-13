import { useEffect, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { Card } from "@astryxdesign/core/Card";
import { Text } from "@astryxdesign/core/Text";
import { HStack } from "@astryxdesign/core/Layout";

interface AudioPlayerProps {
  meetingId: string;
}

const SpeakerIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 16, height: 16 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M19.114 5.636a9 9 0 0 1 0 12.728M16.463 8.288a5.25 5.25 0 0 1 0 7.424M6.75 8.25l4.72-4.72a.75.75 0 0 1 1.28.53v15.88a.75.75 0 0 1-1.28.53l-4.72-4.72H4.51c-.88 0-1.704-.507-1.938-1.354A9.009 9.009 0 0 1 2.25 12c0-.83.112-1.633.322-2.396C2.806 8.756 3.63 8.25 4.51 8.25H6.75Z" />
  </svg>
);

/**
 * AudioPlayer — renders a native audio player for a saved meeting.
 *
 * Resolves the WAV path via `get_meeting_audio_path` and converts it to a
 * playable src with `convertFileSrc`. Renders nothing if there's no path or
 * Tauri is unavailable.
 */
export default function AudioPlayer({ meetingId }: AudioPlayerProps) {
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setSrc(null);
    (async () => {
      try {
        const path = await invoke<string | null>("get_meeting_audio_path", { meetingId });
        if (cancelled) return;
        if (path) {
          setSrc(convertFileSrc(path));
        }
      } catch (err) {
        console.warn("get_meeting_audio_path unavailable:", err);
      }
    })();
    return () => { cancelled = true; };
  }, [meetingId]);

  if (!src) return null;

  return (
    <Card style={{
      padding: "14px 16px",
      backgroundColor: "var(--color-background-surface)",
      border: "1px solid var(--color-border)",
      borderRadius: "12px",
      display: "flex",
      flexDirection: "column",
      gap: "10px",
    }}>
      <HStack gap={2} style={{ alignItems: "center", color: "var(--color-text-secondary)" }}>
        <SpeakerIcon />
        <Text style={{ fontSize: "13px", fontWeight: 600, color: "var(--color-text-secondary)" }}>
          Meeting Audio
        </Text>
      </HStack>
      <audio controls src={src} style={{ width: "100%" }} />
    </Card>
  );
}
