import { describe, expect, it } from 'vitest';
import {
  breakEven,
  costPerMbpsMonth,
  levelExpectedMonthly,
  levelPeakMbps,
  suggestedPrice,
  usableMbps,
} from './calculator';

// The worked numbers of docs/08 section 6.2 and 6.3.
describe('cost calculator', () => {
  const box = { serverCost: 105, uplinkMbps: 1000, utilisationHours: 40, floorPrice: 1 };

  it('uses 60% of the uplink and prices a Mbps for a month', () => {
    expect(usableMbps(1000)).toBe(600);
    expect(costPerMbpsMonth(box)).toBeCloseTo(0.175, 3);
    expect(costPerMbpsMonth({ ...box, uplinkMbps: 0 })).toBe(0);
  });

  it('computes the peak egress of a level', () => {
    // Level 2: 12 senders at 8 Mbps, 50 viewers: 50 * (8 + 11 * 0.15) = 482.5 Mbps.
    const l2 = { level: 2, viewersCap: 50, sendersCap: 12, maxBitrateMbps: 8 };
    expect(levelPeakMbps(l2)).toBeCloseTo(482.5, 1);
    // Free channel: 5 senders at 2.5 Mbps, 25 viewers: about 78 Mbps.
    expect(
      levelPeakMbps({ level: 0, viewersCap: 25, sendersCap: 5, maxBitrateMbps: 2.5 }),
    ).toBeCloseTo(77.5, 1);
  });

  it('suggests a price with the safety factor and never below the floor', () => {
    const l2 = { level: 2, viewersCap: 50, sendersCap: 12, maxBitrateMbps: 8 };
    // 482.5 * 40 / 720 * 0.175 = about $4.69 expected; times 3 is about $14.
    expect(levelExpectedMonthly(l2, box)).toBeCloseTo(4.69, 1);
    expect(suggestedPrice(l2, box)).toBeCloseTo(14.07, 1);
    const tiny = { level: 1, viewersCap: 1, sendersCap: 1, maxBitrateMbps: 0.1 };
    expect(suggestedPrice(tiny, box)).toBe(1);
  });

  it('computes break-even counts', () => {
    // Deployment S in doc 08: $128 a month, $5 Patron, $10/$25/$50 levels.
    expect(breakEven(128, 5, [10, 25, 50])).toEqual({ patrons: 26, levels: [13, 6, 3] });
    expect(breakEven(0, 5, [10])).toEqual({ patrons: 0, levels: [0] });
    expect(breakEven(128, 0, [0])).toEqual({ patrons: 0, levels: [0] });
  });
});
