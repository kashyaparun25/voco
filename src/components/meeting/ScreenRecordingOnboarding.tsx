import { Card } from "@astryxdesign/core/Card";
import { Text } from "@astryxdesign/core/Text";
import { Button } from "../ui";
import { VStack, HStack } from "@astryxdesign/core/Layout";

interface ScreenRecordingOnboardingProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirm: () => void;
}

export default function ScreenRecordingOnboarding({
  isOpen,
  onClose,
  onConfirm
}: ScreenRecordingOnboardingProps) {
  if (!isOpen) return null;

  const handleOpenSettings = async () => {
    try {
      // Use Tauri opener plugin or standard URL open to launch macOS privacy settings
      // "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
      // is the direct URL protocol to open Screen Recording settings.
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture");
    } catch (e) {
      console.error("Failed to open System Settings via Tauri opener", e);
    }
  };

  return (
    <div style={{
      position: "fixed",
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      backgroundColor: "rgba(0, 0, 0, 0.6)",
      backdropFilter: "blur(4px)",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      zIndex: 9999,
      animation: "fadeIn 0.2s ease"
    }}>
      <Card style={{
        width: "480px",
        padding: "24px",
        backgroundColor: "var(--color-background-surface)",
        border: "1px solid var(--color-border-strong)",
        borderRadius: "16px",
        boxShadow: "0 20px 25px -5px rgba(0, 0, 0, 0.3), 0 10px 10px -5px rgba(0, 0, 0, 0.3)",
        display: "flex",
        flexDirection: "column",
        gap: "20px"
      }}>
        <VStack gap={2} style={{ alignItems: "center", textAlign: "center" }}>
          <div style={{
            width: "56px",
            height: "56px",
            borderRadius: "14px",
            backgroundColor: "rgba(124, 58, 237, 0.1)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "var(--color-accent)",
            marginBottom: "8px"
          }}>
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 28, height: 28 }}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9 17.25v1.007a3 3 0 0 1-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0 1 15 18.257V17.25m6-12V15a2.25 2.25 0 0 1-2.25 2.25H5.25A2.25 2.25 0 0 1 3 15V5.25m18 0A2.25 2.25 0 0 0 18.75 3H5.25A2.25 2.25 0 0 0 3 5.25m18 0V12a2.25 2.25 0 0 1-2.25 2.25H5.25A2.25 2.25 0 0 1 3 12V5.25" />
            </svg>
          </div>
          <Text style={{ fontSize: "20px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
            Screen Recording Permission Required
          </Text>
          <Text style={{ fontSize: "13px", color: "var(--color-text-secondary)" }}>
            To record system audio (like video calls or browser audio), macOS requires Screen Recording permission. Voco does not record or save your screen.
          </Text>
        </VStack>

        <div style={{
          backgroundColor: "rgba(255, 255, 255, 0.02)",
          border: "1px solid var(--color-border)",
          borderRadius: "10px",
          padding: "16px",
          display: "flex",
          flexDirection: "column",
          gap: "12px"
        }}>
          <HStack gap={3} style={{ alignItems: "flex-start" }}>
            <div style={{
              width: "20px",
              height: "20px",
              borderRadius: "50%",
              backgroundColor: "var(--color-accent)",
              color: "#ffffff",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: "11px",
              fontWeight: "bold",
              flexShrink: 0,
              marginTop: "2px"
            }}>1</div>
            <VStack gap={1}>
              <Text style={{ fontSize: "13px", fontWeight: "600", color: "var(--color-text-primary)" }}>
                Open Screen Recording Settings
              </Text>
              <Text style={{ fontSize: "11px", color: "var(--color-text-secondary)" }}>
                Click the button below to open the macOS Security & Privacy system preferences.
              </Text>
            </VStack>
          </HStack>

          <HStack gap={3} style={{ alignItems: "flex-start" }}>
            <div style={{
              width: "20px",
              height: "20px",
              borderRadius: "50%",
              backgroundColor: "var(--color-accent)",
              color: "#ffffff",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: "11px",
              fontWeight: "bold",
              flexShrink: 0,
              marginTop: "2px"
            }}>2</div>
            <VStack gap={1}>
              <Text style={{ fontSize: "13px", fontWeight: "600", color: "var(--color-text-primary)" }}>
                Enable Voco
              </Text>
              <Text style={{ fontSize: "11px", color: "var(--color-text-secondary)" }}>
                Find "Voco" in the list and toggle the switch to enable it.
              </Text>
            </VStack>
          </HStack>

          <HStack gap={3} style={{ alignItems: "flex-start" }}>
            <div style={{
              width: "20px",
              height: "20px",
              borderRadius: "50%",
              backgroundColor: "var(--color-accent)",
              color: "#ffffff",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: "11px",
              fontWeight: "bold",
              flexShrink: 0,
              marginTop: "2px"
            }}>3</div>
            <VStack gap={1}>
              <Text style={{ fontSize: "13px", fontWeight: "600", color: "var(--color-text-primary)" }}>
                Relaunch Voco (If Prompted)
              </Text>
              <Text style={{ fontSize: "11px", color: "var(--color-text-secondary)" }}>
                macOS may prompt you to close and relaunch the application to apply the settings.
              </Text>
            </VStack>
          </HStack>
        </div>

        <HStack gap={3} style={{ justifyContent: "flex-end", width: "100%" }}>
          <Button
            variant="secondary"
            onClick={onClose}
            label="Cancel"
            style={{
              cursor: "pointer",
              padding: "8px 16px",
              borderRadius: "8px",
              border: "1px solid var(--color-border-strong)"
            }}
          />
          <Button
            variant="secondary"
            onClick={handleOpenSettings}
            label="Open System Settings"
            style={{
              cursor: "pointer",
              padding: "8px 16px",
              borderRadius: "8px",
              border: "1px solid var(--color-accent)",
              color: "var(--color-accent-text)",
              backgroundColor: "rgba(124, 58, 237, 0.05)"
            }}
          />
          <Button
            variant="primary"
            onClick={onConfirm}
            label="I've Enabled It"
            style={{
              cursor: "pointer",
              padding: "8px 16px",
              borderRadius: "8px",
              backgroundColor: "var(--color-accent)",
              color: "#ffffff",
              border: "none"
            }}
          />
        </HStack>
      </Card>
    </div>
  );
}
