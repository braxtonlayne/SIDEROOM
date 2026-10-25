import type { ThemeDefinition, ThemeId, ThemeTokens, Section } from "./types";

const LOUNGE_FONT = '"gg sans", "Inter", "Noto Sans", system-ui, -apple-system, "Segoe UI", sans-serif';
const LOUNGE_MONO = '"IBM Plex Mono", ui-monospace, Consolas, monospace';

/** Layout constants (docs/10 section 3). */
const LOUNGE_LAYOUT = {
  "--lounge-server-rail": "72px",
  "--lounge-channel-list": "240px",
  "--lounge-member-list": "240px",
  "--lounge-header": "48px",
  "--lounge-composer-min": "44px",
  "--lounge-message-gutter": "72px",
} as const;

const loungeShared = {
  "--ui-radius": "8px",
  "--ui-radius-lg": "12px",
  "--ui-font-body": LOUNGE_FONT,
  "--ui-font-display": LOUNGE_FONT,
  "--ui-font-mono": LOUNGE_MONO,
  "--ui-accent": "#5865f2",
  "--ui-accent-hover": "#4752c4",
  "--ui-accent-ink": "#ffffff",
  "--ui-focus": "#00a8fc",
  "--ui-status-online": "#23a55a",
  "--ui-status-idle": "#f0b232",
  "--ui-status-dnd": "#f23f42",
  "--ui-status-offline": "#80848e",
  "--brand": "#5865f2",
  ...LOUNGE_LAYOUT,
} as const;

export const loungeDark: ThemeDefinition = {
  id: "lounge-dark",
  name: "Lounge Dark",
  family: "lounge",
  scheme: "dark",
  tokens: {
    ...loungeShared,
    "--bg-base-lowest": "#1e1f22",
    "--bg-base-lower": "#2b2d31",
    "--bg-base-low": "#313338",
    "--bg-surface-overlay": "#2b2d31",
    "--text-normal": "#dbdee1",
    "--text-muted": "#949ba4",
    "--interactive-normal": "#b5bac1",
    "--interactive-hover": "#dbdee1",
    "--interactive-active": "#ffffff",
    "--mention-bg": "#5865f2",
    "--danger": "#da373c",
    "--ui-bg": "#313338",
    "--ui-bg-raised": "#2b2d31",
    "--ui-bg-overlay": "#2b2d31",
    "--ui-bg-hover": "#35373c",
    "--ui-border": "#3f4147",
    "--ui-text": "#dbdee1",
    "--ui-text-muted": "#a4aab3",
    "--ui-text-faint": "#9aa0aa",
    "--ui-danger": "#da373c",
    "--ui-danger-ink": "#ffffff",
    "--ui-success": "#23a55a",
    "--ui-warning": "#f0b232",
    "--ui-info": "#00a8fc",
    "--ui-shadow-overlay": "0 8px 24px rgb(0 0 0 / 0.45)",
    "--ui-backdrop": "rgb(0 0 0 / 0.7)",
  },
};

export const loungeLight: ThemeDefinition = {
  id: "lounge-light",
  name: "Lounge Light",
  family: "lounge",
  scheme: "light",
  tokens: {
    ...loungeShared,
    "--bg-base-lowest": "#e3e5e8",
    "--bg-base-lower": "#f2f3f5",
    "--bg-base-low": "#ffffff",
    "--bg-surface-overlay": "#ffffff",
    "--text-normal": "#313338",
    "--text-muted": "#5c5e66",
    "--interactive-normal": "#4e5058",
    "--interactive-hover": "#313338",
    "--interactive-active": "#060607",
    "--mention-bg": "#5865f2",
    "--danger": "#d12d35",
    "--ui-bg": "#ffffff",
    "--ui-bg-raised": "#f2f3f5",
    "--ui-bg-overlay": "#ffffff",
    "--ui-bg-hover": "#e8eaed",
    "--ui-border": "#c9ccd1",
    "--ui-text": "#313338",
    "--ui-text-muted": "#5c5e66",
    "--ui-text-faint": "#62666e",
    "--ui-danger": "#d12d35",
    "--ui-danger-ink": "#ffffff",
    "--ui-success": "#1a7f45",
    "--ui-warning": "#a96b00",
    "--ui-info": "#0068c9",
    "--ui-focus": "#0068c9",
    // Amber on white is 1.9:1; the dot needs 3:1 against the surface it sits on.
    "--ui-status-idle": "#b07800",
    "--ui-shadow-overlay": "0 8px 24px rgb(0 0 0 / 0.18)",
    "--ui-backdrop": "rgb(0 0 0 / 0.55)",
  },
};

export const loungeAsh: ThemeDefinition = {
  id: "lounge-ash",
  name: "Lounge Ash",
  family: "lounge",
  scheme: "dark",
  tokens: {
    ...loungeDark.tokens,
    "--bg-base-lowest": "#232428",
    "--bg-base-lower": "#2e3035",
    "--bg-base-low": "#383a40",
    "--bg-surface-overlay": "#2e3035",
    "--ui-bg": "#383a40",
    "--ui-bg-raised": "#2e3035",
    "--ui-bg-overlay": "#2e3035",
    "--ui-bg-hover": "#404249",
    "--ui-border": "#4a4d55",
    "--ui-text": "#e0e1e5",
    "--ui-text-muted": "#aeb2ba",
    "--ui-text-faint": "#a6acb6",
    // #949ba4 is 4.05:1 on this theme's #383a40 (tokens-contrast.test.ts walks every theme).
    "--text-muted": "#a2a8b2",
  },
};

export const loungeOnyx: ThemeDefinition = {
  id: "lounge-onyx",
  name: "Lounge Onyx",
  family: "lounge",
  scheme: "dark",
  tokens: {
    ...loungeDark.tokens,
    "--bg-base-lowest": "#000000",
    "--bg-base-lower": "#070708",
    "--bg-base-low": "#0b0b0c",
    "--bg-surface-overlay": "#121214",
    "--ui-bg": "#0b0b0c",
    "--ui-bg-raised": "#121214",
    "--ui-bg-overlay": "#121214",
    "--ui-bg-hover": "#1a1a1d",
    "--ui-border": "#26272b",
    "--ui-text": "#e3e5e8",
    "--ui-text-muted": "#a0a5ad",
    "--ui-backdrop": "rgb(0 0 0 / 0.8)",
  },
};

/** Calibrated to docs/assets/mockups (entrance, cinema, library). */
export const sideroom: ThemeDefinition = {
  id: "sideroom",
  name: "Side Room",
  family: "sideroom",
  scheme: "dark",
  tokens: {
    "--sr-bg-0": "#111110",
    "--sr-bg-1": "#161613",
    "--sr-bg-2": "#1c1d18",
    "--sr-bg-olive": "#23251d",
    "--sr-line": "#2a2b24",
    "--sr-text": "#ece6d6",
    "--sr-text-2": "#a9a493",
    "--sr-text-3": "#918d7e",
    "--sr-gold": "#c2b08f",
    "--sr-gold-ink": "#1b1a15",
    "--sr-olive-accent": "#8c9a6a",
    "--sr-font-display": '"Newsreader", "Source Serif 4", Georgia, serif',
    "--sr-font-body": '"Hanken Grotesk", "Inter", system-ui, sans-serif',
    "--sr-font-mono": '"IBM Plex Mono", ui-monospace, monospace',
    "--sr-radius": "2px",
    "--sr-eyebrow-size": "12px",
    "--sr-eyebrow-tracking": "0.28em",
    "--sr-eyebrow-rule": "40px",
    "--ui-bg": "#111110",
    "--ui-bg-raised": "#1c1d18",
    "--ui-bg-overlay": "#1c1d18",
    "--ui-bg-hover": "#23251d",
    "--ui-border": "#2a2b24",
    "--ui-text": "#ece6d6",
    "--ui-text-muted": "#a9a493",
    "--ui-text-faint": "#918d7e",
    "--ui-accent": "#c2b08f",
    "--ui-accent-hover": "#d1c1a2",
    "--ui-accent-ink": "#1b1a15",
    "--ui-danger": "#b4503f",
    "--ui-danger-ink": "#fff6f2",
    "--ui-success": "#8c9a6a",
    "--ui-warning": "#c2b08f",
    "--ui-info": "#9aa8a0",
    "--ui-focus": "#d9c9a4",
    "--ui-radius": "2px",
    "--ui-radius-lg": "4px",
    "--ui-font-body": '"Hanken Grotesk", "Inter", system-ui, sans-serif',
    "--ui-font-display": '"Newsreader", "Source Serif 4", Georgia, serif',
    "--ui-font-mono": '"IBM Plex Mono", ui-monospace, monospace',
    "--ui-shadow-overlay": "0 12px 32px rgb(0 0 0 / 0.55)",
    "--ui-backdrop": "rgb(8 8 7 / 0.78)",
    "--ui-status-online": "#8c9a6a",
    "--ui-status-idle": "#c2b08f",
    "--ui-status-dnd": "#b4503f",
    "--ui-status-offline": "#6f6c60",
  },
};

/** Live TV page: Side Room plus the teletext layer (docs/10 section 1). */
export const sideroomBroadcast: ThemeDefinition = {
  id: "sideroom-broadcast",
  name: "Side Room Broadcast",
  family: "sideroom",
  scheme: "dark",
  extends: "sideroom",
  tokens: {
    ...sideroom.tokens,
    "--sr-guide-cell": "#1f2219",
    "--sr-guide-cell-now": "#3a3d2d",
    "--sr-guide-channel": "#23251d",
    "--sr-guide-marker": "#c2b08f",
    "--sr-teletext-font": '"IBM Plex Mono", ui-monospace, monospace',
    "--sr-crt-frame": "#0a0a09",
    "--sr-crt-radius": "14px",
    "--sr-scanline":
      "repeating-linear-gradient(to bottom, rgb(0 0 0 / 0.18) 0, rgb(0 0 0 / 0.18) 1px, transparent 1px, transparent 3px)",
    "--sr-scanline-opacity": "1",
  },
};

export const THEMES: Readonly<Record<ThemeId, ThemeDefinition>> = {
  "lounge-dark": loungeDark,
  "lounge-light": loungeLight,
  "lounge-ash": loungeAsh,
  "lounge-onyx": loungeOnyx,
  sideroom,
  "sideroom-broadcast": sideroomBroadcast,
};

export const DEFAULT_THEME_BY_SECTION: Readonly<Record<Section, ThemeId>> = {
  lounge: "lounge-dark",
  staff: "lounge-dark",
  developer: "lounge-dark",
  entrance: "sideroom",
  cinema: "sideroom",
  library: "sideroom",
  support: "sideroom",
  "live-tv": "sideroom-broadcast",
};

export function getThemeTokens(id: ThemeId): ThemeTokens {
  return { ...THEMES[id].tokens } as ThemeTokens;
}

/** Serialize a token map as CSS declarations. */
export function tokensToCssText(tokens: Record<string, string | undefined>): string {
  return Object.entries(tokens)
    .filter((e): e is [string, string] => e[1] !== undefined)
    .map(([k, v]) => `${k}: ${v};`)
    .join("\n  ");
}

/** A global stylesheet defining every theme under `[data-theme="id"]`. */
export function buildThemesCss(): string {
  return (Object.keys(THEMES) as ThemeId[])
    .map((id) => {
      const def = THEMES[id];
      return `[data-theme="${id}"] {\n  color-scheme: ${def.scheme};\n  ${tokensToCssText(def.tokens)}\n}`;
    })
    .join("\n\n");
}
