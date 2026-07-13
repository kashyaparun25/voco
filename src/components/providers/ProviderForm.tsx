import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card } from "@astryxdesign/core/Card";
import { Button } from "../ui";
import { TextInput } from "../ui";
import { Text } from "@astryxdesign/core/Text";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import ProviderStatus from "./ProviderStatus";

export interface ProviderConfig {
  id: string;
  name: string;
  api_key?: string;
  api_url?: string;
  provider_type: string; // e.g. "embedded", "ollama", "lm_studio", "openai", "nvidia_nim", "groq", "custom"
  default_model?: string; // model id used for STT/LLM calls (e.g. "whisper-1", "llama3.2")
}

interface ProviderFormProps {
  provider?: ProviderConfig | null; // Null if adding a new provider
  onSave: () => void;
  onCancel: () => void;
}

const PROVIDER_DEFAULTS: Record<string, { name: string; url: string; requiresKey: boolean; modelHint: string }> = {
  ollama: { name: "Ollama", url: "http://localhost:11434", requiresKey: false, modelHint: "llama3.2" },
  lm_studio: { name: "LM Studio", url: "http://localhost:1234/v1", requiresKey: false, modelHint: "loaded-model-name" },
  openai: { name: "OpenAI", url: "https://api.openai.com/v1", requiresKey: true, modelHint: "whisper-1 or gpt-4o-mini" },
  nvidia_nim: { name: "NVIDIA NIM", url: "https://integrate.api.nvidia.com/v1", requiresKey: true, modelHint: "parakeet-ctc-1.1b" },
  groq: { name: "Groq", url: "https://api.groq.com/openai/v1", requiresKey: true, modelHint: "whisper-large-v3-turbo" },
  custom: { name: "Custom Provider", url: "http://localhost:8000", requiresKey: false, modelHint: "model-name" }
};

export default function ProviderForm({ provider, onSave, onCancel }: ProviderFormProps) {
  const isEditing = !!provider;
  const [providerType, setProviderType] = useState<string>("ollama");
  const [name, setName] = useState<string>("");
  const [apiUrl, setApiUrl] = useState<string>("");
  const [apiKey, setApiKey] = useState<string>("");
  const [model, setModel] = useState<string>("");
  const [showPassword, setShowPassword] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState<boolean>(false);


  useEffect(() => {
    if (provider) {
      setProviderType(provider.provider_type);
      setName(provider.name);
      setApiUrl(provider.api_url || "");
      setApiKey(provider.api_key || "");
      setModel(provider.default_model || "");
    } else {
      // Set default pre-fills for a new provider
      const type = "ollama";
      setProviderType(type);
      setName(PROVIDER_DEFAULTS[type].name);
      setApiUrl(PROVIDER_DEFAULTS[type].url);
      setApiKey("");
      setModel("");
    }
  }, [provider]);

  const handleTypeChange = (type: string) => {
    if (isEditing) return; // Cannot change type when editing
    setProviderType(type);
    setName(PROVIDER_DEFAULTS[type].name);
    setApiUrl(PROVIDER_DEFAULTS[type].url);
    setApiKey("");
    setModel("");
  };

  const handleSave = async () => {
    if (!name.trim()) {
      setError("Provider name is required.");
      return;
    }

    setSaving(true);
    setError(null);

    const config: ProviderConfig = {
      id: provider?.id || `${providerType}-${Date.now()}`,
      name: name.trim(),
      provider_type: providerType,
      api_url: apiUrl.trim() ? apiUrl.trim() : undefined,
      api_key: apiKey.trim() ? apiKey.trim() : undefined,
      default_model: model.trim() || undefined,
    };

    try {
      // add_provider persists to the DB-backed registry (upserts by id), so
      // edits and adds both survive restarts.
      await invoke("add_provider", { config });
      onSave();
    } catch (err: any) {
      console.error("Failed to save provider:", err);
      setError(typeof err === "string" ? err : err.message || "Failed to save provider config.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <Card style={{
      padding: 24,
      backgroundColor: "var(--color-background-surface)",
      border: "1px solid var(--color-border-strong)",
      borderRadius: "16px",
      width: "100%",
      maxWidth: "520px",
      boxShadow: "0 4px 16px rgba(0, 0, 0, 0.2)"
    }}>
      <VStack gap={4}>
        <VStack gap={1}>
          <Text style={{ fontSize: "20px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
            {isEditing ? `Edit ${name}` : "Add Connection Provider"}
          </Text>
          <Text style={{ fontSize: "13px", color: "var(--color-text-secondary)" }}>
            Connect local servers or cloud APIs for Speech-to-Text and Summarization models.
          </Text>
        </VStack>

        {error && (
          <div style={{
            padding: "12px",
            backgroundColor: "rgba(239, 68, 68, 0.1)",
            border: "1px solid var(--color-recording, #ef4444)",
            borderRadius: "8px",
            color: "var(--color-recording, #ef4444)",
            fontSize: "13px"
          }}>
            {error}
          </div>
        )}

        {/* Provider Type Selection */}
        {!isEditing && (
          <VStack gap={2}>
            <Text style={{ fontSize: "13px", fontWeight: "bold", color: "var(--color-text-secondary)" }}>
              Provider Type
            </Text>
            <div style={{
              display: "grid",
              gridTemplateColumns: "repeat(3, 1fr)",
              gap: "8px"
            }}>
              {Object.keys(PROVIDER_DEFAULTS).map((type) => {
                const isSelected = providerType === type;
                return (
                  <button
                    key={type}
                    type="button"
                    onClick={() => handleTypeChange(type)}
                    style={{
                      padding: "10px 8px",
                      borderRadius: "8px",
                      backgroundColor: isSelected ? "var(--color-accent)" : "var(--color-background-elevated)",
                      color: isSelected ? "#ffffff" : "var(--color-text-primary)",
                      border: `1px solid ${isSelected ? "var(--color-accent)" : "var(--color-border)"}`,
                      fontSize: "12px",
                      fontWeight: "bold",
                      cursor: "pointer",
                      textAlign: "center",
                      transition: "all 0.15s ease"
                    }}
                  >
                    {PROVIDER_DEFAULTS[type].name}
                  </button>
                );
              })}
            </div>
          </VStack>
        )}

        <VStack gap={3}>
          <TextInput
            label="Display Name"
            placeholder="e.g. Local Ollama Server"
            value={name}
            onChange={(val) => setName(val)}
            style={{ width: "100%" }}
          />

          <TextInput
            label="Base URL / API Endpoint"
            placeholder="http://localhost:11434"
            value={apiUrl}
            onChange={(val) => setApiUrl(val)}
            style={{ width: "100%" }}
          />

          <VStack gap={1} style={{ width: "100%" }}>
            <TextInput
              label="Model"
              placeholder={PROVIDER_DEFAULTS[providerType]?.modelHint || "model-name"}
              value={model}
              onChange={(val) => setModel(val)}
              style={{ width: "100%" }}
            />
            <Text style={{ fontSize: "11px", color: "var(--color-text-secondary)" }}>
              The model this provider will use (e.g. {PROVIDER_DEFAULTS[providerType]?.modelHint}).
              Saved with the provider and persisted across restarts.
            </Text>
          </VStack>

          <VStack gap={1} style={{ position: "relative", width: "100%" }}>
            <TextInput
              label={PROVIDER_DEFAULTS[providerType]?.requiresKey ? "API Key (Required)" : "API Key (Optional)"}
              placeholder="sk-..."
              value={apiKey}
              type={showPassword ? "text" : "password"}
              onChange={(val) => setApiKey(val)}
              style={{ width: "100%" }}
            />
            <button
              type="button"
              onClick={() => setShowPassword(!showPassword)}
              style={{
                position: "absolute",
                right: "12px",
                bottom: "10px",
                background: "none",
                border: "none",
                color: "var(--color-text-secondary)",
                cursor: "pointer",
                fontSize: "12px"
              }}
            >
              {showPassword ? "Hide" : "Show"}
            </button>
          </VStack>
        </VStack>

        {/* Live Test Status card */}
        {apiUrl && (
          <VStack gap={2} style={{
            padding: "12px 16px",
            borderRadius: "10px",
            backgroundColor: "var(--color-background-elevated)",
            border: "1px solid var(--color-border)"
          }}>
            <ProviderStatus
              providerId={provider?.id || "temp-test"}
              providerType={providerType}
              apiUrl={apiUrl}
              apiKey={apiKey}
            />
          </VStack>
        )}

        {/* Actions */}
        <HStack gap={3} style={{ justifyContent: "flex-end", width: "100%", marginTop: "8px" }}>
          <Button
            variant="secondary"
            label="Cancel"
            onClick={onCancel}
            style={{ cursor: "pointer" }}
          />
          <Button
            variant="primary"
            label={saving ? "Saving..." : "Save Connection"}
            onClick={handleSave}
            isDisabled={saving || (PROVIDER_DEFAULTS[providerType]?.requiresKey && !apiKey.trim())}
            style={{ cursor: "pointer" }}
          />
        </HStack>
      </VStack>
    </Card>
  );
}
