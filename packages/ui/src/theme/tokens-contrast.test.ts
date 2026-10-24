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

  it.each(ids)("%s: focus ring and status colours reach 3:1 on the base surface", (id) => {
    const t = effective(id, false);
    const bg = t["--ui-bg"]!;
    for (const name of ["--ui-focus", "--ui-status-online", "--ui-status-idle", "--ui-status-dnd"])
      if (has(t, name)) expect(contrastRatio(t[name]!, bg), name).toBeGreaterThanOrEqual(3);
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
