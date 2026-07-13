import { useToast, ToastVariant } from "../../hooks/useToast";

const BORDER_BY_VARIANT: Record<ToastVariant, string> = {
  info: "var(--color-accent)",
  success: "var(--color-speaker-2)",
  error: "var(--color-recording)",
};

/**
 * Toast — a fixed-position (top-right) stack of the current toasts driven by
 * the shared `useToast` store. Drop a single <Toast /> anywhere near the app
 * root; any component can call `showToast(...)` (from the hook or the exported
 * module function) to display a message.
 */
export default function Toast() {
  const { toasts, dismiss } = useToast();

  return (
    <div
      aria-live="polite"
      style={{
        position: "fixed",
        top: 16,
        right: 16,
        display: "flex",
        flexDirection: "column",
        gap: 10,
        zIndex: 9999,
        pointerEvents: "none",
        maxWidth: 360,
      }}
    >
      {toasts.map((t) => (
        <div
          key={t.id}
          role="status"
          onClick={() => dismiss(t.id)}
          style={{
            pointerEvents: "auto",
            cursor: "pointer",
            padding: "12px 16px",
            borderRadius: 10,
            backgroundColor: "var(--color-background-elevated)",
            color: "var(--color-text-primary)",
            border: "1px solid var(--color-border)",
            borderLeft: `4px solid ${BORDER_BY_VARIANT[t.variant] ?? BORDER_BY_VARIANT.info}`,
            boxShadow: "0 4px 16px rgba(0, 0, 0, 0.25)",
            fontSize: 14,
            lineHeight: 1.4,
            animation: "voco-toast-in 0.22s cubic-bezier(0.16, 1, 0.3, 1)",
          }}
        >
          {t.message}
        </div>
      ))}
      <style>
        {`@keyframes voco-toast-in {
            from { opacity: 0; transform: translateX(24px); }
            to { opacity: 1; transform: translateX(0); }
          }`}
      </style>
    </div>
  );
}
