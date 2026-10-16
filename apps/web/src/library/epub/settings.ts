// EPUB reading settings (LIB-RDR-010): font, size, line height, margins, theme and flow. Kept on this device.
import { create } from 'zustand';

export type EpubTheme = 'light' | 'sepia' | 'dark';
export type EpubFlow = 'paginated' | 'scrolled';

export interface EpubSettings {
  /** '' keeps the book's own fonts. */
  font: '' | 'serif' | 'sans-serif' | 'monospace';
  /** Percent of the book's size, 80 to 200. */
  size: number;
  /** 1.2 to 2.2 */
  lineHeight: number;
  /** Side margin as a percent of the page width, 0 to 15. */
  margin: number;
  theme: EpubTheme;
  flow: EpubFlow;
}

export const DEFAULT_EPUB: EpubSettings = {
  font: '',
  size: 100,
  lineHeight: 1.6,
  margin: 6,
  theme: 'light',
  flow: 'paginated',
};

const KEY = 'sr.library.epub.v1';

const THEMES: Record<EpubTheme, { bg: string; fg: string }> = {
  light: { bg: '#ffffff', fg: '#1a1a1a' },
  sepia: { bg: '#f4ecd8', fg: '#433422' },
  dark: { bg: '#16161a', fg: '#d8d8dc' },
};

export const themeColors = (t: EpubTheme) => THEMES[t];

const clampN = (n: unknown, lo: number, hi: number, d: number) =>
  typeof n === 'number' && Number.isFinite(n) ? Math.min(Math.max(n, lo), hi) : d;

/** Reads stored settings, dropping anything out of range. */
export function normalize(raw: unknown): EpubSettings {
  const r = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  const font = ['', 'serif', 'sans-serif', 'monospace'].includes(r.font as string)
    ? (r.font as EpubSettings['font'])
    : DEFAULT_EPUB.font;
  return {
    font,
    size: clampN(r.size, 80, 200, DEFAULT_EPUB.size),
    lineHeight: clampN(r.lineHeight, 1.2, 2.2, DEFAULT_EPUB.lineHeight),
    margin: clampN(r.margin, 0, 15, DEFAULT_EPUB.margin),
    theme: r.theme === 'sepia' || r.theme === 'dark' ? r.theme : 'light',
    flow: r.flow === 'scrolled' ? 'scrolled' : 'paginated',
  };
}

/** The stylesheet injected into every section of the book. */
export function epubCss(s: EpubSettings): string {
  const c = themeColors(s.theme);
  return `
    html { color-scheme: ${s.theme === 'dark' ? 'dark' : 'light'}; }
    html, body { background: ${c.bg} !important; color: ${c.fg} !important; }
    body { ${s.font ? `font-family: ${s.font} !important;` : ''} line-height: ${s.lineHeight} !important; }
    p, li, blockquote, dd { line-height: ${s.lineHeight} !important; }
    a { color: inherit; }
    html { font-size: ${s.size}% !important; }
  `;
}

function load(): EpubSettings {
  try {
    return normalize(JSON.parse(localStorage.getItem(KEY) ?? '{}'));
  } catch {
    return DEFAULT_EPUB;
  }
}

interface State {
  settings: EpubSettings;
  set(patch: Partial<EpubSettings>): void;
}

export const useEpubSettings = create<State>((set, get) => ({
  settings: load(),
  set(patch) {
    const next = normalize({ ...get().settings, ...patch });
    try {
      localStorage.setItem(KEY, JSON.stringify(next));
    } catch {
      /* best effort */
    }
    set({ settings: next });
  },
}));
