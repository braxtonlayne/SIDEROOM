/**
 * Profile theme colors (LNG-PRF-010): two RGB integers become a vertical gradient behind the popout, the full
 * profile and the DM panel. The text color flips to dark on a light gradient so contrast holds.
 */
import type { CSSProperties } from 'react';

const valid = (n: unknown): n is number =>
  typeof n === 'number' && Number.isInteger(n) && n >= 0 && n <= 0xffffff;

export const hexOf = (n: number): string => `#${n.toString(16).padStart(6, '0')}`;

/** WCAG relative luminance of an RGB integer. */
export function luminance(n: number): number {
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin((n >> 16) & 255) + 0.7152 * lin((n >> 8) & 255) + 0.0722 * lin(n & 255);
}

export interface ProfileTheme {
  style: CSSProperties;
  tone: 'light' | 'dark';
}

/**
 * The theme for a profile, or null when it has none (then the surface keeps the app colors).
 * `colors` is `[start, end]`; a lone `accent` makes a flat theme.
 */
export function profileTheme(
  colors: readonly [number, number] | null | undefined,
  accent?: number | null,
): ProfileTheme | null {
  let a: number;
  let b: number;
  if (colors && valid(colors[0]) && valid(colors[1])) [a, b] = colors;
  else if (valid(accent) && accent > 0) a = b = accent;
  else return null;
  const mid = luminance(a) / 2 + luminance(b) / 2;
  // Black text on a surface with luminance above 0.4 is well over 4.5:1; white text below it likewise.
  const tone = mid > 0.4 ? 'light' : 'dark';
  const vars: Record<string, string> = {
    '--prf-start': hexOf(a),
    '--prf-end': hexOf(b),
    '--prf-bg': `linear-gradient(180deg, ${hexOf(a)}, ${hexOf(b)})`,
    '--text-normal': tone === 'light' ? '#111' : '#fff',
    '--text-muted': tone === 'light' ? '#333' : '#e6e6e6',
  };
  return { style: vars as CSSProperties, tone };
}
