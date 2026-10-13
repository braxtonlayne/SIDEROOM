/** Connection quality from RTCPeerConnection stats (VOX-UI-001/003 indicators). */
export type QualityLevel = 'unknown' | 'good' | 'fair' | 'poor';

export interface ConnectionQuality {
  level: QualityLevel;
  /** Round-trip time in ms, when known. */
  pingMs: number | null;
  /** Packets the far side reported lost in the last interval, percent. */
  lossPct: number | null;
}

export const UNKNOWN_QUALITY: ConnectionQuality = { level: 'unknown', pingMs: null, lossPct: null };

export function levelFor(pingMs: number | null, lossPct: number | null): QualityLevel {
  if (pingMs === null && lossPct === null) return 'unknown';
  const ping = pingMs ?? 0;
  const loss = lossPct ?? 0;
  if (ping >= 350 || loss >= 10) return 'poor';
  if (ping >= 160 || loss >= 3) return 'fair';
  return 'good';
}

/** Signal bars (0..3) for a level; `unknown` shows none. */
export const barsFor = (level: QualityLevel): number =>
  level === 'good' ? 3 : level === 'fair' ? 2 : level === 'poor' ? 1 : 0;

export const QUALITY_LABEL: Record<QualityLevel, string> = {
  unknown: 'Measuring connection',
  good: 'Good connection',
  fair: 'Fair connection',
  poor: 'Poor connection',
};

interface StatsLike {
  forEach(cb: (stat: Record<string, unknown>) => void): void;
}

const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);

/** Read RTT (selected candidate pair, else remote-inbound-rtp) and the far side's fraction lost. */
export function qualityFromStats(report: StatsLike): ConnectionQuality {
  let pairRtt: number | null = null;
  let remoteRtt: number | null = null;
  let loss: number | null = null;
  report.forEach((s) => {
    if (s.type === 'candidate-pair' && (s.state === 'succeeded' || s.nominated === true)) {
      const rtt = num(s.currentRoundTripTime);
      if (rtt !== null) pairRtt = rtt * 1000;
    } else if (s.type === 'remote-inbound-rtp' && s.kind !== 'video') {
      const rtt = num(s.roundTripTime);
      if (rtt !== null) remoteRtt = rtt * 1000;
      const lost = num(s.fractionLost);
      if (lost !== null) loss = Math.min(100, Math.max(0, lost * 100));
    }
  });
  const pingMs = pairRtt ?? remoteRtt;
  const rounded = pingMs === null ? null : Math.round(pingMs);
  return { level: levelFor(rounded, loss), pingMs: rounded, lossPct: loss };
}
