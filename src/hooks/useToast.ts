import { useSyncExternalStore, useCallback } from "react";

export type ToastVariant = "info" | "success" | "error";

export interface ToastItem {
  id: number;
  message: string;
  variant: ToastVariant;
}

// --- Module-level pub/sub store so any component can trigger toasts ---
// (No prop drilling, dependency-free.)
let toasts: ToastItem[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const timers = new Map<number, ReturnType<typeof setTimeout>>();

const AUTO_DISMISS_MS = 4000;

function emit() {
  // Notify all subscribers with a fresh reference for useSyncExternalStore.
  for (const listener of listeners) {
    listener();
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshot(): ToastItem[] {
  return toasts;
}

/** Add an auto-dismissing toast. Usable from anywhere (not just React). */
export function showToast(message: string, variant: ToastVariant = "info"): number {
  const id = nextId++;
  toasts = [...toasts, { id, message, variant }];
  emit();

  const timer = setTimeout(() => {
    dismiss(id);
  }, AUTO_DISMISS_MS);
  timers.set(id, timer);

  return id;
}

/** Remove a toast by id. */
export function dismiss(id: number): void {
  const timer = timers.get(id);
  if (timer) {
    clearTimeout(timer);
    timers.delete(id);
  }
  const next = toasts.filter((t) => t.id !== id);
  if (next.length !== toasts.length) {
    toasts = next;
    emit();
  }
}

/**
 * Hook exposing the shared toast store.
 * Returns { toasts, showToast, dismiss }.
 */
export function useToast(): {
  toasts: ToastItem[];
  showToast: (message: string, variant?: ToastVariant) => number;
  dismiss: (id: number) => void;
} {
  const current = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  const show = useCallback(
    (message: string, variant: ToastVariant = "info") => showToast(message, variant),
    []
  );
  const remove = useCallback((id: number) => dismiss(id), []);

  return { toasts: current, showToast: show, dismiss: remove };
}

export default useToast;
