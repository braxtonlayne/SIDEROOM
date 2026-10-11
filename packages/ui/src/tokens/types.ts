/** Token schema (DS-TOK-001). Mirrors the `sr-theme` token schema; to be regenerated from it once it exists. */

export const THEME_IDS = [
  "lounge-dark",
  "lounge-light",
  "lounge-ash",
  "lounge-onyx",
  "sideroom",
  "sideroom-broadcast",
] as const;
export type ThemeId = (typeof THEME_IDS)[number];

export const SECTIONS = ["lounge", "staff", "developer", "entrance", "cinema", "library", "support", "live-tv"] as const;
export type Section = (typeof SECTIONS)[number];

/** Semantic tokens every theme must define. Primitives use only these. */
export const UI_TOKEN_NAMES = [
  "--ui-bg",
  "--ui-bg-raised",
  "--ui-bg-overlay",
  "--ui-bg-hover",
  "--ui-border",
  "--ui-text",
  "--ui-text-muted",
  "--ui-text-faint",
  "--ui-accent",
  "--ui-accent-hover",
  "--ui-accent-ink",
  "--ui-danger",
  "--ui-danger-ink",
  "--ui-success",
  "--ui-warning",
  "--ui-info",
  "--ui-focus",
  "--ui-radius",
  "--ui-radius-lg",
  "--ui-font-body",
  "--ui-font-display",
  "--ui-font-mono",
  "--ui-shadow-overlay",
  "--ui-backdrop",
  "--ui-status-online",
  "--ui-status-idle",
  "--ui-status-dnd",
  "--ui-status-offline",
] as const;
export type UiTokenName = (typeof UI_TOKEN_NAMES)[number];

/** A token name is always a CSS custom property. */
export type TokenName = `--${string}`;
export type TokenSet = Record<TokenName, string>;
export type ThemeTokens = TokenSet & Record<UiTokenName, string>;

export interface ThemeDefinition {
  id: ThemeId;
  name: string;
  family: "lounge" | "sideroom";
  /** Used for the CSS `color-scheme` property. */
  scheme: "light" | "dark";
  /** Theme this one extends (informational; tokens are already merged). */
  extends?: ThemeId;
  tokens: Partial<TokenSet> & Record<UiTokenName, string>;
}
