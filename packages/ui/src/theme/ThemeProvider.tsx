import { createContext, useContext, useEffect, useMemo, useState, type CSSProperties, type HTMLAttributes } from "react";
import { THEMES } from "../tokens/themes";
import type { Section, ThemeId } from "../tokens/types";
import {
  DEFAULT_APPEARANCE,
  appearanceVars,
  prefersReducedMotion,
  resolveTheme,
  serverAccentTokens,
  type AppearanceSettings,
} from "./runtime";

export interface ThemeContextValue {
  theme: ThemeId;
  section: Section;
  appearance: AppearanceSettings;
  reducedMotion: boolean;
  /** Element overlays portal into so they inherit the theme variables. */
  portalRoot: HTMLElement | null;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

/** Where overlays (modals, toasts) should portal: the nearest theme root, else document.body. */
export function usePortalTarget(): HTMLElement | null {
  const ctx = useContext(ThemeContext);
  if (ctx?.portalRoot) return ctx.portalRoot;
  return typeof document === "undefined" ? null : document.body;
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used inside <ThemeProvider>");
  return ctx;
}

export interface ThemeProviderProps extends Omit<HTMLAttributes<HTMLDivElement>, "style"> {
  section: Section;
  userGlobal?: ThemeId | null;
  perSection?: Partial<Record<Section, ThemeId>>;
  platformDefault?: ThemeId;
  /** Server accent hex; ignored unless `allowServerTheming` (default true). */
  serverAccent?: string | null;
  allowServerTheming?: boolean;
  appearance?: Partial<AppearanceSettings>;
  style?: CSSProperties;
}

function useSystemReducedMotion(): boolean {
  const [value, setValue] = useState(prefersReducedMotion);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const onChange = () => setValue(mq.matches);
    mq.addEventListener?.("change", onChange);
    return () => mq.removeEventListener?.("change", onChange);
  }, []);
  return value;
}

export function ThemeProvider({
  section,
  userGlobal,
  perSection,
  platformDefault,
  serverAccent,
  allowServerTheming = true,
  appearance,
  style,
  children,
  ...rest
}: ThemeProviderProps) {
  const systemReduced = useSystemReducedMotion();
  const [portalRoot, setPortalRoot] = useState<HTMLElement | null>(null);
  const theme = resolveTheme({ section, userGlobal, perSection, platformDefault });
  const merged = useMemo<AppearanceSettings>(() => ({ ...DEFAULT_APPEARANCE, ...appearance }), [appearance]);
  const reducedMotion = merged.reducedMotion === "on" || (merged.reducedMotion === "system" && systemReduced);

  const vars = useMemo(
    () => ({
      ...THEMES[theme].tokens,
      ...(allowServerTheming ? serverAccentTokens(serverAccent) : {}),
      ...appearanceVars(merged, systemReduced),
    }),
    [theme, serverAccent, allowServerTheming, merged, systemReduced],
  );

  const ctx = useMemo<ThemeContextValue>(
    () => ({ theme, section, appearance: merged, reducedMotion, portalRoot }),
    [theme, section, merged, reducedMotion, portalRoot],
  );

  const rootStyle = {
    ...vars,
    colorScheme: THEMES[theme].scheme,
    background: "var(--ui-bg)",
    color: "var(--ui-text)",
    fontFamily: "var(--ui-font-body)",
    fontSize: "var(--ui-font-size-base)",
    filter: merged.saturation === 100 ? undefined : "saturate(var(--ui-saturation))",
    ...style,
  } as CSSProperties;

  return (
    <ThemeContext.Provider value={ctx}>
      <div
        {...rest}
        ref={setPortalRoot}
        data-theme={theme}
        data-section={section}
        data-reduced-motion={reducedMotion ? "true" : "false"}
        data-contrast={merged.contrast}
        style={rootStyle}
      >
        {children}
      </div>
    </ThemeContext.Provider>
  );
}
