import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { THEMES, type ThemeDefinition } from '@sideroom/ui';
import { describe, expect, it } from 'vitest';

/**
 * P30-VIS-1 (XC-VIS-003 S5): a link in running text reads at WCAG AA (4.5:1) on every Lounge surface it can sit on.
 * The per-theme values live in styles/app-links.css; a Lounge theme without a rule falls back to its --brand (the
 * fallback msg.css and shell.css give the token). Each value is checked against the theme's --bg-base-low,
 * --bg-base-lower and --ui-bg-raised as themes.ts defines them.
 */
const here = dirname(fileURLToPath(import.meta.url));
const css = readFileSync(resolve(here, '../app-links.css'), 'utf8');

/** `--text-link` per theme id, from the `[data-theme='…'] { --text-link: … }` rules (selector lists included). */
function linkRules(source: string): Map<string, string> {
  const out = new Map<string, string>();
  const stripped = source.replace(/\/\*[\s\S]*?\*\//g, '');
  for (const m of stripped.matchAll(/([^{}]+)\{([^}]*)\}/g)) {
    const value = /--text-link:\s*([^;]+);/.exec(m[2]!)?.[1]?.trim();
    if (!value) continue;
    for (const id of m[1]!.matchAll(/\[data-theme='([^']+)'\]/g)) out.set(id[1]!, value);
  }
  return out;
}

function resolveValue(value: string, theme: ThemeDefinition): string {
  const ref = /^var\((--[\w-]+)\)$/.exec(value);
  if (!ref) return value;
  const tokens = theme.tokens as Record<string, string>;
  const v = tokens[ref[1]!];
  if (!v) throw new Error(`${theme.id}: ${ref[1]} is not a token`);
  return v;
}

function luminance(hex: string): number {
  const m = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!m) throw new Error(`not a #rrggbb colour: ${hex}`);
  const n = parseInt(m[1]!, 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi! + 0.05) / (lo! + 0.05);
}

const rules = linkRules(css);
const lounge = Object.values(THEMES).filter((t) => t.family === 'lounge');

describe('--text-link contrast (P30-VIS-1)', () => {
  it('finds the Lounge themes and the rules', () => {
    expect(lounge.length).toBeGreaterThanOrEqual(5);
    for (const id of ['lounge-light', 'lounge-dark', 'lounge-onyx', 'lounge-ash'])
      expect(rules.has(id)).toBe(true);
  });

  for (const theme of lounge) {
    it(`${theme.id}: links are at least 4.5:1 on its surfaces`, () => {
      const tokens = theme.tokens as Record<string, string>;
      const link = resolveValue(rules.get(theme.id) ?? 'var(--brand)', theme);
      for (const surface of ['--bg-base-low', '--bg-base-lower', '--ui-bg-raised']) {
        const bg = tokens[surface]!;
        expect(contrast(link, bg), `${link} on ${surface} ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }
});
