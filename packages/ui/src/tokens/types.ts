/**
 * Token schema (DS-TOK-001). Theme ids and semantic token names are generated from the
 * sr-theme registry into `../theme/tokens.generated.ts`; do not redeclare them here.
 */
import type { ThemeId, UiTokenName } from "../theme/tokens.generated";

export { THEME_IDS, UI_TOKEN_NAMES, KNOWN_TOKEN_NAMES } from "../theme/tokens.generated";
export type { ThemeId, UiTokenName, KnownTokenName } from "../theme/tokens.generated";

export const SECTIONS = ["lounge", "staff", "developer", "entrance", "cinema", "library", "support", "live-tv"] as const;
export type Section = (typeof SECTIONS)[number];

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
