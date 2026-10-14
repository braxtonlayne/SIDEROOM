// The cost calculator of docs/08 section 6.4. Pure functions; the Pricing screen feeds them the operator's
// real hardware costs and shows the suggested prices next to the configured ones.

export const THUMB_MBPS = 0.15;
export const SAFETY_FACTOR = 3;

export interface CostInputs {
  /** Monthly cost of the server (USD). */
  serverCost: number;
  uplinkMbps: number;
  /** Hours a month a level's worst-case load is expected to run. */
  utilisationHours: number;
  /** Lowest price worth charging (USD). */
  floorPrice: number;
}

export interface LevelShape {
  level: number;
  viewersCap: number;
  sendersCap: number;
  /** Bitrate of one full-quality stream at this level (Mbps). */
  maxBitrateMbps: number;
}

export const usableMbps = (uplinkMbps: number) => uplinkMbps * 0.6;

export const costPerMbpsMonth = (c: CostInputs) =>
  usableMbps(c.uplinkMbps) > 0 ? c.serverCost / usableMbps(c.uplinkMbps) : 0;

export const levelPeakMbps = (l: LevelShape) =>
  l.viewersCap * (l.maxBitrateMbps + (l.sendersCap - 1) * THUMB_MBPS);

export const levelExpectedMonthly = (l: LevelShape, c: CostInputs) =>
  ((levelPeakMbps(l) * c.utilisationHours) / 720) * costPerMbpsMonth(c);

export const suggestedPrice = (l: LevelShape, c: CostInputs) =>
  Math.max(levelExpectedMonthly(l, c) * SAFETY_FACTOR, c.floorPrice);

/** How many of each supporter it takes to cover the server cost. */
export function breakEven(serverCost: number, patronPrice: number, thresholds: number[]) {
  const ceil = (n: number) => (n > 0 && Number.isFinite(n) ? Math.ceil(n) : 0);
  return {
    patrons: ceil(serverCost / patronPrice),
    levels: thresholds.map((t) => ceil(serverCost / t)),
  };
}
