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

/** Colour values that relative colour syntax can read (no `var()`, no `color-mix()`). */
const SIMPLE_COLOR = /^(#[0-9a-f]{3,8}|rgba?\([^()]*\)|hsla?\([^()]*\))$/i;

/**
 * Q35 / Q61: saturation without CSS `filter`. A `filter` makes `position: fixed` descendants
 * position against the filtered root, which breaks popovers and menus while it is active.
 * Each simple colour token keeps its authored value in a `--sat-base-*` twin and is re-derived
 * with relative colour syntax, so no containing block is created. At 100 the tokens pass through.
 */
export function saturatedTokens(tokens: Record<string, string>, saturation: number): Record<string, string> {
  if (clamp(saturation, 0, 100) >= 100) return tokens;
  const out: Record<string, string> = {};
  for (const [name, value] of Object.entries(tokens)) {
    if (name.startsWith("--") && SIMPLE_COLOR.test(value.trim())) {
      const base = `--sat-base-${name.slice(2)}`;
      out[base] = value.trim();
      out[name] = `oklch(from var(${base}) l calc(c * var(--ui-saturation)) h / alpha)`;
    } else {
      out[name] = value;
    }
  }
  return out;
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
    // The legacy and Side Room tokens that most of the Lounge, Cinema and Library CSS reads directly: without
    // these the setting would only reach the primitives (RDY-A11Y-004). The fallbacks keep a theme that lacks
    // a token (the Lounge has no --sr-*) from turning the property invalid.
    vars["--text-muted"] = "var(--text-normal, var(--ui-text))";
    vars["--interactive-normal"] = "var(--text-normal, var(--ui-text))";
    vars["--sr-text-2"] = "var(--sr-text, var(--ui-text))";
    vars["--sr-text-3"] = "var(--sr-text, var(--ui-text))";
    vars["--sr-line"] = "var(--sr-text, var(--ui-text))";
  } else {
    vars["--ui-contrast-boost"] = "0";
  }
  return vars;
}

export function themeScheme(id: ThemeId): "light" | "dark" {
  return THEMES[id].scheme;
}
