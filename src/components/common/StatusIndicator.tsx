import { Text } from "@astryxdesign/core/Text";

export type StatusKind = "connected" | "error" | "checking" | "idle";

export interface StatusIndicatorProps {
  status: StatusKind;
  label?: string;
}

const COLOR_BY_STATUS: Record<StatusKind, string> = {
  connected: "var(--color-speaker-2)", // green
  error: "var(--color-recording)", // red
  checking: "var(--color-speaker-3)", // amber
  idle: "var(--color-text-secondary)", // muted
};

export default function StatusIndicator({ status, label }: StatusIndicatorProps) {
  const color = COLOR_BY_STATUS[status] ?? COLOR_BY_STATUS.idle;
  const isPulsing = status === "checking";

  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 8,
      }}
    >
      <span
        aria-hidden="true"
        style={{
          width: 10,
          height: 10,
          borderRadius: "50%",
          backgroundColor: color,
          flexShrink: 0,
          boxShadow: `0 0 0 3px color-mix(in srgb, ${color} 20%, transparent)`,
          animation: isPulsing ? "voco-status-pulse 1.2s ease-in-out infinite" : undefined,
        }}
      />
      {label ? (
        <Text style={{ fontSize: 13, color: "var(--color-text-secondary)" }}>{label}</Text>
      ) : null}
      <style>
        {`@keyframes voco-status-pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.35; }
          }`}
      </style>
    </span>
  );
}
