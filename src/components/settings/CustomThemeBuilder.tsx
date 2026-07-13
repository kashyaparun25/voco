import { useState } from "react";
import { Card } from "@astryxdesign/core/Card";
import { Button } from "../ui";
import { Text } from "@astryxdesign/core/Text";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import {
  applyCustomPalette,
  saveCustomPalette,
  readCustomPalette,
  DEFAULT_CUSTOM_PALETTE,
  type CustomPalette,
} from "../../lib/themes";

interface CustomThemeBuilderProps {
  /** Whether the custom theme is the active theme (drives live apply). */
  isActive: boolean;
  /** Called when the user wants to switch to / activate the custom theme. */
  onActivate: () => void;
}

const FIELDS: Array<{ key: keyof CustomPalette; label: string }> = [
  { key: "accent", label: "Accent" },
  { key: "backgroundApp", label: "Background (App)" },
  { key: "backgroundSurface", label: "Background (Surface)" },
  { key: "backgroundElevated", label: "Background (Elevated)" },
  { key: "textPrimary", label: "Text (Primary)" },
  { key: "textSecondary", label: "Text (Secondary)" },
  { key: "border", label: "Border" },
];

export default function CustomThemeBuilder({ isActive, onActivate }: CustomThemeBuilderProps) {
  const [palette, setPalette] = useState<CustomPalette>(() => readCustomPalette());

  const update = (key: keyof CustomPalette, value: string) => {
    const next = { ...palette, [key]: value };
    setPalette(next);
    saveCustomPalette(next);
    // Apply live only when custom is the active theme so the user sees it.
    if (isActive) applyCustomPalette(next);
  };

  const reset = () => {
    setPalette(DEFAULT_CUSTOM_PALETTE);
    saveCustomPalette(DEFAULT_CUSTOM_PALETTE);
    if (isActive) applyCustomPalette(DEFAULT_CUSTOM_PALETTE);
  };

  return (
    <Card
      style={{
        padding: 20,
        backgroundColor: "var(--color-background-surface)",
        border: "1px solid var(--color-border)",
        borderRadius: "12px",
        display: "flex",
        flexDirection: "column",
        gap: 16,
        width: "100%",
      }}
    >
      <HStack style={{ justifyContent: "space-between", alignItems: "center", width: "100%" }}>
        <VStack gap={1}>
          <Text style={{ fontSize: "16px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
            Custom Theme Builder
          </Text>
          <Text style={{ fontSize: "13px", color: "var(--color-text-secondary)" }}>
            Pick your own colors. {isActive ? "Changes apply instantly." : "Activate the Custom theme to see it live."}
          </Text>
        </VStack>
        {!isActive && (
          <Button
            variant="primary"
            onClick={() => {
              applyCustomPalette(palette);
              onActivate();
            }}
            label="Use Custom Theme"
            style={{
              padding: "8px 16px",
              borderRadius: "8px",
              backgroundColor: "var(--color-accent)",
              color: "#ffffff",
              border: "none",
              cursor: "pointer",
              fontWeight: 600,
            }}
          />
        )}
      </HStack>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
          gap: "12px",
          width: "100%",
        }}
      >
        {FIELDS.map(({ key, label }) => (
          <label
            key={key}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 10,
              padding: "8px 10px",
              borderRadius: "8px",
              border: "1px solid var(--color-border)",
              backgroundColor: "var(--color-background-elevated)",
            }}
          >
            <Text style={{ fontSize: "12px", fontWeight: 600, color: "var(--color-text-secondary)" }}>
              {label}
            </Text>
            <HStack gap={2} style={{ alignItems: "center" }}>
              <Text style={{ fontSize: "11px", fontFamily: "monospace", color: "var(--color-text-secondary)" }}>
                {palette[key]}
              </Text>
              <input
                type="color"
                value={toHex(palette[key])}
                onChange={(e) => update(key, e.target.value)}
                aria-label={`${label} color`}
                style={{
                  width: 32,
                  height: 32,
                  padding: 0,
                  border: "1px solid var(--color-border-strong)",
                  borderRadius: 6,
                  background: "none",
                  cursor: "pointer",
                }}
              />
            </HStack>
          </label>
        ))}
      </div>

      <HStack style={{ justifyContent: "flex-end" }}>
        <Button
          variant="secondary"
          onClick={reset}
          label="Reset to Defaults"
          style={{
            padding: "8px 14px",
            borderRadius: "8px",
            border: "1px solid var(--color-border-strong)",
            backgroundColor: "var(--color-background-surface-hover)",
            color: "var(--color-text-primary)",
            cursor: "pointer",
            fontSize: 13,
          }}
        />
      </HStack>
    </Card>
  );
}

/**
 * <input type="color"> only accepts #rrggbb. Coerce any stored value
 * (which may be rgba() etc.) into a hex the picker can display.
 */
function toHex(value: string): string {
  if (/^#[0-9a-fA-F]{6}$/.test(value)) return value;
  if (/^#[0-9a-fA-F]{3}$/.test(value)) {
    const r = value[1], g = value[2], b = value[3];
    return `#${r}${r}${g}${g}${b}${b}`;
  }
  return "#000000";
}
