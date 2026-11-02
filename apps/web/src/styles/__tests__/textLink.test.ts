import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { THEMES, type ThemeDefinition } from '@sideroom/ui';
import { describe, expect, it } from 'vitest';

/**
 * P30-VIS-1 (XC-VIS-003 S5, S6): a link in running text reads at WCAG AA (4.5:1) on every Lounge surface it can sit
 * on. The per-theme values are the `--text-link` token in packages/ui/src/tokens/themes.ts (registered in sr-theme,
 * so a custom theme sets it too); a Lounge theme that omits it falls back to its --brand (the fallback msg.css and
 * shell.css give the token). Each value is checked against the theme's --bg-base-low, --bg-base-lower and
 * --ui-bg-raised.
 */
const here = dirname(fileURLToPath(import.meta.url));

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

const linkOf = (theme: ThemeDefinition): string => {
  const tokens = theme.tokens as Record<string, string>;
  return tokens['--text-link'] ?? tokens['--brand']!;
};

const lounge = Object.values(THEMES).filter((t) => t.family === 'lounge');

describe('--text-link contrast (P30-VIS-1)', () => {
  it('finds the Lounge themes and gives each built-in theme its own link colour', () => {
    expect(lounge.length).toBeGreaterThanOrEqual(5);
    for (const t of Object.values(THEMES))
      expect((t.tokens as Record<string, string>)['--text-link'], t.id).toMatch(/^#[0-9a-f]{6}$/i);
  });

  it('keeps the values in the theme definitions, not in a stylesheet', () => {
    expect(existsSync(resolve(here, '../app-links.css'))).toBe(false);
  });

  it('gives the families their agreed values', () => {
    const v = (id: keyof typeof THEMES) => linkOf(THEMES[id]);
    expect(v('lounge-dark')).toBe('#00a8fc');
    expect(v('lounge-onyx')).toBe('#00a8fc');
    expect(v('lounge-ash')).toBe('#1ab2ff');
    expect(v('lounge-light')).toBe(THEMES['lounge-light'].tokens['--ui-accent-hover']);
    expect(v('lounge-editorial')).toBe(THEMES['lounge-editorial'].tokens['--brand']);
  });

  for (const theme of lounge) {
    it(`${theme.id}: links are at least 4.5:1 on its surfaces`, () => {
      const tokens = theme.tokens as Record<string, string>;
      const link = linkOf(theme);
      for (const surface of ['--bg-base-low', '--bg-base-lower', '--ui-bg-raised']) {
        const bg = tokens[surface]!;
        expect(contrast(link, bg), `${link} on ${surface} ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }
});
