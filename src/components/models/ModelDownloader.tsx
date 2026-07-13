import { Card } from "@astryxdesign/core/Card";
import { Text } from "@astryxdesign/core/Text";
import { ProgressBar } from "@astryxdesign/core/ProgressBar";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { ModelInfo } from "./ModelCard";

interface ModelDownloaderProps {
  downloadingModels: ModelInfo[];
}

export default function ModelDownloader({ downloadingModels }: ModelDownloaderProps) {
  if (downloadingModels.length === 0) return null;

  return (
    <Card style={{
      padding: 16,
      backgroundColor: "rgba(124, 58, 237, 0.05)",
      border: "1px dashed var(--color-accent)",
      borderRadius: "12px",
      display: "flex",
      flexDirection: "column",
      gap: 12
    }}>
      <HStack style={{ alignItems: "center", gap: 8 }}>
        <div style={{
          width: 8,
          height: 8,
          borderRadius: "50%",
          backgroundColor: "var(--color-accent)",
          animation: "pill-pulse 1.5s infinite ease-in-out"
        }} />
        <Text style={{ fontSize: "14px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
          Active Downloads ({downloadingModels.length})
        </Text>
      </HStack>

      <VStack gap={4}>
        {downloadingModels.map((model) => (
          <VStack key={model.id} gap={1} style={{ width: "100%" }}>
            <HStack style={{ justifyContent: "space-between", fontSize: "12px" }}>
              <Text style={{ fontWeight: "500", color: "var(--color-text-primary)" }}>
                {model.name}
              </Text>
              <Text style={{ color: "var(--color-text-secondary)" }}>
                {Math.round(model.progress * 100)}%
              </Text>
            </HStack>
            <ProgressBar 
              value={model.progress * 100} 
              max={100} 
              label={`Downloading ${model.name}`} 
              isLabelHidden
            />
          </VStack>
        ))}
      </VStack>
    </Card>
  );
}
