import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import WaveformCanvas from "../components/waveform/WaveformCanvas";

type PillState = "recording" | "processing" | "idle";

export default function PillWindow() {
  const [state, setState] = useState<PillState>("recording");
  const [rms, setRms] = useState<number>(0);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    const sub = <T,>(event: string, handler: (payload: T) => void) => {
      listen<T>(event, (e) => handler(e.payload))
        .then((un) => unlisteners.push(un))
        .catch(() => { /* Tauri unavailable */ });
    };

    sub<number>("dictation-audio-level", (v) => setRms(typeof v === "number" ? v : 0));
    sub<unknown>("dictation-status", (status) => {
      const s = String(status).toLowerCase();
      if (s.includes("record")) setState("recording");
      else if (s.includes("process")) setState("processing");
      else if (s.includes("idle")) setState("idle");
    });

    return () => { for (const un of unlisteners) { try { un(); } catch { /* no-op */ } } };
  }, []);

  const handleStop = async () => {
    try {
      setState("processing");
      await invoke("stop_dictation");
    } catch (err) {
      console.error("Failed to stop dictation:", err);
    }
  };

  const dotColor =
    state === "recording" ? "var(--color-recording)"
    : state === "processing" ? "var(--color-accent)"
    : "var(--color-text-secondary)";

  return (
    <div className="pill-container" data-tauri-drag-region>
      <div
        className="pill-status-dot"
        style={{ backgroundColor: dotColor, animation: state === "recording" ? undefined : "none" }}
        title={state}
      />

      {/* Waveform fills the pill — no live text (Whisper isn't a streaming model). */}
      <div className="pill-waveform-wrapper" data-tauri-drag-region>
        <WaveformCanvas active={state === "recording"} rmsProp={rms} />
      </div>

      <div className="pill-actions">
        <button className="pill-button stop-btn" onClick={handleStop} title="Stop dictation">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" style={{ width: 15, height: 15 }}>
            <path fillRule="evenodd" d="M4.5 7.5a3 3 0 0 1 3-3h9a3 3 0 0 1 3 3v9a3 3 0 0 1-3 3h-9a3 3 0 0 1-3-3v-9Z" clipRule="evenodd" />
          </svg>
        </button>
      </div>
    </div>
  );
}
