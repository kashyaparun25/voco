import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card } from "@astryxdesign/core/Card";
import { Button } from "../ui";
import { Badge } from "@astryxdesign/core/Badge";
import { Text } from "@astryxdesign/core/Text";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import ProviderStatus from "./ProviderStatus";
import ProviderForm, { ProviderConfig } from "./ProviderForm";

export default function ProviderList() {
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  
  // Form active state
  const [isFormOpen, setIsFormOpen] = useState<boolean>(false);
  const [editingProvider, setEditingProvider] = useState<ProviderConfig | null>(null);

  const fetchProviders = async () => {
    try {
      setLoading(true);
      setError(null);
      const list = await invoke<ProviderConfig[]>("get_providers");
      
      // Ensure Embedded provider is always present at the top
      const hasEmbedded = list.some(p => p.id === "embedded" || p.provider_type === "embedded");
      if (!hasEmbedded) {
        const embeddedProvider: ProviderConfig = {
          id: "embedded",
          name: "Embedded Models",
          provider_type: "embedded"
        };
        setProviders([embeddedProvider, ...list]);
      } else {
        // Move embedded to the top
        const embedded = list.filter(p => p.provider_type === "embedded");
        const rest = list.filter(p => p.provider_type !== "embedded");
        setProviders([...embedded, ...rest]);
      }
    } catch (err: any) {
      console.error("Failed to load providers:", err);
      setError("Failed to load connection providers from backend.");
      // Fallback: at least show Embedded
      setProviders([{
        id: "embedded",
        name: "Embedded Models (Local)",
        provider_type: "embedded"
      }]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchProviders();
  }, []);

  const handleDelete = async (id: string) => {
    if (id === "embedded") return;
    if (!confirm("Are you sure you want to remove this connection provider?")) return;
    
    try {
      await invoke("delete_provider", { id });
      fetchProviders();
    } catch (err: any) {
      console.error("Failed to delete provider:", err);
      setError("Failed to delete connection provider.");
    }
  };

  const getProviderTypeLabel = (type: string) => {
    switch (type) {
      case "embedded": return "Embedded (Disk)";
      case "ollama": return "Ollama";
      case "lm_studio": return "LM Studio";
      case "openai": return "OpenAI Cloud";
      case "nvidia_nim": return "NVIDIA NIM";
      case "groq": return "Groq Cloud";
      case "custom": return "Custom Server";
      default: return type.toUpperCase();
    }
  };

  const getProviderTypeColor = (type: string): "neutral" | "blue" | "purple" | "teal" | "orange" | "green" | "red" => {
    switch (type) {
      case "embedded": return "green";
      case "ollama": return "blue";
      case "lm_studio": return "purple";
      case "openai": return "teal";
      case "nvidia_nim": return "orange";
      case "groq": return "neutral";
      default: return "neutral";
    }
  };

  if (isFormOpen) {
    return (
      <VStack style={{ alignItems: "center", width: "100%", padding: "12px 0" }}>
        <ProviderForm
          provider={editingProvider}
          onSave={() => {
            setIsFormOpen(false);
            setEditingProvider(null);
            fetchProviders();
          }}
          onCancel={() => {
            setIsFormOpen(false);
            setEditingProvider(null);
          }}
        />
      </VStack>
    );
  }

  return (
    <VStack gap={4} style={{ width: "100%" }}>
      <HStack style={{ justifyContent: "space-between", alignItems: "center", width: "100%" }}>
        <VStack gap={1}>
          <Text style={{ fontSize: "20px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
            Connection Providers
          </Text>
          <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
            Configure and monitor external APIs and local LLM/STT backends.
          </Text>
        </VStack>
        <Button
          variant="primary"
          label="Add Provider"
          onClick={() => {
            setEditingProvider(null);
            setIsFormOpen(true);
          }}
          style={{ cursor: "pointer" }}
        />
      </HStack>

      {error && (
        <div style={{
          padding: "12px 16px",
          backgroundColor: "rgba(239, 68, 68, 0.1)",
          border: "1px solid var(--color-recording, #ef4444)",
          borderRadius: "8px",
          color: "var(--color-recording, #ef4444)",
          fontSize: "14px"
        }}>
          {error}
        </div>
      )}

      {loading ? (
        <VStack style={{ alignItems: "center", padding: 40, width: "100%" }}>
          <Text style={{ color: "var(--color-text-secondary)" }}>Loading providers...</Text>
        </VStack>
      ) : (
        <div style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))",
          gap: "16px",
          width: "100%"
        }}>
          {providers.map((p) => (
            <Card
              key={p.id}
              style={{
                padding: 18,
                backgroundColor: "var(--color-background-surface)",
                border: "1px solid var(--color-border)",
                borderRadius: "12px",
                display: "flex",
                flexDirection: "column",
                gap: 16,
                boxShadow: "0 2px 8px rgba(0, 0, 0, 0.1)"
              }}
            >
              <VStack gap={2}>
                <HStack style={{ justifyContent: "space-between", alignItems: "flex-start", width: "100%" }}>
                  <VStack gap={1} style={{ flex: 1 }}>
                    <Text style={{ fontSize: "16px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
                      {p.name}
                    </Text>
                    <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)", wordBreak: "break-all" }}>
                      {p.provider_type === "embedded" 
                        ? "Local GGUF models on disk" 
                        : (p.api_url || "No URL configured")}
                    </Text>
                  </VStack>
                  <Badge 
                    variant={getProviderTypeColor(p.provider_type)} 
                    label={getProviderTypeLabel(p.provider_type)} 
                  />
                </HStack>

                {/* Connection Status Indicator */}
                <ProviderStatus 
                  providerId={p.id} 
                  providerType={p.provider_type} 
                  apiUrl={p.api_url} 
                />
              </VStack>

              <HStack style={{ justifyContent: "flex-end", width: "100%", borderTop: "1px solid var(--color-border)", paddingTop: 12 }}>
                {p.provider_type !== "embedded" && (
                  <HStack gap={2}>
                    <Button
                      variant="secondary"
                      label="Remove"
                      style={{ 
                        color: "var(--color-recording, #ef4444)", 
                        borderColor: "var(--color-border)",
                        cursor: "pointer"
                      }}
                      onClick={() => handleDelete(p.id)}
                    />
                    <Button
                      variant="secondary"
                      label="Edit"
                      style={{ cursor: "pointer" }}
                      onClick={() => {
                        setEditingProvider(p);
                        setIsFormOpen(true);
                      }}
                    />
                  </HStack>
                )}
                {p.provider_type === "embedded" && (
                  <Text style={{ fontSize: "11px", color: "var(--color-text-secondary)" }}>
                    System default (read-only)
                  </Text>
                )}
              </HStack>
            </Card>
          ))}
        </div>
      )}
    </VStack>
  );
}
