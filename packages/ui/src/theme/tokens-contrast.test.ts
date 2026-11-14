import { describe, expect, it } from "vitest";
import { DEFAULT_THEME_BY_SECTION, THEMES } from "../tokens/themes";
import type { ThemeId } from "../tokens/types";
import { contrastRatio, isHexColor } from "../util/contrast";
import { appearanceVars, DEFAULT_APPEARANCE } from "./runtime";

/**
 * RDY-A11Y-004: walks the token set. For every theme (and so for every section, which each use one of them) the
 * text and surface tokens must meet WCAG AA in the normal setting, and the High contrast setting must lift every
 * text token the Lounge, Cinema and Library CSS reads to the full-strength text colour (7:1, AAA) and the
 * borders to a visible line.
 */

type Tokens = Record<string, string>;

/** The effective token map: theme tokens, then the appearance variables on top, `var()` references resolved. */
function effective(id: ThemeId, high: boolean): Tokens {
  const merged: Tokens = { ...(THEMES[id].tokens as Tokens) };
  Object.assign(merged, appearanceVars({ ...DEFAULT_APPEARANCE, contrast: high ? "high" : "normal" }, false));
  const resolve = (value: string, depth = 0): string => {
    const m = /^var\((--[a-z0-9-]+)(?:,\s*(.+))?\)$/i.exec(value.trim());
    if (!m || depth > 8) return value;
    const target = merged[m[1]!];
    return resolve(target ?? m[2] ?? value, depth + 1);
  };
  return Object.fromEntries(Object.entries(merged).map(([k, v]) => [k, resolve(v)]));
}

const has = (t: Tokens, name: string) => name in t && isHexColor(t[name]!);

/** Foreground and background token names to check, per family. Names missing from a theme are skipped. */
const SURFACES = ["--ui-bg", "--ui-bg-raised", "--ui-bg-overlay"];
const TEXTS = ["--ui-text", "--ui-text-muted", "--ui-text-faint"];
const LOUNGE_SURFACES = ["--bg-base-lowest", "--bg-base-lower", "--bg-base-low", "--bg-surface-overlay"];
const LOUNGE_TEXTS = ["--text-normal", "--text-muted", "--interactive-normal"];
const SR_SURFACES = ["--sr-bg-0", "--sr-bg-1", "--sr-bg-2"];
const SR_TEXTS = ["--sr-text", "--sr-text-2", "--sr-text-3"];

function pairs(t: Tokens, fgs: string[], bgs: string[]): [string, string][] {
  return fgs.flatMap((f) => bgs.map((b): [string, string] => [f, b])).filter(([f, b]) => has(t, f) && has(t, b));
}

/** Hue (degrees) and HSL saturation (0..1) of a `#rrggbb` colour. */
function hsl(hex: string): { hue: number; saturation: number } {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255) as [number, number, number];
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  if (d === 0) return { hue: 0, saturation: 0 };
  const saturation = d / (1 - Math.abs(max + min - 1));
  const h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return { hue: (h * 60 + 360) % 360, saturation };
}

const ids = Object.keys(THEMES) as ThemeId[];

describe("token set: normal contrast", () => {
  it.each(ids)("%s: text tokens reach 4.5:1 on their surfaces", (id) => {
    const t = effective(id, false);
    const failures: string[] = [];
    const all = [
      ...pairs(t, TEXTS, SURFACES),
      ...pairs(t, LOUNGE_TEXTS, LOUNGE_SURFACES),
      ...pairs(t, SR_TEXTS, SR_SURFACES),
    ];
    expect(all.length).toBeGreaterThan(0);
    for (const [f, b] of all) {
      const r = contrastRatio(t[f]!, t[b]!);
      if (r < 4.5) failures.push(`${f} ${t[f]} on ${b} ${t[b]} = ${r.toFixed(2)}`);
    }
    expect(failures).toEqual([]);
  });

  it.each(ids)("%s: ink on accent and danger fills reaches 4.5:1", (id) => {
    const t = effective(id, false);
    for (const [fg, bg] of [
      ["--ui-accent-ink", "--ui-accent"],
      ["--ui-accent-ink", "--ui-accent-hover"],
      ["--ui-danger-ink", "--ui-danger"],
    ] as const)
      if (has(t, fg) && has(t, bg)) expect(contrastRatio(t[fg]!, t[bg]!), `${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
  });

  it.each(ids)("%s: danger text reaches 4.5:1 on every surface, in both contrast settings (D-577)", (id) => {
    for (const high of [false, true]) {
      const t = effective(id, high);
      expect(has(t, "--ui-danger-text"), "--ui-danger-text defined").toBe(true);
      const all = [...pairs(t, ["--ui-danger-text"], SURFACES), ...pairs(t, ["--ui-danger-text"], LOUNGE_SURFACES)];
      expect(all.length).toBeGreaterThanOrEqual(2);
      for (const [f, b] of all)
        expect(contrastRatio(t[f]!, t[b]!), `${high ? "high" : "normal"}: ${f} on ${b} ${t[b]}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it.each(ids)("%s: focus ring and status colours reach 3:1 on the base surface", (id) => {
    const t = effective(id, false);
    const bg = t["--ui-bg"]!;
    for (const name of ["--ui-focus", "--ui-status-online", "--ui-status-idle", "--ui-status-dnd"])
      if (has(t, name)) expect(contrastRatio(t[name]!, bg), name).toBeGreaterThanOrEqual(3);
  });

  // The Editorial look (and its Cinema, Library and TV sibling); the Lounge themes keep their own values.
  it.each(ids.filter((id) => id === "lounge-editorial" || id.startsWith("sideroom")))("%s: the status colours reach 3:1 on every surface a status sits on (D-741, D-756)", (id) => {
    const t = effective(id, false);
    for (const name of ["--ui-status-online", "--ui-status-idle", "--ui-status-dnd"])
      if (has(t, name))
        for (const surface of [...SURFACES, ...LOUNGE_SURFACES])
          if (has(t, surface)) expect(contrastRatio(t[name]!, t[surface]!), `${name} on ${surface} ${t[surface]}`).toBeGreaterThanOrEqual(3);
  });

  // D-756: 3:1 was met by the olive and missed by eye. Online is a clear green: a green hue, and saturated enough
  // to read as a colour at 13 px beside the warm browns (a pale or greyish green fails).
  it.each(ids)("%s: the online colour is a clear, saturated green (D-756)", (id) => {
    const t = effective(id, false);
    if (!has(t, "--ui-status-online")) return;
    const { hue, saturation } = hsl(t["--ui-status-online"]!);
    expect(hue, "hue").toBeGreaterThanOrEqual(100);
    expect(hue, "hue").toBeLessThanOrEqual(165);
    expect(saturation, "saturation").toBeGreaterThanOrEqual(0.5);
  });
});

describe("token set: high contrast", () => {
  it("covers every section with a theme from the set", () => {
    for (const theme of Object.values(DEFAULT_THEME_BY_SECTION)) expect(THEMES[theme]).toBeDefined();
  });

  it.each(ids)("%s: every text token resolves to the full-strength text colour (7:1)", (id) => {
    const t = effective(id, true);
    const failures: string[] = [];
    const all = [
      ...pairs(t, TEXTS, SURFACES),
      ...pairs(t, LOUNGE_TEXTS, LOUNGE_SURFACES),
      ...pairs(t, SR_TEXTS, SR_SURFACES),
    ];
    for (const [f, b] of all) {
      const r = contrastRatio(t[f]!, t[b]!);
      if (r < 7) failures.push(`${f} ${t[f]} on ${b} ${t[b]} = ${r.toFixed(2)}`);
    }
    expect(failures).toEqual([]);
  });

  it.each(ids)("%s: borders and hairlines are a visible 3:1 line", (id) => {
    const t = effective(id, true);
    for (const name of ["--ui-border", "--sr-line"])
      if (has(t, name) && has(t, "--ui-bg")) expect(contrastRatio(t[name]!, t["--ui-bg"]!), name).toBeGreaterThanOrEqual(3);
  });

  it.each(ids)("%s: the setting only ever raises contrast", (id) => {
    const normal = effective(id, false);
    const high = effective(id, true);
    for (const [f, b] of pairs(normal, [...TEXTS, ...LOUNGE_TEXTS, ...SR_TEXTS], [...SURFACES, ...LOUNGE_SURFACES, ...SR_SURFACES]))
      expect(contrastRatio(high[f]!, high[b]!), `${f} on ${b}`).toBeGreaterThanOrEqual(contrastRatio(normal[f]!, normal[b]!) - 0.01);
  });
});
