import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { buildThemesCss, getThemeTokens, THEMES } from "../tokens/themes";
import { THEME_IDS, UI_TOKEN_NAMES } from "../tokens/types";
import { contrastRatio } from "../util/contrast";
import { fontFaceCss, fontPreloadLinks } from "./fonts";
import { appearanceVars, DEFAULT_APPEARANCE, resolveTheme, saturatedTokens, serverAccentTokens } from "./runtime";
import { ThemeProvider, useTheme } from "./ThemeProvider";

describe("tokens", () => {
  it.each(THEME_IDS)("%s defines all semantic tokens as custom properties", (id) => {
    const tokens = getThemeTokens(id);
    for (const name of UI_TOKEN_NAMES) expect(tokens[name], `${id} ${name}`).toBeTruthy();
    for (const k of Object.keys(tokens)) expect(k.startsWith("--")).toBe(true);
  });

  it.each(THEME_IDS)("%s meets WCAG AA for core text pairs", (id) => {
    const t = getThemeTokens(id);
    for (const bg of [t["--ui-bg"], t["--ui-bg-raised"]]) {
      expect(contrastRatio(t["--ui-text"], bg)).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(t["--ui-text-muted"], bg)).toBeGreaterThanOrEqual(4.5);
    }
    expect(contrastRatio(t["--ui-accent-ink"], t["--ui-accent"])).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(t["--ui-danger-ink"], t["--ui-danger"])).toBeGreaterThanOrEqual(4.5);
  });

  it("sideroom matches the documented palette", () => {
    const t = THEMES.sideroom.tokens;
    expect(t["--sr-bg-0"]).toBe("#111110");
    expect(t["--sr-gold"]).toBe("#c2b08f");
    expect(t["--sr-radius"]).toBe("2px");
  });

  it("broadcast extends sideroom with a teletext layer", () => {
    expect(THEMES["sideroom-broadcast"].tokens["--sr-bg-0"]).toBe("#111110");
    expect(THEMES["sideroom-broadcast"].tokens["--sr-teletext-font"]).toContain("Plex Mono");
  });

  it("emits a stylesheet for every theme", () => {
    const css = buildThemesCss();
    for (const id of THEME_IDS) expect(css).toContain(`[data-theme="${id}"]`);
  });
});

describe("resolveTheme", () => {
  it("uses the section default", () => {
    expect(resolveTheme({ section: "lounge" })).toBe("lounge-dark");
    expect(resolveTheme({ section: "cinema" })).toBe("sideroom");
    expect(resolveTheme({ section: "live-tv" })).toBe("sideroom-broadcast");
  });
  it("user global beats default; per-section beats global", () => {
    expect(resolveTheme({ section: "lounge", userGlobal: "sideroom" })).toBe("sideroom");
    expect(resolveTheme({ section: "lounge", userGlobal: "sideroom", perSection: { lounge: "lounge-onyx" } })).toBe(
      "lounge-onyx",
    );
  });
  it("ignores unknown ids", () => {
    expect(resolveTheme({ section: "lounge", userGlobal: "nope" as never })).toBe("lounge-dark");
  });
});

describe("serverAccentTokens", () => {
  it("recolors with a readable ink", () => {
    const t = serverAccentTokens("#ffeeaa");
    expect(t["--ui-accent"]).toBe("#ffeeaa");
    expect(t["--ui-accent-ink"]).toBe("#111110");
  });
  it("rejects non-hex values", () => {
    expect(serverAccentTokens("red; background:url(x)")).toEqual({});
    expect(serverAccentTokens(null)).toEqual({});
  });
});

describe("appearanceVars", () => {
  it("handles reduced motion, system and explicit", () => {
    expect(appearanceVars(DEFAULT_APPEARANCE, true)["--ui-reduced-motion"]).toBe("1");
    expect(appearanceVars(DEFAULT_APPEARANCE, false)["--ui-reduced-motion"]).toBe("0");
    expect(appearanceVars({ ...DEFAULT_APPEARANCE, reducedMotion: "on" }, false)["--ui-reduced-motion"]).toBe("1");
    expect(appearanceVars({ ...DEFAULT_APPEARANCE, reducedMotion: "on" }, false)["--sr-scanline-opacity"]).toBe("0");
  });
  it("clamps font size and saturation", () => {
    const v = appearanceVars({ ...DEFAULT_APPEARANCE, fontSize: 99, saturation: -5 }, false);
    expect(v["--ui-font-size-base"]).toBe("24px");
    expect(v["--ui-saturation"]).toBe("0");
  });
  it("boosts contrast", () => {
    const v = appearanceVars({ ...DEFAULT_APPEARANCE, contrast: "high" }, false);
    expect(v["--ui-text-muted"]).toBe("var(--ui-text)");
  });
});

describe("ThemeProvider", () => {
  function Probe() {
    const t = useTheme();
    return <span data-testid="probe">{`${t.theme}|${t.reducedMotion}`}</span>;
  }

  it("applies theme attributes, variables and context", () => {
    render(
      <ThemeProvider section="cinema" serverAccent="#336699" data-testid="root" appearance={{ reducedMotion: "on" }}>
        <Probe />
      </ThemeProvider>,
    );
    const root = screen.getByTestId("root");
    expect(root).toHaveAttribute("data-theme", "sideroom");
    expect(root.style.getPropertyValue("--sr-gold")).toBe("#c2b08f");
    expect(root.style.getPropertyValue("--ui-accent")).toBe("#336699");
    expect(screen.getByTestId("probe")).toHaveTextContent("sideroom|true");
  });

  it("ignores the server accent when the viewer disallows server theming", () => {
    render(<ThemeProvider section="lounge" serverAccent="#336699" allowServerTheming={false} data-testid="root" />);
    expect(screen.getByTestId("root").style.getPropertyValue("--ui-accent")).toBe("#5865f2");
  });

  it("throws outside a provider", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Probe />)).toThrow();
    spy.mockRestore();
  });
});

describe("fonts", () => {
  it("generates swap font faces from a base url", () => {
    const css = fontFaceCss("/assets/fonts");
    expect(css).toContain("font-display: swap");
    expect(css).toContain('url("/assets/fonts/newsreader-variable.woff2")');
    expect(css).toContain("Hanken Grotesk");
    expect(css).toContain("IBM Plex Mono");
  });
  it("lists preload links", () => {
    expect(fontPreloadLinks().every((l) => l.as === "font" && l.crossOrigin === "anonymous")).toBe(true);
  });
});

describe("saturation (Q35, Q61)", () => {
  it("passes tokens through at 100 and re-derives simple colours below 100", () => {
    const tokens = { "--ui-accent": "#5865f2", "--ui-radius": "8px", "--ui-accent-hover": "color-mix(in srgb, #5865f2 85%, black)" };
    expect(saturatedTokens(tokens, 100)).toBe(tokens);
    const out = saturatedTokens(tokens, 50);
    expect(out["--sat-base-ui-accent"]).toBe("#5865f2");
    expect(out["--ui-accent"]).toBe("oklch(from var(--sat-base-ui-accent) l calc(c * var(--ui-saturation)) h / alpha)");
    expect(out["--ui-radius"]).toBe("8px");
    expect(out["--ui-accent-hover"]).toBe(tokens["--ui-accent-hover"]);
  });

  it("never sets a CSS filter on the provider root", () => {
    const { container } = render(
      <ThemeProvider section="lounge" appearance={{ saturation: 40 }}>
        <span>x</span>
      </ThemeProvider>,
    );
    const root = container.firstElementChild as HTMLElement;
    expect(root.style.filter).toBe("");
    expect(root.style.getPropertyValue("--ui-accent")).toContain("oklch(from var(--sat-base-ui-accent)");
  });
});
