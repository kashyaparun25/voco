import { Card } from "@astryxdesign/core/Card";
import { Text } from "@astryxdesign/core/Text";
import { Button } from "../ui";
import { Badge } from "@astryxdesign/core/Badge";
import { ProgressBar } from "@astryxdesign/core/ProgressBar";
import { VStack, HStack } from "@astryxdesign/core/Layout";

export interface ModelInfo {
  id: string;
  name: string;
  size_bytes: number;
  is_downloaded: boolean;
  progress: number; // 0.0 to 1.0
  category: string; // "stt" or "llm"
  provider_id?: string;
  provider_name?: string;
  provider_type?: string;
  is_external?: boolean;
}

interface ModelCardProps {
  model: ModelInfo;
  onDownload: (id: string) => void;
  onDelete: (id: string) => void;
}

export default function ModelCard({ model, onDownload, onDelete }: ModelCardProps) {
  const formatSize = (bytes: number) => {
    if (bytes >= 1024 * 1024 * 1024) {
      return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  };

  const getModelTier = (id: string) => {
    const lower = id.toLowerCase();
    if (lower.includes("tiny")) return "Tiny Tier";
    if (lower.includes("base")) return "Base Tier";
    if (lower.includes("small")) return "Small Tier";
    if (lower.includes("medium")) return "Medium Tier";
    if (lower.includes("large")) return "Large Tier";
    if (lower.includes("1.5b")) return "1.5B Parameters";
    if (lower.includes("7b")) return "7B Parameters";
    return "Standard Tier";
  };

  const getEstimatedRam = (id: string, category: string) => {
    const lower = id.toLowerCase();
    if (category === "stt") {
      if (lower.includes("tiny")) return "~150 MB";
      if (lower.includes("base")) return "~300 MB";
      if (lower.includes("small")) return "~600 MB";
      if (lower.includes("medium")) return "~1.5 GB";
      if (lower.includes("large")) return "~3.0 GB";
      return "~500 MB";
    } else {
      if (lower.includes("1.5b")) return "~2.0 GB";
      if (lower.includes("3b")) return "~4.0 GB";
      if (lower.includes("7b")) return "~8.0 GB";
      return "~4.0 GB";
    }
  };

  const isDownloading = model.progress > 0 && model.progress < 1.0 && !model.is_downloaded;

  return (
    <Card style={{
      padding: 16,
      backgroundColor: "var(--color-background-surface)",
      border: "1px solid var(--color-border)",
      borderRadius: "12px",
      display: "flex",
      flexDirection: "column",
      gap: 16,
      boxShadow: "0 2px 8px rgba(0, 0, 0, 0.1)"
    }}>
      <VStack gap={2}>
        <HStack style={{ justifyContent: "space-between", alignItems: "flex-start", width: "100%" }}>
          <VStack gap={1} style={{ flex: 1 }}>
            <Text style={{ fontSize: "16px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
              {model.name}
            </Text>
            <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
              ID: {model.id}
            </Text>
          </VStack>
          <Badge 
            variant={model.category === "stt" ? "blue" : "purple"} 
            label={model.category === "stt" ? "Speech-to-Text" : "LLM / Chat"} 
          />
        </HStack>

        <HStack gap={2} style={{ flexWrap: "wrap", marginTop: 4 }}>
          <Badge variant="neutral" label={getModelTier(model.id)} />
          {model.size_bytes > 0 && <Badge variant="teal" label={`Size: ${formatSize(model.size_bytes)}`} />}
          {!model.is_external && <Badge variant="orange" label={`RAM Required: ${getEstimatedRam(model.id, model.category)}`} />}
          {model.is_external && <Badge variant="purple" label="API Model" />}
          {model.is_downloaded && !model.is_external && <Badge variant="green" label="Downloaded" />}
        </HStack>
      </VStack>

      {isDownloading && (
        <VStack gap={2} style={{ width: "100%" }}>
          <ProgressBar 
            value={model.progress * 100} 
            max={100} 
            label="Downloading model..." 
            hasValueLabel
          />
        </VStack>
      )}

      <HStack style={{ justifyContent: "flex-end", width: "100%", marginTop: 4 }}>
        {model.is_external ? (
          <Badge variant="green" label="Ready (API)" />
        ) : model.is_downloaded ? (
          <Button 
            variant="secondary" 
            label="Remove Model"
            style={{ 
              color: "var(--color-recording, #ef4444)", 
              borderColor: "var(--color-border)",
              cursor: "pointer"
            }}
            onClick={() => onDelete(model.id)}
          />
        ) : isDownloading ? (
          <Button 
            variant="secondary" 
            label="Downloading..." 
            isDisabled 
            style={{ opacity: 0.6 }} 
          />
        ) : (
          <Button 
            variant="primary"
            label="Download"
            onClick={() => onDownload(model.id)}
            style={{ cursor: "pointer" }}
          />
        )}
      </HStack>
    </Card>
  );
}
