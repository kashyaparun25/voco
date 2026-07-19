import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import WaveformCanvas from "../components/waveform/WaveformCanvas";

type PillState = "recording" | "processing" | "idle";

export default function PillWindow() {
  const [state, setState] = useState<PillState>("recording");
  const [partial, setPartial] = useState<string>("");
  // The backend sizes the pill window taller when live preview is active —
  // infer the layout from the window height so the two stay in lockstep.
  const [expanded, setExpanded] = useState<boolean>(window.innerHeight > 50);

  useEffect(() => {
    const onResize = () => setExpanded(window.innerHeight > 50);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    const sub = <T,>(event: string, handler: (payload: T) => void) => {
      listen<T>(event, (e) => handler(e.payload))
        .then((un) => unlisteners.push(un))
        .catch(() => { /* Tauri unavailable */ });
    };

    sub<unknown>("dictation-status", (status) => {
      const s = String(status).toLowerCase();
      if (s.includes("record")) {
        // New session: clear any live text from the previous one.
        setPartial("");
        setState("recording");
      } else if (s.includes("process")) setState("processing");
      else if (s.includes("idle")) { setState("idle"); setPartial(""); }
    });

    // Live transcript preview (fast local engines only). Keep only the tail —
    // the pill is one line, and the newest words are what matter.
    sub<string>("dictation-partial", (text) => {
      const t = String(text ?? "");
      setPartial(t.length > 220 ? t.slice(t.length - 220) : t);
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

  const showLiveText = state === "recording" && partial.length > 0;

  return (
    <div className="pill-container" data-tauri-drag-region>
      <div
        className="pill-status-dot"
        style={{ backgroundColor: dotColor, animation: state === "recording" ? undefined : "none" }}
        title={state}
      />

      {state === "processing" ? (
        /* Transcription + paste still running: keep the pill up (with the last
           live words if we have them) so the user knows the recording
           registered. The backend hides the pill after the text is pasted. */
        <div className="pill-processing" data-tauri-drag-region style={{ display: "flex", alignItems: "center", gap: 8, flex: 1, minWidth: 0 }}>
          <div className="pill-spinner" />
          {partial && (
            <div style={liveTextStyle} data-tauri-drag-region>
              <span style={{ opacity: 0.7 }}>{partial}</span>
            </div>
          )}
        </div>
      ) : (
        <>
          {expanded ? (
            /* Live-preview layout: words on top, waveform underneath — both
               always visible while recording. */
            <div style={stackStyle} data-tauri-drag-region>
              <div style={liveTextStyle} data-tauri-drag-region>
                {showLiveText ? partial : <span style={{ opacity: 0.45 }}>…</span>}
              </div>
              <div style={{ height: 12, minWidth: 0 }} data-tauri-drag-region>
                <WaveformCanvas active={state === "recording"} />
              </div>
            </div>
          ) : (
            <div className="pill-waveform-wrapper" data-tauri-drag-region>
              <WaveformCanvas active={state === "recording"} />
            </div>
          )}

          <div className="pill-actions">
            <button className="pill-button stop-btn" onClick={handleStop} title="Stop dictation">
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" style={{ width: 15, height: 15 }}>
                <path fillRule="evenodd" d="M4.5 7.5a3 3 0 0 1 3-3h9a3 3 0 0 1 3 3v9a3 3 0 0 1-3 3h-9a3 3 0 0 1-3-3v-9Z" clipRule="evenodd" />
              </svg>
            </button>
          </div>
        </>
      )}
    </div>
  );
}

/* Two stacked rows filling the middle of the pill. */
const stackStyle: React.CSSProperties = {
  flex: 1,
  minWidth: 0,
  display: "flex",
  flexDirection: "column",
  justifyContent: "center",
  gap: 3,
};

/* One compact line, clipped from the left so the newest words stay visible. */
const liveTextStyle: React.CSSProperties = {
  flex: "0 0 auto",
  minWidth: 0,
  overflow: "hidden",
  whiteSpace: "nowrap",
  display: "flex",
  justifyContent: "flex-end",
  fontSize: 11,
  lineHeight: "14px",
  color: "var(--color-text-primary)",
  transition: "opacity 0.15s ease",
};
