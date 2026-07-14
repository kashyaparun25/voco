import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

interface WaveformCanvasProps {
  active?: boolean;
  className?: string;
  rmsProp?: number; // Optional prop to manually control the waveform
}

/**
 * Voice-driven scrolling waveform.
 *
 * Every incoming RMS level (one per 20ms audio block from the backend) becomes
 * a bar; bars scroll left as new audio arrives, so the shape IS the recent
 * amplitude envelope of the user's voice — not a canned animation.
 *
 * Real mic levels are tiny and vary hugely with the OS input volume (this
 * user's speech averages ~0.01 RMS), so bars are normalized adaptively:
 * a tracked noise floor maps to the baseline and a slowly-decaying peak maps
 * to full height, with sqrt shaping for perceptual dynamics. Whispering and
 * shouting both produce a full, honest waveform.
 */
export default function WaveformCanvas({ active = true, className, rmsProp }: WaveformCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // Rolling raw-RMS history, newest last. Sized generously; the renderer takes
  // the last `barCount` entries.
  const levelsRef = useRef<number[]>([]);
  const floorRef = useRef(0.004); // adaptive noise floor (snap down, creep up)
  const peakRef = useRef(0.02); // adaptive peak (snap up, decay down)
  const activeRef = useRef(active);
  activeRef.current = active;

  const pushLevel = (rms: number) => {
    if (!Number.isFinite(rms) || rms < 0) return;
    const floor = floorRef.current;
    floorRef.current = rms < floor ? rms : Math.min(floor * 1.02 + 1e-5, rms);
    peakRef.current = Math.max(peakRef.current * 0.992, rms, floorRef.current + 0.006);
    const levels = levelsRef.current;
    levels.push(rms);
    if (levels.length > 128) levels.splice(0, levels.length - 128);
  };
  const pushLevelRef = useRef(pushLevel);
  pushLevelRef.current = pushLevel;

  // Feed levels: from the prop when provided, else from Tauri events.
  useEffect(() => {
    if (rmsProp !== undefined) {
      pushLevelRef.current(rmsProp);
      return;
    }

    if (!active) return;

    const unlisteners: Array<() => void> = [];
    for (const name of ["dictation-audio-level", "audio-level", "audio_level"]) {
      listen<number>(name, (event) => {
        if (typeof event.payload === "number") pushLevelRef.current(event.payload);
      })
        .then((unsub) => unlisteners.push(unsub))
        .catch(() => {
          /* Tauri unavailable */
        });
    }

    return () => {
      for (const un of unlisteners) {
        try {
          un();
        } catch {
          /* no-op */
        }
      }
    };
  }, [active, rmsProp]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let animationId: number;
    // Fixed bar geometry; the COUNT adapts to the available width. (A fixed
    // count made bars sub-pixel — 0.6px — inside the 120px pill: invisible.)
    const BAR_W = 3;
    const GAP = 2;
    const MAX_BARS = 64;
    const BASELINE = 0.1;
    // Current animated height for each bar slot (0 to 1), newest at the end.
    const currentHeights = new Array(MAX_BARS).fill(BASELINE);

    // Map a raw RMS to 0..1 against the adaptive floor/peak window.
    const normalize = (rms: number) => {
      const floor = floorRef.current;
      const span = Math.max(peakRef.current - floor, 0.003);
      const n = Math.max(0, Math.min(1, (rms - floor) / span));
      return Math.sqrt(n); // perceptual shaping: quiet speech stays visible
    };

    const render = () => {
      const dpr = window.devicePixelRatio || 1;
      const rect = canvas.getBoundingClientRect();

      // Handle high-DPI scaling
      if (canvas.width !== rect.width * dpr || canvas.height !== rect.height * dpr) {
        canvas.width = rect.width * dpr;
        canvas.height = rect.height * dpr;
        ctx.scale(dpr, dpr);
      }

      const width = rect.width;
      const height = rect.height;

      ctx.clearRect(0, 0, width, height);

      // As many fixed-width bars as fit, drawn centered; the newest audio is
      // the rightmost bar.
      const barCount = Math.min(MAX_BARS, Math.max(8, Math.floor((width + GAP) / (BAR_W + GAP))));
      const levels = levelsRef.current;
      for (let i = 0; i < barCount; i++) {
        const levelIdx = levels.length - barCount + i;
        let target = BASELINE;
        if (activeRef.current && levelIdx >= 0) {
          target = BASELINE + (1 - BASELINE) * normalize(levels[levelIdx]);
        }
        // Fast attack, slower release — but bars mostly move by scrolling.
        const rate = target > currentHeights[i] ? 0.5 : 0.3;
        currentHeights[i] += (target - currentHeights[i]) * rate;
      }
      if (!activeRef.current && levels.length) {
        // Session over: let the old envelope drain out instead of freezing.
        levels.splice(0, Math.max(1, Math.ceil(levels.length / 20)));
      }

      const accentColor =
        getComputedStyle(document.documentElement).getPropertyValue("--color-accent").trim() ||
        "#7c3aed";

      const gradient = ctx.createLinearGradient(0, height, 0, 0);
      gradient.addColorStop(0, accentColor);
      gradient.addColorStop(1, "#a78bfa"); // Lighter violet for peak

      const xOffset = (width - (barCount * (BAR_W + GAP) - GAP)) / 2;
      for (let i = 0; i < barCount; i++) {
        const barHeight = Math.max(currentHeights[i] * height, 2);
        const x = xOffset + i * (BAR_W + GAP);
        const y = (height - barHeight) / 2; // Center vertically

        ctx.fillStyle = gradient;
        ctx.beginPath();
        if (ctx.roundRect) {
          ctx.roundRect(x, y, BAR_W, barHeight, BAR_W / 2);
        } else {
          const r = BAR_W / 2;
          ctx.moveTo(x + r, y);
          ctx.arcTo(x + BAR_W, y, x + BAR_W, y + barHeight, r);
          ctx.arcTo(x + BAR_W, y + barHeight, x, y + barHeight, r);
          ctx.arcTo(x, y + barHeight, x, y, r);
          ctx.arcTo(x, y, x + BAR_W, y, r);
        }
        ctx.fill();
      }

      animationId = requestAnimationFrame(render);
    };

    render();

    return () => {
      cancelAnimationFrame(animationId);
    };
  }, []);

  return (
    <canvas
      ref={canvasRef}
      className={className}
      style={{
        width: "100%",
        height: "100%",
        display: "block",
      }}
    />
  );
}
