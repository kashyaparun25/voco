import { useEffect, useRef, useState, useCallback } from "react";

/**
 * useDictation
 *
 * Manages dictation state by talking to the Tauri backend.
 *
 * Backend commands invoked: `start_dictation`, `stop_dictation`.
 *
 * Backend events subscribed to (names confirmed from
 * src-tauri/src/services/dictation.rs plus defensive fallbacks):
 *   - `dictation-status`       -> payload is the DictationStatus enum,
 *                                 serialized as the string "Idle" | "Recording" | "Processing"
 *                                 (also handles object payloads like { status } / { state }).
 *   - `dictation-audio-level`  -> RMS number (confirmed emitted name).
 *   - `dictation-level`        -> RMS number (fallback / alias).
 *   - `dictation-final`        -> final transcription text (string).
 *   - `dictation-partial`      -> partial text (fallback).
 *   - `transcription-partial`  -> partial text (fallback).
 *   - `partial-transcription`  -> partial text (fallback).
 *   - `trigger-dictation-toggle` -> backend asks the UI to toggle dictation.
 *
 * All invoke/listen calls are wrapped in try/catch so the hook degrades
 * gracefully when Tauri isn't available (e.g. browser preview).
 */

export interface UseDictationResult {
  isRecording: boolean;
  partialText: string;
  audioLevel: number;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  toggle: () => Promise<void>;
}

// Throttle audio-level state updates to ~60fps to avoid thrashing React.
const LEVEL_THROTTLE_MS = 1000 / 60;

function extractText(payload: unknown): string {
  if (typeof payload === "string") return payload;
  if (payload && typeof payload === "object") {
    const obj = payload as Record<string, unknown>;
    const candidate = obj.text ?? obj.partial ?? obj.value;
    if (typeof candidate === "string") return candidate;
  }
  return "";
}

function extractStatus(payload: unknown): string {
  if (typeof payload === "string") return payload;
  if (payload && typeof payload === "object") {
    const obj = payload as Record<string, unknown>;
    const candidate = obj.status ?? obj.state;
    if (typeof candidate === "string") return candidate;
  }
  return "";
}

function extractLevel(payload: unknown): number | null {
  if (typeof payload === "number") return payload;
  if (payload && typeof payload === "object") {
    const obj = payload as Record<string, unknown>;
    const candidate = obj.level ?? obj.rms ?? obj.value;
    if (typeof candidate === "number") return candidate;
  }
  return null;
}

export function useDictation(): UseDictationResult {
  const [isRecording, setIsRecording] = useState(false);
  const [partialText, setPartialText] = useState("");
  const [audioLevel, setAudioLevel] = useState(0);

  // Latest recording state, so `toggle` can read it without re-subscribing.
  const isRecordingRef = useRef(false);
  useEffect(() => {
    isRecordingRef.current = isRecording;
  }, [isRecording]);

  // Throttle for audio level updates.
  const lastLevelUpdateRef = useRef(0);
  const pendingLevelRef = useRef<number | null>(null);
  const levelRafRef = useRef<number | null>(null);

  const flushLevel = useCallback(() => {
    levelRafRef.current = null;
    if (pendingLevelRef.current != null) {
      setAudioLevel(pendingLevelRef.current);
      pendingLevelRef.current = null;
      lastLevelUpdateRef.current = performance.now();
    }
  }, []);

  const pushLevel = useCallback(
    (value: number) => {
      pendingLevelRef.current = value;
      const now = performance.now();
      if (now - lastLevelUpdateRef.current >= LEVEL_THROTTLE_MS) {
        flushLevel();
      } else if (levelRafRef.current == null) {
        levelRafRef.current = requestAnimationFrame(flushLevel);
      }
    },
    [flushLevel]
  );

  const start = useCallback(async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("start_dictation");
      // Optimistic; backend will confirm via `dictation-status`.
      setIsRecording(true);
      setPartialText("");
    } catch (err) {
      console.warn("useDictation.start failed:", err);
      // Surface the reason (most commonly: no STT model downloaded yet).
      try {
        const { showToast } = await import("./useToast");
        const msg = String(err);
        if (/model/i.test(msg) && /download/i.test(msg)) {
          showToast("No dictation model yet — download one in Settings → Models.", "error");
        } else {
          showToast(`Dictation couldn't start: ${msg}`, "error");
        }
      } catch { /* toast unavailable */ }
    }
  }, []);

  const stop = useCallback(async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("stop_dictation");
      setIsRecording(false);
    } catch (err) {
      console.warn("useDictation.stop failed (Tauri unavailable?):", err);
    }
  }, []);

  const toggle = useCallback(async () => {
    if (isRecordingRef.current) {
      await stop();
    } else {
      await start();
    }
  }, [start, stop]);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    const setup = async () => {
      let listen: typeof import("@tauri-apps/api/event").listen;
      try {
        ({ listen } = await import("@tauri-apps/api/event"));
      } catch (err) {
        console.warn("useDictation: Tauri event API unavailable:", err);
        return;
      }

      const safeListen = async (
        event: string,
        handler: (payload: unknown) => void
      ) => {
        try {
          const un = await listen<unknown>(event, (e) => handler(e.payload));
          if (cancelled) {
            un();
          } else {
            unlisteners.push(un);
          }
        } catch (err) {
          console.warn(`useDictation: failed to listen to "${event}":`, err);
        }
      };

      await Promise.all([
        safeListen("dictation-status", (payload) => {
          const status = extractStatus(payload).toLowerCase();
          if (status === "recording") {
            setIsRecording(true);
          } else if (status === "idle" || status === "processing") {
            setIsRecording(false);
            if (status === "idle") setPartialText("");
          }
        }),
        safeListen("dictation-audio-level", (payload) => {
          const level = extractLevel(payload);
          if (level != null) pushLevel(level);
        }),
        safeListen("dictation-level", (payload) => {
          const level = extractLevel(payload);
          if (level != null) pushLevel(level);
        }),
        safeListen("dictation-final", (payload) => {
          setPartialText(extractText(payload));
        }),
        safeListen("dictation-partial", (payload) => {
          setPartialText(extractText(payload));
        }),
        safeListen("transcription-partial", (payload) => {
          setPartialText(extractText(payload));
        }),
        safeListen("partial-transcription", (payload) => {
          setPartialText(extractText(payload));
        }),
        safeListen("trigger-dictation-toggle", () => {
          void toggle();
        }),
      ]);
    };

    void setup();

    return () => {
      cancelled = true;
      for (const un of unlisteners) {
        try {
          un();
        } catch {
          /* no-op */
        }
      }
      if (levelRafRef.current != null) {
        cancelAnimationFrame(levelRafRef.current);
        levelRafRef.current = null;
      }
    };
  }, [pushLevel, toggle]);

  return { isRecording, partialText, audioLevel, start, stop, toggle };
}

export default useDictation;
