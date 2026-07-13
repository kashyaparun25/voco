import { useState, type ReactNode } from "react";
import { Card } from "@astryxdesign/core/Card";
import { Button } from "../ui";
import { Text } from "@astryxdesign/core/Text";
import { VStack, HStack } from "@astryxdesign/core/Layout";

interface FirstRunOnboardingProps {
  isOpen: boolean;
  onClose: () => void;
  /** Take the user to the models/settings screen to download a model. */
  onGoToModels: () => void;
}

type Step = "welcome" | "mic" | "models";
type MicStatus = "idle" | "granted" | "denied";

const MicIcon = ({ color = "currentColor" }: { color?: string }) => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke={color} style={{ width: 28, height: 28 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M12 18.75a6 6 0 0 0 6-6v-1.5m-6 7.5a6 6 0 0 1-6-6v-1.5m6 7.5v3.75m-3.75 0h7.5M12 15.75a3 3 0 0 1-3-3V4.5a3 3 0 1 1 6 0v8.25a3 3 0 0 1-3 3Z" />
  </svg>
);

const DownloadIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 28, height: 28 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75V16.5M16.5 12 12 16.5m0 0L7.5 12m4.5 4.5V3" />
  </svg>
);

/**
 * FirstRunOnboarding — shown once on first launch (guarded by the caller via
 * localStorage). Welcomes the user, requests microphone access (triggering the
 * macOS prompt), and points to model download.
 */
export default function FirstRunOnboarding({ isOpen, onClose, onGoToModels }: FirstRunOnboardingProps) {
  const [step, setStep] = useState<Step>("welcome");
  const [micStatus, setMicStatus] = useState<MicStatus>("idle");
  const [requesting, setRequesting] = useState(false);

  if (!isOpen) return null;

  const requestMic = async () => {
    setRequesting(true);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      // Stop tracks immediately — we only wanted to trigger/confirm permission.
      stream.getTracks().forEach((t) => t.stop());
      setMicStatus("granted");
    } catch (err) {
      console.warn("Microphone permission denied or unavailable:", err);
      setMicStatus("denied");
    } finally {
      setRequesting(false);
    }
  };

  const primaryButton = (label: string, onClick: () => void, disabled = false) => (
    <Button
      variant="primary"
      onClick={onClick}
      isDisabled={disabled}
      label={label}
      style={{
        padding: "10px 20px",
        borderRadius: "8px",
        backgroundColor: "var(--color-accent)",
        color: "#ffffff",
        border: "none",
        cursor: disabled ? "not-allowed" : "pointer",
        fontWeight: 600,
        opacity: disabled ? 0.6 : 1,
      }}
    />
  );

  const iconCircle = (icon: ReactNode) => (
    <div style={{
      width: 56, height: 56, borderRadius: "50%",
      backgroundColor: "var(--color-background-surface-hover)",
      display: "flex", alignItems: "center", justifyContent: "center",
      color: "var(--color-accent)",
    }}>
      {icon}
    </div>
  );

  return (
    <div
      role="dialog"
      aria-modal="true"
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1000,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        backgroundColor: "rgba(0,0,0,0.55)",
        backdropFilter: "blur(2px)",
      }}
    >
      <Card
        className="voco-tab-enter"
        style={{
          width: "440px",
          maxWidth: "90vw",
          padding: "28px",
          backgroundColor: "var(--color-background-elevated)",
          border: "1px solid var(--color-border-strong)",
          borderRadius: "16px",
          display: "flex",
          flexDirection: "column",
          gap: "20px",
          boxShadow: "0 12px 40px rgba(0,0,0,0.45)",
        }}
      >
        {step === "welcome" && (
          <>
            <VStack gap={3} style={{ alignItems: "center", textAlign: "center" }}>
              {iconCircle(<MicIcon color="var(--color-accent)" />)}
              <Text style={{ fontSize: "22px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
                Welcome to Voco
              </Text>
              <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)", lineHeight: 1.5 }}>
                Private, on-device dictation and meeting transcription powered by local AI.
                Let's get you set up in two quick steps.
              </Text>
            </VStack>
            <HStack style={{ justifyContent: "flex-end" }} gap={2}>
              {primaryButton("Get Started", () => setStep("mic"))}
            </HStack>
          </>
        )}

        {step === "mic" && (
          <>
            <VStack gap={3} style={{ alignItems: "center", textAlign: "center" }}>
              {iconCircle(<MicIcon color="var(--color-accent)" />)}
              <Text style={{ fontSize: "20px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
                Microphone Access
              </Text>
              <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)", lineHeight: 1.5 }}>
                Voco needs your microphone to transcribe speech. macOS will ask for
                permission — click Allow.
              </Text>
              {micStatus === "granted" && (
                <Text style={{ fontSize: "13px", fontWeight: 600, color: "var(--color-accent-text, var(--color-accent))" }}>
                  Microphone access granted.
                </Text>
              )}
              {micStatus === "denied" && (
                <Text style={{ fontSize: "13px", color: "var(--color-recording)" }}>
                  Access was denied. You can enable it later in System Settings → Privacy.
                </Text>
              )}
            </VStack>
            <HStack style={{ justifyContent: "space-between" }} gap={2}>
              <Button
                variant="secondary"
                onClick={() => setStep("models")}
                label="Skip"
                style={{
                  padding: "10px 16px", borderRadius: "8px",
                  border: "1px solid var(--color-border-strong)",
                  backgroundColor: "transparent",
                  color: "var(--color-text-secondary)", cursor: "pointer",
                }}
              />
              {micStatus === "granted"
                ? primaryButton("Continue", () => setStep("models"))
                : primaryButton(requesting ? "Requesting..." : "Enable Microphone", requestMic, requesting)}
            </HStack>
          </>
        )}

        {step === "models" && (
          <>
            <VStack gap={3} style={{ alignItems: "center", textAlign: "center" }}>
              {iconCircle(<DownloadIcon />)}
              <Text style={{ fontSize: "20px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
                Download a Model
              </Text>
              <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)", lineHeight: 1.5 }}>
                Voco runs AI locally. Head to Settings to download a speech-to-text model
                recommended for your Mac — then you're ready to go.
              </Text>
            </VStack>
            <HStack style={{ justifyContent: "space-between" }} gap={2}>
              <Button
                variant="secondary"
                onClick={onClose}
                label="Do it later"
                style={{
                  padding: "10px 16px", borderRadius: "8px",
                  border: "1px solid var(--color-border-strong)",
                  backgroundColor: "transparent",
                  color: "var(--color-text-secondary)", cursor: "pointer",
                }}
              />
              {primaryButton("Go to Models", onGoToModels)}
            </HStack>
          </>
        )}
      </Card>
    </div>
  );
}
