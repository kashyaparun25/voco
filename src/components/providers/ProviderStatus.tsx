import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Badge } from "@astryxdesign/core/Badge";
import { Text } from "@astryxdesign/core/Text";
import { HStack, VStack } from "@astryxdesign/core/Layout";
import { Button } from "../ui";

interface ProviderStatusProps {
  providerId: string;
  providerType: string;
  apiUrl?: string;
  /** When provided (form context), the test uses the AS-TYPED values below
   *  instead of the last-saved provider. */
  apiKey?: string;
  onStatusChange?: (status: "healthy" | "error" | "testing" | "unknown") => void;
}

export default function ProviderStatus({
  providerId,
  providerType,
  apiUrl,
  apiKey,
  onStatusChange
}: ProviderStatusProps) {
  const [status, setStatus] = useState<"healthy" | "error" | "testing" | "unknown">("unknown");
  const [latency, setLatency] = useState<number | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const checkConnection = async () => {
    if (providerType === "embedded") {
      setStatus("healthy");
      setLatency(1); // Local disk latency is sub-1ms
      setErrorMessage(null);
      onStatusChange?.("healthy");
      return;
    }

    setStatus("testing");
    setErrorMessage(null);
    onStatusChange?.("testing");

    const startTime = performance.now();
    try {
      // Test the AS-TYPED config only in the form context (apiKey prop
      // present — possibly empty while typing). List cards pass no apiKey and
      // must use the saved provider, whose stored (decrypted) key is applied —
      // otherwise keyed providers 401 and show a false "Offline".
      const success = apiKey !== undefined && apiUrl
        ? await invoke<boolean>("test_provider_config", {
            config: {
              id: providerId,
              name: providerId,
              provider_type: providerType,
              api_url: apiUrl,
              api_key: apiKey && apiKey.trim() ? apiKey : undefined,
            },
          })
        : await invoke<boolean>("test_provider_connection", { id: providerId });
      const endTime = performance.now();
      const calculatedLatency = Math.round(endTime - startTime);

      if (success) {
        setStatus("healthy");
        setLatency(calculatedLatency);
        onStatusChange?.("healthy");
      } else {
        setStatus("error");
        setLatency(null);
        setErrorMessage("Connection returned invalid response");
        onStatusChange?.("error");
      }
    } catch (err: any) {
      console.error(`Connection test failed for provider ${providerId}:`, err);
      setStatus("error");
      setLatency(null);
      setErrorMessage(typeof err === "string" ? err : err.message || "Failed to connect");
      onStatusChange?.("error");
    }
  };

  useEffect(() => {
    // Debounce: don't fire a network test per keystroke while typing.
    const t = setTimeout(() => void checkConnection(), 600);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [providerId, providerType, apiUrl, apiKey]);

  const getStatusBadge = () => {
    switch (status) {
      case "healthy":
        return <Badge variant="green" label="Healthy" />;
      case "testing":
        return <Badge variant="orange" label="Testing..." />;
      case "error":
        return <Badge variant="red" label="Offline" />;
      default:
        return <Badge variant="neutral" label="Unknown" />;
    }
  };

  return (
    <VStack gap={2} style={{ width: "100%", padding: "8px 0" }}>
      <HStack style={{ justifyContent: "space-between", alignItems: "center", width: "100%" }}>
        <HStack gap={2} style={{ alignItems: "center" }}>
          <Text style={{ fontSize: "13px", fontWeight: "600", color: "var(--color-text-secondary)" }}>
            Connection:
          </Text>
          {getStatusBadge()}
          {latency !== null && (
            <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
              ({latency} ms)
            </Text>
          )}
        </HStack>
        
        {providerType !== "embedded" && (
          <Button
            variant="secondary"
            label="Retest"
            onClick={checkConnection}
            style={{
              padding: "4px 8px",
              fontSize: "11px",
              minHeight: "unset",
              cursor: "pointer"
            }}
          />
        )}
      </HStack>
      {errorMessage && (
        <Text style={{ fontSize: "11px", color: "var(--color-recording, #ef4444)", marginTop: 2 }}>
          Error: {errorMessage}
        </Text>
      )}
    </VStack>
  );
}
