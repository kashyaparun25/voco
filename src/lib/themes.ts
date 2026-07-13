/**
 * themes.ts - Typed registry of Voco's curated theme system.
 *
 * Each theme's actual colors live in `src/styles/themes/<id>.css` as CSS custom
 * properties scoped under `[data-theme="<id>"]`. The `preview` hex values here
 * are representative swatches (matching the CSS) used to render selection cards.
 * These preview hexes are the ONLY place hard-coded colors are permitted in TS,
 * alongside the .css theme files which are the source of truth.
 */

export type ThemeId =
  | "midnight"
  | "daylight"
  | "aurora"
  | "sunset"
  | "ocean"
  | "monochrome"
  | "rose"
  | "neon"
  | "custom";

export interface ThemeMeta {
  id: ThemeId;
  name: string;
  emoji: string;
  description: string;
  mode: "dark" | "light";
  preview: {
    bg: string;
    surface: string;
    accent: string;
    text: string;
  };
}

export const THEMES: ThemeMeta[] = [
  {
    id: "midnight",
    name: "Midnight",
    emoji: "🌙",
    description: "Deep dark with electric violet accents.",
    mode: "dark",
    preview: {
      bg: "#0a0a12",
      surface: "#12121e",
      accent: "#7c3aed",
      text: "#e8e8f0",
    },
  },
  {
    id: "daylight",
    name: "Daylight",
    emoji: "☀️",
    description: "Clean white and soft gray for bright rooms.",
    mode: "light",
    preview: {
      bg: "#f5f5f7",
      surface: "#ffffff",
      accent: "#7c3aed",
      text: "#1d1d1f",
    },
  },
  {
    id: "aurora",
    name: "Aurora",
    emoji: "🌌",
    description: "Dark night sky with purple-to-cyan-to-green gradients.",
    mode: "dark",
    preview: {
      bg: "#071018",
      surface: "#0c1a24",
      accent: "#2dd4bf",
      text: "#e6f4f1",
    },
  },
  {
    id: "sunset",
    name: "Sunset",
    emoji: "🌅",
    description: "Warm dark tones fading from amber and orange to rose.",
    mode: "dark",
    preview: {
      bg: "#1a0f14",
      surface: "#241419",
      accent: "#f97316",
      text: "#fbeee6",
    },
  },
  {
    id: "ocean",
    name: "Ocean",
    emoji: "🌊",
    description: "Deep navy depths with teal and aqua highlights.",
    mode: "dark",
    preview: {
      bg: "#071624",
      surface: "#0c2033",
      accent: "#0ea5e9",
      text: "#e2f1f8",
    },
  },
  {
    id: "monochrome",
    name: "Monochrome",
    emoji: "⬛",
    description: "Pure black and white; color only for speaker tags.",
    mode: "dark",
    preview: {
      bg: "#000000",
      surface: "#0d0d0d",
      accent: "#e5e5e5",
      text: "#f5f5f5",
    },
  },
  {
    id: "rose",
    name: "Rosé",
    emoji: "🌸",
    description: "Light and airy with blush pink and rose gold.",
    mode: "light",
    preview: {
      bg: "#fdf2f4",
      surface: "#ffffff",
      accent: "#e11d74",
      text: "#3f2a30",
    },
  },
  {
    id: "neon",
    name: "Neon",
    emoji: "💜",
    description: "True black with vivid neon green, pink, and blue.",
    mode: "dark",
    preview: {
      bg: "#000000",
      surface: "#0a0a0a",
      accent: "#39ff14",
      text: "#f0fff4",
    },
  },
  {
    id: "custom",
    name: "Custom",
    emoji: "🎨",
    description: "Build your own palette with the theme builder.",
    mode: "dark",
    preview: {
      bg: "#0a0a12",
      surface: "#12121e",
      accent: "#7c3aed",
      text: "#e8e8f0",
    },
  },
];

/**
 * Custom theme palette — the set of tokens the builder lets a user override.
 * Values are CSS colors, applied as inline custom properties on
 * `document.documentElement`.
 */
export interface CustomPalette {
  accent: string;
  backgroundApp: string;
  backgroundSurface: string;
  backgroundElevated: string;
  textPrimary: string;
  textSecondary: string;
  border: string;
}

/** Maps CustomPalette keys to the CSS custom property they drive. */
export const CUSTOM_TOKEN_MAP: Record<keyof CustomPalette, string> = {
  accent: "--color-accent",
  backgroundApp: "--color-background-app",
  backgroundSurface: "--color-background-surface",
  backgroundElevated: "--color-background-elevated",
  textPrimary: "--color-text-primary",
  textSecondary: "--color-text-secondary",
  border: "--color-border",
};

export const DEFAULT_CUSTOM_PALETTE: CustomPalette = {
  accent: "#7c3aed",
  backgroundApp: "#0a0a12",
  backgroundSurface: "#12121e",
  backgroundElevated: "#1e1e32",
  textPrimary: "#e8e8f0",
  textSecondary: "#9090a8",
  border: "#2a2a3a",
};

export const CUSTOM_THEME_STORAGE_KEY = "voco-custom-theme";

/** Read the persisted custom palette (falling back to the default). */
export function readCustomPalette(): CustomPalette {
  try {
    const raw = localStorage.getItem(CUSTOM_THEME_STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<CustomPalette>;
      return { ...DEFAULT_CUSTOM_PALETTE, ...parsed };
    }
  } catch {
    // Ignore parse / storage failures.
  }
  return DEFAULT_CUSTOM_PALETTE;
}

/** Persist the custom palette. */
export function saveCustomPalette(palette: CustomPalette): void {
  try {
    localStorage.setItem(CUSTOM_THEME_STORAGE_KEY, JSON.stringify(palette));
  } catch {
    // Ignore persistence failures.
  }
}

/**
 * The element the theme tokens live on. Prefer the in-<Theme> wrapper (see
 * App.tsx) so custom-theme vars aren't shadowed by astryx's neutral tokens;
 * fall back to <html> before the wrapper mounts.
 */
function themeRoot(): HTMLElement {
  if (typeof document === "undefined") {
    return {} as HTMLElement;
  }
  return document.getElementById("voco-theme-root") ?? document.documentElement;
}

/** Apply the custom palette as inline CSS custom properties on the theme root. */
export function applyCustomPalette(palette: CustomPalette): void {
  const root = themeRoot();
  (Object.keys(CUSTOM_TOKEN_MAP) as Array<keyof CustomPalette>).forEach((key) => {
    root.style?.setProperty(CUSTOM_TOKEN_MAP[key], palette[key]);
  });
}

/** Remove all inline custom-theme overrides from the theme root. */
export function clearCustomPalette(): void {
  const root = themeRoot();
  Object.values(CUSTOM_TOKEN_MAP).forEach((cssVar) => {
    root.style?.removeProperty(cssVar);
  });
}

const DEFAULT_THEME = THEMES[0]; // midnight

/**
 * Resolve a theme by id, falling back to Midnight for unknown ids.
 */
export function getTheme(id: string): ThemeMeta {
  return THEMES.find((t) => t.id === id) ?? DEFAULT_THEME;
}
