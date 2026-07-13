import { useState, useEffect, useCallback } from "react";
import {
  getTheme,
  applyCustomPalette,
  clearCustomPalette,
  readCustomPalette,
  type ThemeId,
  type ThemeMeta,
} from "../lib/themes";

const STORAGE_KEY = "voco-theme";
const DEFAULT_THEME_ID: ThemeId = "midnight";

function readInitialThemeId(): ThemeId {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      // Validate against the registry; getTheme falls back to midnight.
      return getTheme(stored).id;
    }
  } catch {
    // localStorage unavailable (e.g. SSR / restricted context) — ignore.
  }
  return DEFAULT_THEME_ID;
}

export interface UseThemeResult {
  themeId: ThemeId;
  setThemeId: (id: ThemeId) => void;
  theme: ThemeMeta;
  mode: "dark" | "light";
}

/**
 * useTheme - self-contained theme state hook.
 *
 * Reads the initial theme from localStorage ("voco-theme", default "midnight"),
 * applies it to `document.documentElement` via the `data-theme` attribute,
 * persists changes back to localStorage, and derives the "dark"/"light" mode
 * from the theme registry.
 */
export function useTheme(): UseThemeResult {
  const [themeId, setThemeIdState] = useState<ThemeId>(readInitialThemeId);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", themeId);
    try {
      localStorage.setItem(STORAGE_KEY, themeId);
    } catch {
      // Ignore persistence failures.
    }

    // The custom theme is driven by inline CSS custom properties. Re-apply the
    // stored palette when it's active, and clear the overrides otherwise so
    // switching to a built-in theme restores its .css tokens.
    if (themeId === "custom") {
      applyCustomPalette(readCustomPalette());
    } else {
      clearCustomPalette();
    }
  }, [themeId]);

  // Keep the DOM in sync if another component (the builder) updates the stored
  // custom palette while the custom theme is active.
  useEffect(() => {
    if (themeId !== "custom") return;
    const onStorage = (e: StorageEvent) => {
      if (e.key === "voco-custom-theme") applyCustomPalette(readCustomPalette());
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, [themeId]);

  const setThemeId = useCallback((id: ThemeId) => {
    setThemeIdState(getTheme(id).id);
  }, []);

  const theme = getTheme(themeId);

  return {
    themeId,
    setThemeId,
    theme,
    mode: theme.mode,
  };
}
