import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

interface WaveformCanvasProps {
  active?: boolean;
  className?: string;
  rmsProp?: number; // Optional prop to manually control the waveform
}

export default function WaveformCanvas({ active = true, className, rmsProp }: WaveformCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rmsRef = useRef<number>(0);

  // Sync prop or listen to Tauri events
  useEffect(() => {
    if (rmsProp !== undefined) {
      rmsRef.current = rmsProp;
      return;
    }

    if (!active) {
      rmsRef.current = 0;
      return;
    }

    let unlistenAudioLevel: (() => void) | undefined;
    let unlistenAudioLevelUnderscore: (() => void) | undefined;

    // Listen to "audio-level" Tauri event
    listen<number>("audio-level", (event) => {
      const val = typeof event.payload === "number" ? event.payload : 0;
      rmsRef.current = val;
    }).then((unsub) => {
      unlistenAudioLevel = unsub;
    });

    // Listen to "audio_level" Tauri event (just in case)
    listen<number>("audio_level", (event) => {
      const val = typeof event.payload === "number" ? event.payload : 0;
      rmsRef.current = val;
    }).then((unsub) => {
      unlistenAudioLevelUnderscore = unsub;
    });

    return () => {
      if (unlistenAudioLevel) unlistenAudioLevel();
      if (unlistenAudioLevelUnderscore) unlistenAudioLevelUnderscore();
    };
  }, [active, rmsProp]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let animationId: number;
    const barCount = 18;
    // Current animated height for each bar (0 to 1)
    const currentHeights = new Array(barCount).fill(0.05);
    // Phase for each bar to create a natural idle animation when no signal is present
    const phases = new Array(barCount).fill(0).map(() => Math.random() * Math.PI * 2);

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

      const targetRms = rmsRef.current;
      
      // Calculate target heights for each bar
      const targetHeights = new Array(barCount).fill(0);
      for (let i = 0; i < barCount; i++) {
        // Create a symmetric dome shape (Gaussian-like distribution)
        // Center of the waveform is at barCount / 2
        const distFromCenter = Math.abs(i - (barCount - 1) / 2);
        const centerFactor = Math.exp(-Math.pow(distFromCenter / (barCount / 3.5), 2));
        
        let target = 0.05; // Baseline idle height
        
        if (active) {
          if (targetRms > 0.01) {
            // Under active signal, compute height based on RMS and center weighting
            const randomJitter = 0.4 + Math.random() * 0.8;
            target = targetRms * centerFactor * randomJitter;
          } else {
            // Idle animation - a gentle breathing sine wave
            phases[i] += 0.05;
            target = 0.05 + 0.12 * Math.sin(phases[i]) * centerFactor;
          }
        }
        
        // Clamp target
        targetHeights[i] = Math.max(0.05, Math.min(1.0, target));
      }

      // Smoothly interpolate current heights to target heights
      for (let i = 0; i < barCount; i++) {
        // Faster upward response, slower downward decay for a responsive feel
        const rate = targetHeights[i] > currentHeights[i] ? 0.35 : 0.18;
        currentHeights[i] += (targetHeights[i] - currentHeights[i]) * rate;
      }

      // Draw bars
      const padding = 2;
      const totalSpacing = padding * (barCount - 1);
      const barWidth = (width - totalSpacing) / barCount;

      // Color/Gradient setup
      // Retrieve theme color from computed style or fallback to a gorgeous gradient
      const accentColor = getComputedStyle(document.documentElement)
        .getPropertyValue("--color-accent")
        .trim() || "#7c3aed";
      
      const gradient = ctx.createLinearGradient(0, height, 0, 0);
      gradient.addColorStop(0, accentColor);
      gradient.addColorStop(1, "#a78bfa"); // Lighter violet for peak

      for (let i = 0; i < barCount; i++) {
        const barHeight = currentHeights[i] * height;
        const x = i * (barWidth + padding);
        const y = (height - barHeight) / 2; // Center vertically

        // Draw rounded rectangle
        ctx.fillStyle = gradient;
        ctx.beginPath();
        if (ctx.roundRect) {
          ctx.roundRect(x, y, barWidth, barHeight, barWidth / 2);
        } else {
          // Fallback roundRect implementation
          const r = barWidth / 2;
          ctx.moveTo(x + r, y);
          ctx.arcTo(x + barWidth, y, x + barWidth, y + barHeight, r);
          ctx.arcTo(x + barWidth, y + barHeight, x, y + barHeight, r);
          ctx.arcTo(x, y + barHeight, x, y, r);
          ctx.arcTo(x, y, x + barWidth, y, r);
        }
        ctx.fill();
      }

      animationId = requestAnimationFrame(render);
    };

    render();

    return () => {
      cancelAnimationFrame(animationId);
    };
  }, [active]);

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
