// Role colour gradients (J-08, LNG-SSET-032): the stops of a role and how a name is painted with them.

/** The fixed triple Discord uses for its "holographic" preset: primary, secondary, tertiary. */
export const HOLOGRAPHIC = {
  primary: 11_127_295,
  secondary: 16_759_788,
  tertiary: 16_761_760,
} as const;

/** The stops a role carries on the wire (`colors`), as integers or absent. */
export interface RoleStops {
  primary_color?: number | null;
  secondary_color?: number | null;
  tertiary_color?: number | null;
}

const hex = (n: number) => `#${(n & 0xffffff).toString(16).padStart(6, '0')}`;

/**
 * The CSS colours of a role, first stop first. A zero primary colour means "no colour", so a role without one
 * has no stops even if a later stop is set; a stop of 0 after the first is a real black.
 */
export function stopsOf(c: RoleStops | undefined | null, color?: number): string[] {
  const primary = c?.primary_color ?? color ?? 0;
  if (!primary) return [];
  const out = [hex(primary)];
  if (c?.secondary_color != null) {
    out.push(hex(c.secondary_color));
    if (c.tertiary_color != null) out.push(hex(c.tertiary_color));
  }
  return out;
}

/** `linear-gradient(...)` over two or three stops, or undefined when the role is one plain colour. */
export function gradientOf(stops: readonly string[]): string | undefined {
  return stops.length > 1 ? `linear-gradient(90deg, ${stops.join(', ')})` : undefined;
}

/**
 * Inline style for a name: the first stop as plain `color` (what a browser without `background-clip: text` shows)
 * and, with two or more stops, the gradient as `backgroundImage`; the `[data-gradient]` rule in the stylesheet
 * turns the text transparent only where `background-clip: text` works.
 */
export function nameStyle(stops: readonly string[]): { color?: string; backgroundImage?: string } {
  const first = stops[0];
  if (!first) return {};
  const g = gradientOf(stops);
  return g ? { color: first, backgroundImage: g } : { color: first };
}
