import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card } from "@astryxdesign/core/Card";
import { Text } from "@astryxdesign/core/Text";
import { Badge } from "@astryxdesign/core/Badge";
import { VStack, HStack } from "@astryxdesign/core/Layout";

/**
 * The backend `recommend_models()` shape is not finalized. We read a few
 * likely field names defensively so the banner renders whatever is available.
 */
interface RecommendationRaw {
  tier?: string;
  stt_model?: string;
  stt?: string;
  recommended_stt?: string;
  llm_model?: string;
  llm?: string;
  recommended_llm?: string;
  ram_mb?: number;
  [key: string]: unknown;
}

function pick(...vals: Array<unknown>): string | null {
  for (const v of vals) {
    if (typeof v === "string" && v.trim()) return v;
  }
  return null;
}

const ChipIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 18, height: 18 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 0 0 2.25-2.25V6.75a2.25 2.25 0 0 0-2.25-2.25H6.75A2.25 2.25 0 0 0 4.5 6.75v10.5a2.25 2.25 0 0 0 2.25 2.25Zm.75-12h9v9h-9v-9Z" />
  </svg>
);

/**
 * ModelRecommendation — "Recommended for your Mac" banner.
 *
 * Reads detected RAM via `get_system_ram_mb()` and a recommendation tier /
 * suggested models via `recommend_models()`. Renders nothing if neither
 * command is available.
 */
export default function ModelRecommendation() {
  const [ramMb, setRamMb] = useState<number | null>(null);
  const [rec, setRec] = useState<RecommendationRaw | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const ram = await invoke<number>("get_system_ram_mb");
        if (!cancelled && typeof ram === "number") setRamMb(ram);
      } catch (err) {
        console.warn("get_system_ram_mb unavailable:", err);
      }
      try {
        const r = await invoke<RecommendationRaw>("recommend_models");
        if (!cancelled && r && typeof r === "object") setRec(r);
      } catch (err) {
        console.warn("recommend_models unavailable:", err);
      }
    })();
    return () => { cancelled = true; };
  }, []);

  if (ramMb === null && !rec) return null;

  const tier = pick(rec?.tier);
  const stt = pick(rec?.stt_model, rec?.recommended_stt, rec?.stt);
  const llm = pick(rec?.llm_model, rec?.recommended_llm, rec?.llm);
  const ramGb = ramMb !== null ? (ramMb / 1024).toFixed(0) : null;

  return (
    <Card style={{
      padding: 16,
      backgroundColor: "rgba(124, 58, 237, 0.05)",
      border: "1px solid var(--color-accent)",
      borderRadius: "12px",
      display: "flex",
      flexDirection: "column",
      gap: 12,
    }}>
      <HStack style={{ justifyContent: "space-between", alignItems: "center" }}>
        <HStack gap={2} style={{ alignItems: "center", color: "var(--color-accent)" }}>
          <ChipIcon />
          <Text style={{ fontSize: "14px", fontWeight: "bold", color: "var(--color-text-primary)" }}>
            Recommended for your Mac
          </Text>
        </HStack>
        <HStack gap={2} style={{ alignItems: "center" }}>
          {tier && <Badge variant="green" label={tier.toUpperCase()} />}
          {ramGb && (
            <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
              {ramGb} GB RAM
            </Text>
          )}
        </HStack>
      </HStack>

      {(stt || llm) && (
        <VStack gap={1}>
          {stt && (
            <HStack gap={2} style={{ alignItems: "center" }}>
              <Text style={{ fontSize: "12px", fontWeight: 600, color: "var(--color-text-secondary)", minWidth: 44 }}>STT</Text>
              <Text style={{ fontSize: "13px", color: "var(--color-text-primary)" }}>{stt}</Text>
            </HStack>
          )}
          {llm && (
            <HStack gap={2} style={{ alignItems: "center" }}>
              <Text style={{ fontSize: "12px", fontWeight: 600, color: "var(--color-text-secondary)", minWidth: 44 }}>LLM</Text>
              <Text style={{ fontSize: "13px", color: "var(--color-text-primary)" }}>{llm}</Text>
            </HStack>
          )}
        </VStack>
      )}

      {!stt && !llm && (
        <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>
          Based on your hardware, download a model below to get started.
        </Text>
      )}
    </Card>
  );
}
