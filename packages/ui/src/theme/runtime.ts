import { DEFAULT_THEME_BY_SECTION, THEMES } from "../tokens/themes";
import type { Section, ThemeId } from "../tokens/types";
import { THEME_IDS } from "../tokens/types";
import { isHexColor, readableInk } from "../util/contrast";

export interface ThemeSelection {
  section: Section;
  /** Platform default per section is built in; this overrides it for tests/embedding. */
  platformDefault?: ThemeId;
  /** "One theme for everything" (Settings > Appearance). */
  userGlobal?: ThemeId | null;
  /** Per-section override (Settings > Appearance > Per-section themes). */
  perSection?: Partial<Record<Section, ThemeId>>;
}

export function isThemeId(value: unknown): value is ThemeId {
  return typeof value === "string" && (THEME_IDS as readonly string[]).includes(value);
}

/**
 * DS-TOK-004: platform default -> user global -> per-section override.
 * (Server accent is layered on top as tokens, see `serverAccentTokens`.)
 * Unknown ids are ignored so stale settings can never break rendering.
 */
export function resolveTheme(sel: ThemeSelection): ThemeId {
  const override = sel.perSection?.[sel.section];
  if (isThemeId(override)) return override;
  if (isThemeId(sel.userGlobal)) return sel.userGlobal;
  if (isThemeId(sel.platformDefault)) return sel.platformDefault;
  return DEFAULT_THEME_BY_SECTION[sel.section];
}

/** Recolor the accent for a server (only applied when the viewer allows server theming). */
export function serverAccentTokens(accent: string | null | undefined): Record<string, string> {
  if (!accent || !isHexColor(accent)) return {};
  return {
    "--ui-accent": accent,
    "--ui-accent-hover": `color-mix(in srgb, ${accent} 85%, black)`,
    "--ui-accent-ink": readableInk(accent),
    "--brand": accent,
    "--server-accent": accent,
  };
}

export interface AppearanceSettings {
  /** `system` follows prefers-reduced-motion. */
  reducedMotion: "system" | "on" | "off";
  contrast: "normal" | "high";
  /** 0..100, Discord parity. */
  saturation: number;
  /** Base font size in px, 12..24. */
  fontSize: number;
}

export const DEFAULT_APPEARANCE: AppearanceSettings = {
  reducedMotion: "system",
  contrast: "normal",
  saturation: 100,
  fontSize: 16,
};

export const FONT_SIZE_MIN = 12;
export const FONT_SIZE_MAX = 24;

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, Number.isFinite(n) ? n : lo));

export function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-reduced-motion: reduce)").matches
    : false;
}

/** DS-TOK-006: runtime variables, applied on the provider root. */
export function appearanceVars(settings: AppearanceSettings, systemReducedMotion: boolean): Record<string, string> {
  const reduced = settings.reducedMotion === "on" || (settings.reducedMotion === "system" && systemReducedMotion);
  const fontSize = clamp(settings.fontSize, FONT_SIZE_MIN, FONT_SIZE_MAX);
  const vars: Record<string, string> = {
    "--ui-reduced-motion": reduced ? "1" : "0",
    "--ui-motion-duration": reduced ? "0.001ms" : "160ms",
    "--ui-saturation": String(clamp(settings.saturation, 0, 100) / 100),
    "--ui-font-size-base": `${fontSize}px`,
    "--ui-font-scale": String(fontSize / 16),
    "--sr-scanline-opacity": reduced ? "0" : "1",
  };
  if (settings.contrast === "high") {
    vars["--ui-contrast-boost"] = "1";
    vars["--ui-text-muted"] = "var(--ui-text)";
    vars["--ui-text-faint"] = "var(--ui-text)";
    vars["--ui-border"] = "var(--ui-text-muted)";
  } else {
    vars["--ui-contrast-boost"] = "0";
  }
  return vars;
}

export function themeScheme(id: ThemeId): "light" | "dark" {
  return THEMES[id].scheme;
}
