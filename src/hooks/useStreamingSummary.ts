import { useEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

type SummaryLength = "short" | "medium" | "long";
type SummaryStyle = "bullets" | "paragraphs" | "action";

interface SummaryTokenPayload {
  meeting_id: string;
  token: string;
}
interface SummaryDonePayload {
  meeting_id: string;
  summary: string;
}

export interface UseStreamingSummaryResult {
  /** Live text accumulated from summary-token events (empty when idle). */
  streamingText: string;
  /** True while a stream is in-flight. */
  isStreaming: boolean;
  /**
   * Kick off a streaming summary for a meeting. Falls back to the given
   * non-streaming invoker if the streaming command errors. Resolves to the
   * final summary string, or null on failure.
   */
  generate: (
    meetingId: string,
    length: SummaryLength,
    style: SummaryStyle,
    command: "summarize_meeting_streaming",
    fallbackCommand: "summarize_meeting" | "regenerate_summary"
  ) => Promise<string | null>;
}

/**
 * useStreamingSummary — drives the live-token summary flow.
 *
 * Subscribes to `summary-token` / `summary-done` for the current meeting,
 * appending tokens to `streamingText`. Everything is wrapped in try/catch so
 * it degrades gracefully when Tauri or the streaming command is unavailable.
 */
export function useStreamingSummary(): UseStreamingSummaryResult {
  const [streamingText, setStreamingText] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const activeMeetingRef = useRef<string | null>(null);
  const doneResolveRef = useRef<((summary: string) => void) | null>(null);
  const accumulatedRef = useRef("");
  const unlistensRef = useRef<UnlistenFn[]>([]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const unToken = await listen<SummaryTokenPayload>("summary-token", (event) => {
          const { meeting_id, token } = event.payload || ({} as SummaryTokenPayload);
          if (!activeMeetingRef.current || meeting_id !== activeMeetingRef.current) return;
          accumulatedRef.current += token ?? "";
          setStreamingText(accumulatedRef.current);
        });
        const unDone = await listen<SummaryDonePayload>("summary-done", (event) => {
          const { meeting_id, summary } = event.payload || ({} as SummaryDonePayload);
          if (!activeMeetingRef.current || meeting_id !== activeMeetingRef.current) return;
          const finalText = summary || accumulatedRef.current;
          doneResolveRef.current?.(finalText);
          doneResolveRef.current = null;
        });
        if (cancelled) {
          unToken();
          unDone();
          return;
        }
        unlistensRef.current = [unToken, unDone];
      } catch {
        // Tauri unavailable — streaming simply won't emit; callers fall back.
      }
    })();
    return () => {
      cancelled = true;
      unlistensRef.current.forEach((fn) => {
        try { fn(); } catch { /* ignore */ }
      });
      unlistensRef.current = [];
    };
  }, []);

  const generate = useCallback<UseStreamingSummaryResult["generate"]>(
    async (meetingId, length, style, command, fallbackCommand) => {
      activeMeetingRef.current = meetingId;
      accumulatedRef.current = "";
      setStreamingText("");
      setIsStreaming(true);

      try {
        // Wire a promise resolved by the summary-done listener, but also
        // accept the command's own return value as a fallback completion.
        const donePromise = new Promise<string>((resolve) => {
          doneResolveRef.current = resolve;
        });

        const invokePromise = invoke<string>(command, { meetingId, length, style });

        // Whichever settles first wins; both represent completion.
        const result = await Promise.race([
          donePromise,
          invokePromise.then((s) => s),
        ]);

        // Ensure any trailing done event has a chance to resolve too.
        const finalText = result || accumulatedRef.current;
        return finalText || null;
      } catch (streamErr) {
        console.warn("Streaming summary failed, falling back:", streamErr);
        try {
          const sum = await invoke<string>(fallbackCommand, { meetingId, length, style });
          return sum;
        } catch (fallbackErr) {
          console.error("Fallback summary failed:", fallbackErr);
          return null;
        }
      } finally {
        setIsStreaming(false);
        doneResolveRef.current = null;
        activeMeetingRef.current = null;
      }
    },
    []
  );

  return { streamingText, isStreaming, generate };
}

export default useStreamingSummary;
