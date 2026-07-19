import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../ui";

/**
 * FirstRunOnboarding — full-window six-step setup wizard shown once on first
 * launch. Completion is stored in the backend setting "onboarding_complete"
 * (the caller decides visibility and also honors the legacy localStorage flag).
 *
 * Steps: Welcome → Permissions → Dictation model → Meeting intelligence →
 * AI notes → Done. Every setup step is skippable; permissions and models can
 * always be finished later from Getting Started / Settings.
 */

interface FirstRunOnboardingProps {
  isOpen: boolean;
  /** Mark onboarding complete and unmount the wizard. */
  onComplete: () => void;
  /** Mark complete, unmount, and navigate to the AI providers settings section. */
  onOpenAiSettings: () => void;
}

interface ModelInfo {
  id: string;
  name: string;
  size_bytes: number;
  is_downloaded: boolean;
  progress: number; // 0.0 to 1.0
  category: string; // "stt" | "llm" | "vad"
}

const STEPS = ["welcome", "permissions", "dictation", "meeting", "ai", "done"] as const;
type StepKey = (typeof STEPS)[number];

const DICTATION_MODEL_ID = "parakeet-tdt-v3";
const MEETING_MODEL_ID = "moss-transcribe-diarize";

const HOTKEY_LABELS: Record<string, string> = {
  LeftOption: "Left Option ⌥",
  RightOption: "Right Option ⌥",
  "double:LeftOption": "Double-tap Left Option ⌥",
  "double:RightOption": "Double-tap Right Option ⌥",
  Fn: "Fn / Globe 🌐",
  "double:Fn": "Double-tap Fn 🌐",
  LeftControl: "Left Control ⌃",
  "CommandOrControl+Shift+Space": "⌘ ⇧ Space",
  "Alt+Space": "⌥ Space",
};

function formatSize(bytes: number): string {
  if (!bytes) return "";
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  return `${Math.round(bytes / (1024 * 1024))} MB`;
}

/** Voco brand mark — same single-stroke "Vo" as the sidebar LogoIcon, at tile size. */
const LogoMark = ({ size = 80 }: { size?: number }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 100 100"
    xmlns="http://www.w3.org/2000/svg"
    className="onb-logo"
    style={{ background: "#16191D" }}
    aria-label="Voco"
    role="img"
  >
    <path
      d="M 16.9 57.2 L 23.4 57.2 L 26.2 50.7 L 32.0 74.5 L 41.3 41.7 L 43.4 39.4 L 45.9 37.6 L 48.7 36.1 L 51.6 35.2 L 54.5 34.7 L 57.5 34.7 L 60.4 35.2 L 63.2 36.2 L 65.7 37.6 L 67.9 39.3 L 69.8 41.4 L 71.3 43.7 L 72.4 46.2 L 73.1 48.8 L 73.3 51.5 L 73.1 54.1 L 72.5 56.6 L 71.5 59.0 L 70.2 61.1 L 68.5 62.9 L 66.6 64.5 L 64.5 65.6 L 62.2 66.5 L 59.9 66.9 L 57.6 66.9 L 55.4 66.6 L 53.2 65.9 L 51.3 64.9 L 49.5 63.6 L 48.0 62.0 L 46.8 60.3 L 46.0 58.4 L 45.4 56.5 L 45.2 54.5 L 45.3 52.5 L 45.8 50.7 L 46.5 48.9 L 47.4 47.3 L 48.6 45.9 L 50.0 44.8 L 51.5 43.9 L 53.1 43.3 L 54.8 43.0 L 56.4 43.0 L 58.0 43.2 L 59.6 43.7 L 60.9 44.4 L 62.1 45.3 L 63.2 46.3 L 64.0 47.5 L 64.6 48.8 L 64.9 50.1 L 65.1 51.4 L 65.0 52.7 L 64.7 54.0 L 64.2 55.1 L 63.6 56.1 L 62.8 57.0 L 61.9 57.7 L 60.9 58.2"
      fill="none"
      stroke="#AEB9C2"
      strokeWidth="5"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

// ── Small pieces ───────────────────────────────────────────────────────────

const CheckBadge = ({ done, index }: { done: boolean; index?: number }) => (
  <div className={done ? "onb-check" : "onb-check onb-check--pending"} aria-hidden>
    {done ? "✓" : index ?? ""}
  </div>
);

interface PermRowProps {
  title: string;
  desc: string;
  granted: boolean;
  requesting?: boolean;
  onGrant: () => void;
}

const PermissionRow = ({ title, desc, granted, requesting, onGrant }: PermRowProps) => (
  <div className={granted ? "onb-row onb-row--granted" : "onb-row"}>
    <CheckBadge done={granted} />
    <div className="onb-row-text">
      <span className="onb-row-title">{title}</span>
      <span className="onb-row-desc">{desc}</span>
    </div>
    {!granted && (
      <Button variant="secondary" size="sm" label={requesting ? "Requesting…" : "Grant"} isDisabled={requesting} onClick={onGrant} />
    )}
  </div>
);

interface ModelCardProps {
  model: ModelInfo | undefined;
  fallbackName: string;
  fallbackSize: string;
  copy: string;
  onDownload: (id: string) => void;
  id: string;
}

const ModelDownloadCard = ({ model, fallbackName, fallbackSize, copy, onDownload, id }: ModelCardProps) => {
  const downloaded = !!model?.is_downloaded;
  const progress = model?.progress ?? 0;
  const downloading = !downloaded && progress > 0 && progress < 1;
  const size = model?.size_bytes ? formatSize(model.size_bytes) : fallbackSize;

  return (
    <div className={downloaded ? "onb-card onb-card--done" : "onb-card"}>
      <div className="onb-card-head">
        <CheckBadge done={downloaded} />
        <span className="onb-card-title">{model?.name || fallbackName}</span>
        <span className="onb-card-meta">{size}</span>
      </div>
      <p className="onb-card-desc">{copy}</p>
      {downloaded ? (
        <span className="onb-downloaded">✓ Downloaded and ready</span>
      ) : downloading ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <div className="onb-bar">
            <div className="onb-bar-fill" style={{ width: `${Math.round(progress * 100)}%` }} />
          </div>
          <span className="onb-card-meta">Downloading… {Math.round(progress * 100)}%</span>
        </div>
      ) : (
        <div>
          <Button variant="primary" size="sm" label={`Download (${size})`} isDisabled={!model} onClick={() => onDownload(id)} />
          {!model && <div className="onb-card-meta" style={{ marginTop: 8 }}>Model catalog unavailable — you can download it later in Settings.</div>}
        </div>
      )}
    </div>
  );
};

// ── The wizard ─────────────────────────────────────────────────────────────

export default function FirstRunOnboarding({ isOpen, onComplete, onOpenAiSettings }: FirstRunOnboardingProps) {
  const [stepIndex, setStepIndex] = useState(0);
  const step: StepKey = STEPS[stepIndex];

  // Permissions (mirrors GettingStartedPage's checks/requests).
  const [micGranted, setMicGranted] = useState(false);
  const [accessibilityGranted, setAccessibilityGranted] = useState(false);
  const [inputMonGranted, setInputMonGranted] = useState(false);
  const [screenGranted, setScreenGranted] = useState(false);
  const [micRequesting, setMicRequesting] = useState(false);

  // Models (shared by the dictation / meeting / AI-notes steps).
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [downloadError, setDownloadError] = useState<string | null>(null);

  // Hotkey label for the final step.
  const [hotkeyLabel, setHotkeyLabel] = useState("Left Option ⌥");

  const loadPermissions = useCallback(async () => {
    const call = async <T,>(cmd: string, def: T): Promise<T> => {
      try { return await invoke<T>(cmd); } catch { return def; }
    };
    setAccessibilityGranted(await call<boolean>("check_accessibility_permission", false));
    setInputMonGranted((await call<string>("check_input_monitoring_permission", "unknown")) === "granted");
    setMicGranted((await call<string>("check_microphone_permission", "unknown")) === "granted");
    setScreenGranted(await call<boolean>("check_screen_recording_permission", false));
  }, []);

  const fetchModels = useCallback(async () => {
    try {
      const list = await invoke<ModelInfo[]>("list_models");
      if (Array.isArray(list)) setModels(list);
    } catch { /* backend unavailable — cards fall back gracefully */ }
  }, []);

  // Initial load + permission polling while the wizard is open.
  useEffect(() => {
    if (!isOpen) return;
    void loadPermissions();
    void fetchModels();
    invoke<string | null>("get_setting", { key: "dictation_hotkey" })
      .then((hk) => setHotkeyLabel(HOTKEY_LABELS[hk || "LeftOption"] || hk || "Left Option ⌥"))
      .catch(() => { /* keep default */ });

    const onFocus = () => void loadPermissions();
    window.addEventListener("focus", onFocus);
    const timer = window.setInterval(() => void loadPermissions(), 3000);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.clearInterval(timer);
    };
  }, [isOpen, loadPermissions, fetchModels]);

  // Live download progress (same event shapes ModelSelector handles).
  useEffect(() => {
    if (!isOpen) return;
    let unlistenProgress: (() => void) | undefined;
    let unlistenComplete: (() => void) | undefined;

    listen<any>("model-download-progress", (event) => {
      const payload = event.payload;
      if (!payload) return;
      const id = payload.model_id ?? payload.id;
      if (id === undefined) return;
      let progress: number | undefined;
      if (typeof payload.percent === "number") progress = payload.percent / 100;
      else if (typeof payload.progress === "number") progress = payload.progress;
      else if (typeof payload.downloaded_bytes === "number" && payload.total_bytes) progress = payload.downloaded_bytes / payload.total_bytes;
      if (progress === undefined) return;
      const clamped = Math.max(0, Math.min(1, progress));
      setModels((prev) =>
        prev.map((m) => (m.id === id ? { ...m, progress: clamped, is_downloaded: clamped >= 1 ? true : m.is_downloaded } : m))
      );
    }).then((un) => { unlistenProgress = un; });

    listen<any>("model-download-complete", (event) => {
      const payload = event.payload;
      const id = typeof payload === "string" ? payload : payload?.id || payload?.model_id;
      if (id) {
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, progress: 1, is_downloaded: true } : m)));
      }
      void fetchModels(); // poll list_models for the official state
    }).then((un) => { unlistenComplete = un; });

    return () => {
      unlistenProgress?.();
      unlistenComplete?.();
    };
  }, [isOpen, fetchModels]);

  const handleDownload = useCallback(async (id: string) => {
    setDownloadError(null);
    // Optimistic starting state; real progress arrives via events.
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, progress: Math.max(m.progress, 0.01) } : m)));
    try {
      await invoke("download_model", { id });
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, progress: 1, is_downloaded: true } : m)));
      void fetchModels();
    } catch (err) {
      console.error("Model download failed:", err);
      setDownloadError(`Download failed: ${err}`);
      void fetchModels(); // revert optimistic state
    }
  }, [fetchModels]);

  const requestMic = useCallback(async () => {
    setMicRequesting(true);
    try {
      // Trigger the native macOS prompt directly.
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      stream.getTracks().forEach((t) => t.stop());
    } catch {
      // Denied or unavailable — send the user to System Settings.
      try { await invoke("request_microphone_permission"); } catch { /* ignore */ }
    } finally {
      setMicRequesting(false);
      setTimeout(() => void loadPermissions(), 800);
    }
  }, [loadPermissions]);

  const requestAndReload = useCallback((cmd: string) => {
    void invoke(cmd).catch(() => { /* ignore */ }).then(() => setTimeout(() => void loadPermissions(), 800));
  }, [loadPermissions]);

  if (!isOpen) return null;

  const goBack = () => setStepIndex((i) => Math.max(0, i - 1));
  const goNext = () => setStepIndex((i) => Math.min(STEPS.length - 1, i + 1));

  const sttModel = models.find((m) => m.id === DICTATION_MODEL_ID);
  const mossModel = models.find((m) => m.id === MEETING_MODEL_ID);
  const llmModels = models.filter((m) => m.category === "llm");
  const downloadedLlm = llmModels.filter((m) => m.is_downloaded);

  const renderStep = () => {
    switch (step) {
      case "welcome":
        return (
          <>
            <LogoMark size={80} />
            <h1 className="onb-h1 onb-serif">Welcome to Voco</h1>
            <p className="onb-lead">
              Private, on-device dictation and meeting notes — your voice never leaves your Mac.
            </p>
          </>
        );

      case "permissions":
        return (
          <>
            <h2 className="onb-h2 onb-serif">A few permissions</h2>
            <p className="onb-lead">
              Voco needs macOS permissions to hear you and type for you. You can grant these now
              or any time later from Getting Started.
            </p>
            <div className="onb-rows">
              <PermissionRow
                title="Microphone"
                desc="Records your voice for dictation and meetings."
                granted={micGranted}
                requesting={micRequesting}
                onGrant={() => void requestMic()}
              />
              <PermissionRow
                title="Accessibility"
                desc="Lets Voco type transcribed text at your cursor."
                granted={accessibilityGranted}
                onGrant={() => requestAndReload("request_accessibility_permission")}
              />
              <PermissionRow
                title="Input Monitoring"
                desc="Makes the bare-modifier dictation hotkey (e.g. Left Option) work."
                granted={inputMonGranted}
                onGrant={() => requestAndReload("request_input_monitoring_permission")}
              />
              <PermissionRow
                title="Screen Recording"
                desc="Only for meetings — captures the system audio of other participants."
                granted={screenGranted}
                onGrant={() => requestAndReload("request_screen_recording_permission")}
              />
            </div>
            <p className="onb-fineprint">
              This screen updates automatically after you grant a permission in System Settings.
            </p>
          </>
        );

      case "dictation":
        return (
          <>
            <h2 className="onb-h2 onb-serif">Dictation model</h2>
            <p className="onb-lead">
              Fast on-device speech-to-text for dictation and live captions. Downloaded once,
              then it works fully offline.
            </p>
            <ModelDownloadCard
              id={DICTATION_MODEL_ID}
              model={sttModel}
              fallbackName="Parakeet TDT v3"
              fallbackSize="~660 MB"
              copy="Fast on-device speech-to-text for dictation and live captions."
              onDownload={(id) => void handleDownload(id)}
            />
            {downloadError && <p className="onb-fineprint" style={{ color: "var(--color-recording, #ef4444)" }}>{downloadError}</p>}
          </>
        );

      case "meeting":
        return (
          <>
            <h2 className="onb-h2 onb-serif">Meeting intelligence</h2>
            <p className="onb-lead">
              One model that transcribes meetings and tells speakers apart, fully offline
              (English + Chinese).
            </p>
            <ModelDownloadCard
              id={MEETING_MODEL_ID}
              model={mossModel}
              fallbackName="MOSS Transcribe+Diarize 0.9B"
              fallbackSize="~987 MB"
              copy="Transcribes meetings AND tells speakers apart, fully offline (English + Chinese)."
              onDownload={(id) => void handleDownload(id)}
            />
            {downloadError && <p className="onb-fineprint" style={{ color: "var(--color-recording, #ef4444)" }}>{downloadError}</p>}
          </>
        );

      case "ai":
        return (
          <>
            <h2 className="onb-h2 onb-serif">AI notes</h2>
            <p className="onb-lead">
              Summaries and structured notes need a language model. Run one locally, or connect
              a cloud provider — you can also decide later.
            </p>
            <div className={downloadedLlm.length > 0 ? "onb-card onb-card--done" : "onb-card"}>
              <div className="onb-card-head">
                <CheckBadge done={downloadedLlm.length > 0} />
                <span className="onb-card-title">Local (embedded LLM)</span>
              </div>
              <p className="onb-card-desc">Everything stays on your Mac. Slower than the cloud, completely private.</p>
              {llmModels.length === 0 ? (
                <span className="onb-card-meta">No embedded LLM available in this build — use a cloud provider, or add one later in Settings.</span>
              ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                  {llmModels.slice(0, 3).map((m) => {
                    const downloading = !m.is_downloaded && m.progress > 0 && m.progress < 1;
                    return (
                      <div key={m.id} className="onb-card-row">
                        <span style={{ flex: 1, fontSize: 13, fontWeight: 600, color: "var(--color-text-primary)" }}>{m.name}</span>
                        {m.is_downloaded ? (
                          <span className="onb-downloaded">✓</span>
                        ) : downloading ? (
                          <div className="onb-bar" style={{ width: 110 }}>
                            <div className="onb-bar-fill" style={{ width: `${Math.round(m.progress * 100)}%` }} />
                          </div>
                        ) : (
                          <Button variant="secondary" size="sm" label={`Download (${formatSize(m.size_bytes)})`} onClick={() => void handleDownload(m.id)} />
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
            <div className="onb-card">
              <div className="onb-card-head">
                <CheckBadge done={false} />
                <span className="onb-card-title">Cloud provider</span>
              </div>
              <p className="onb-card-desc">
                Connect OpenAI, Groq, Ollama and more for faster, higher-quality summaries.
                Configure it in AI provider settings.
              </p>
              <div>
                <Button variant="secondary" size="sm" label="Open provider settings" onClick={onOpenAiSettings} />
              </div>
            </div>
            {downloadError && <p className="onb-fineprint" style={{ color: "var(--color-recording, #ef4444)" }}>{downloadError}</p>}
          </>
        );

      case "done":
        return (
          <>
            <LogoMark size={64} />
            <h1 className="onb-h1 onb-serif">You're all set</h1>
            <p className="onb-lead">
              Press <span className="onb-kbd">{hotkeyLabel}</span> to dictate anywhere on your Mac.
            </p>
            <p className="onb-fineprint">
              Anything you skipped lives in Getting Started — permissions, models and providers
              can be finished there whenever you like.
            </p>
          </>
        );
    }
  };

  return (
    <div
      className="onb-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Voco setup"
      // Box-critical styles inline so a stylesheet hiccup can never break the overlay.
      style={{ position: "fixed", inset: 0, zIndex: 5000 }}
    >
      <div className="onb-topbar">
        {step !== "done" && <button type="button" className="onb-skip" onClick={onComplete}>Skip setup</button>}
      </div>

      <div className="onb-body">
        <div className="onb-col">
          {/* key on step so the enter animation re-triggers per step */}
          <div key={step} className="onb-step">
            {renderStep()}
          </div>

          <div className="onb-footer">
            {stepIndex > 0 && step !== "done" ? (
              <Button variant="ghost" label="Back" onClick={goBack} />
            ) : (
              <span />
            )}
            {step === "done" ? (
              <Button variant="primary" label="Start using Voco" onClick={onComplete} />
            ) : (
              <Button variant="primary" label="Continue" onClick={goNext} />
            )}
          </div>

          <div className="onb-dots" aria-hidden>
            {STEPS.map((s, i) => (
              <span
                key={s}
                className={
                  i === stepIndex
                    ? "onb-dot onb-dot--active"
                    : i < stepIndex
                      ? "onb-dot onb-dot--done"
                      : "onb-dot"
                }
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
