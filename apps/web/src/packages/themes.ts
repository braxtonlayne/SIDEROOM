// Store themes (EXT-PKG-005): the themes a person turned on from the Theme Store. A theme is only a map of
// `--ui-*` design tokens plus a colour scheme (it never carries CSS, D-063). The tokens are applied as inline
// custom properties on the ThemeProvider root, after the built-in theme, so a store theme only has to name
// what it changes. The server validated every value; the client checks again before it touches the DOM.
import { useMemo, type CSSProperties } from 'react';
import { create } from 'zustand';
import { myEnabled, type Enablement } from './api';

export interface StoreTheme {
  packageId: string;
  name: string;
  /** `all` or the section (`lounge`, `cinema`, `library`, `entrance`) it applies to. */
  section: string;
  scheme: 'light' | 'dark';
  tokens: Record<string, string>;
}

const KEY = /^--ui-[a-z0-9-]{1,48}$/;
// Hex colours, functional colours, lengths and plain words; nothing that can load, nest or end a declaration.
const VALUE = /^[#a-zA-Z0-9(),.%\s/+-]{1,160}$/;
const BANNED = /url\s*\(|expression|import|javascript|var\s*\(/i;

/** The tokens that are safe to set, whatever the payload says. */
export function safeTokens(tokens: unknown): Record<string, string> {
  const out: Record<string, string> = {};
  if (!tokens || typeof tokens !== 'object') return out;
  for (const [k, v] of Object.entries(tokens as Record<string, unknown>)) {
    if (KEY.test(k) && typeof v === 'string' && VALUE.test(v) && !BANNED.test(v)) out[k] = v.trim();
  }
  return out;
}

/** A store theme from a package payload, or null when it is not a theme. */
export function themeOf(e: {
  package_id?: string;
  id?: string;
  name: string;
  section?: string;
  payload: Record<string, unknown> | null;
}): StoreTheme | null {
  const p = e.payload;
  if (!p) return null;
  const scheme = p.scheme === 'light' ? 'light' : 'dark';
  const tokens = safeTokens(p.tokens);
  return {
    packageId: String(e.package_id ?? e.id ?? ''),
    name: e.name,
    section: e.section ?? 'all',
    scheme,
    tokens,
  };
}

export function themeStyle(t: StoreTheme | null | undefined): CSSProperties {
  if (!t) return {};
  return { ...t.tokens, colorScheme: t.scheme } as CSSProperties;
}

interface State {
  themes: StoreTheme[];
  /** A theme being tried out in the Theme Store; wins over everything while set. */
  preview: StoreTheme | null;
  loaded: boolean;
  load: () => Promise<void>;
  setThemes: (list: Enablement[]) => void;
  setPreview: (t: StoreTheme | null) => void;
}

export const useStoreThemes = create<State>()((set, get) => ({
  themes: [],
  preview: null,
  loaded: false,
  load: async () => {
    if (get().loaded) return;
    set({ loaded: true });
    try {
      get().setThemes(await myEnabled());
    } catch {
      /* not signed in, or the server has no packages: keep the built-in themes */
    }
  },
  setThemes: (list) =>
    set({
      themes: list
        .filter((e) => e.kind === 'theme')
        .map((e) => themeOf({ ...e, id: e.package_id }))
        .filter((t): t is StoreTheme => t !== null),
    }),
  setPreview: (preview) => set({ preview }),
}));

/** The inline style of the store theme in effect for a section: the preview, else that section's, else the global one. */
export function useStoreThemeStyle(section: string): CSSProperties {
  const themes = useStoreThemes((s) => s.themes);
  const preview = useStoreThemes((s) => s.preview);
  return useMemo(
    () =>
      themeStyle(
        preview ??
          themes.find((x) => x.section === section) ??
          themes.find((x) => x.section === 'all'),
      ),
    [preview, themes, section],
  );
}
