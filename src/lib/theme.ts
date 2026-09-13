// SPEC: settings-storage-i18n (CFG-05, CFG-06, CFG-09), epub-fidelity (FID-14)

import type { CustomTheme } from "../types";

export const SUPPORTED_THEMES = ["light", "dark", "sepia", "custom"] as const;
export type Theme = (typeof SUPPORTED_THEMES)[number];
export const DEFAULT_THEME: Theme = "dark";

export const THEME_LABEL_KEYS: Record<Theme, string> = {
  light: "settings.themeLight",
  dark: "settings.themeDark",
  sepia: "settings.themeSepia",
  custom: "settings.themeCustom",
};

/** Themes that stopped existing after users could already have them persisted.
 *  Mapping them instead of dropping them keeps a user from silently landing on
 *  the default and believing the app forgot their choice. `ocean` and
 *  `terracotta` left on 2026-09-12 (AD-073): the warm cream one is closest to
 *  sepia, the dark teal one to dark. */
const RENAMED_THEMES: Record<string, Theme> = {
  claude: "sepia",
  terracotta: "sepia",
  ocean: "dark",
};

const HEX = /^#[0-9a-fA-F]{6}$/;
const THEME_KEY = "readme-theme";
const CUSTOM_KEY = "readme-custom-theme";
const CUSTOM_VARS = ["--bg-app", "--text-primary", "--accent"] as const;

/** `#rrggbb` and nothing else. Checked here as well as in Rust
 *  (`config::is_hex_color`) because the value is written into CSS - on the
 *  app's root and inside the book's iframe - and anything else there would be
 *  markup, not a color. */
export function isHexColor(value: unknown): value is string {
  return typeof value === "string" && HEX.test(value);
}

function isCustomTheme(value: unknown): value is CustomTheme {
  const c = value as CustomTheme | null;
  return !!c && isHexColor(c.background) && isHexColor(c.text) && isHexColor(c.accent);
}

export function normalizeTheme(raw: string | null | undefined): Theme {
  const value = raw ?? "";
  if (SUPPORTED_THEMES.includes(value as Theme)) return value as Theme;
  return RENAMED_THEMES[value] ?? DEFAULT_THEME;
}

export function cachedTheme(): Theme {
  return normalizeTheme(localStorage.getItem(THEME_KEY));
}

/** The custom colors from the last session, so a custom theme paints before
 *  the config arrives instead of flashing the defaults (CFG-05 AC 4). */
export function cachedCustomTheme(): CustomTheme | null {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(CUSTOM_KEY) ?? "null");
    return isCustomTheme(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export function applyTheme(theme: string, custom?: CustomTheme | null) {
  const normalized = normalizeTheme(theme);
  const root = document.documentElement;
  root.setAttribute("data-theme", normalized);
  localStorage.setItem(THEME_KEY, normalized);
  if (normalized === "custom" && isCustomTheme(custom)) {
    root.style.setProperty("--bg-app", custom.background);
    root.style.setProperty("--text-primary", custom.text);
    root.style.setProperty("--accent", custom.accent);
    localStorage.setItem(CUSTOM_KEY, JSON.stringify(custom));
  } else {
    // Inline values would otherwise keep beating the preset's stylesheet.
    CUSTOM_VARS.forEach((name) => root.style.removeProperty(name));
  }
}

function readVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** The colors on screen right now, or null if any is not a plain hex. The
 *  starting point of a custom theme, so choosing it does not jump to white. */
export function currentColors(): CustomTheme | null {
  const colors = {
    background: readVar("--bg-app"),
    text: readVar("--text-primary"),
    accent: readVar("--accent"),
  };
  return isCustomTheme(colors) ? colors : null;
}

/** Background and text for the book's page (FID-14). Null means "do not force
 *  anything": a value that is not a validated hex never reaches the iframe. */
export function pageColors(): { background: string; text: string } | null {
  const colors = currentColors();
  return colors ? { background: colors.background, text: colors.text } : null;
}
