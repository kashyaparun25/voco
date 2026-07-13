import { Card } from "@astryxdesign/core/Card";
import { Text } from "@astryxdesign/core/Text";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { THEMES, type ThemeId, type ThemeMeta } from "../../lib/themes";
import CustomThemeBuilder from "./CustomThemeBuilder";

interface ThemeSettingsProps {
  activeThemeId: string;
  onSelect: (id: ThemeId) => void;
}

function ThemeCard({
  theme,
  isActive,
  onSelect,
}: {
  theme: ThemeMeta;
  isActive: boolean;
  onSelect: (id: ThemeId) => void;
}) {
  const swatches = [
    theme.preview.bg,
    theme.preview.surface,
    theme.preview.accent,
    theme.preview.text,
  ];

  return (
    <Card
      onClick={() => onSelect(theme.id)}
      style={{
        padding: 16,
        cursor: "pointer",
        backgroundColor: "var(--color-background-surface)",
        border: isActive
          ? "2px solid var(--color-accent)"
          : "2px solid var(--color-border)",
        borderRadius: "12px",
        transition: "border-color 150ms ease, transform 150ms ease",
        display: "flex",
        flexDirection: "column",
        gap: 12,
      }}
    >
      <VStack gap={2} style={{ width: "100%" }}>
        <HStack style={{ justifyContent: "space-between", alignItems: "center", width: "100%" }}>
          <HStack gap={2} style={{ alignItems: "center" }}>
            <span style={{ fontSize: "20px", lineHeight: 1 }} aria-hidden="true">
              {theme.emoji}
            </span>
            <Text style={{ fontSize: "16px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
              {theme.name}
            </Text>
          </HStack>
          {isActive && (
            <Text style={{ fontSize: "12px", fontWeight: "bold", color: "var(--color-accent-text)" }}>
              Active
            </Text>
          )}
        </HStack>

        <Text style={{ fontSize: "13px", color: "var(--color-text-secondary)", lineHeight: 1.4 }}>
          {theme.description}
        </Text>
      </VStack>

      {/* Swatch preview row — uses representative hex values from the registry */}
      <HStack gap={0} style={{ width: "100%", borderRadius: "8px", overflow: "hidden", border: "1px solid var(--color-border)" }}>
        {swatches.map((color, i) => (
          <div
            key={i}
            style={{
              flex: 1,
              height: "28px",
              backgroundColor: color,
            }}
          />
        ))}
      </HStack>
    </Card>
  );
}

export default function ThemeSettings({ activeThemeId, onSelect }: ThemeSettingsProps) {
  return (
    <VStack gap={4} style={{ width: "100%" }}>
      <VStack gap={1}>
        <Text style={{ fontSize: "20px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
          Appearance
        </Text>
        <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
          Choose a theme for Voco. Changes apply instantly.
        </Text>
      </VStack>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(240px, 1fr))",
          gap: "16px",
          width: "100%",
        }}
      >
        {THEMES.map((theme) => (
          <ThemeCard
            key={theme.id}
            theme={theme}
            isActive={theme.id === activeThemeId}
            onSelect={onSelect}
          />
        ))}
      </div>

      <CustomThemeBuilder
        isActive={activeThemeId === "custom"}
        onActivate={() => onSelect("custom")}
      />
    </VStack>
  );
}
