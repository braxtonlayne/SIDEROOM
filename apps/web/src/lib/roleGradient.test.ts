import { describe, expect, it } from 'vitest';
import { HOLOGRAPHIC, gradientOf, nameStyle, stopsOf } from './roleGradient';

describe('role gradients (J-08)', () => {
  it('the holographic preset is the triple the server enforces (K-02)', () => {
    // Mirrors sr_chat::role_rules::HOLOGRAPHIC; the server refuses any other tertiary stop.
    expect(HOLOGRAPHIC).toEqual({
      primary: 11_127_295,
      secondary: 16_759_788,
      tertiary: 16_761_760,
    });
  });

  it('builds the stops from the colors object, first stop first', () => {
    expect(stopsOf({ primary_color: 0xff0000 })).toEqual(['#ff0000']);
    expect(stopsOf({ primary_color: 0xff0000, secondary_color: 0x0000ff })).toEqual([
      '#ff0000',
      '#0000ff',
    ]);
    expect(
      stopsOf({
        primary_color: HOLOGRAPHIC.primary,
        secondary_color: HOLOGRAPHIC.secondary,
        tertiary_color: HOLOGRAPHIC.tertiary,
      }),
    ).toEqual(['#a9c9ff', '#ffbbec', '#ffc3a0']);
    // No colour at all means no stops, whatever else is set; a black second stop is a real stop.
    expect(stopsOf({ primary_color: 0, secondary_color: 5 })).toEqual([]);
    expect(stopsOf({ primary_color: 1, secondary_color: 0 })).toEqual(['#000001', '#000000']);
    // A tertiary stop alone does not count.
    expect(stopsOf({ primary_color: 1, tertiary_color: 5 })).toEqual(['#000001']);
    expect(stopsOf(undefined, 0x00ff00)).toEqual(['#00ff00']);
  });

  it('paints plain colour first and adds the gradient only for two or more stops', () => {
    expect(nameStyle([])).toEqual({});
    expect(nameStyle(['#111111'])).toEqual({ color: '#111111' });
    expect(gradientOf(['#111111'])).toBeUndefined();
    expect(nameStyle(['#111111', '#222222', '#333333'])).toEqual({
      color: '#111111',
      backgroundImage: 'linear-gradient(90deg, #111111, #222222, #333333)',
    });
  });
});
