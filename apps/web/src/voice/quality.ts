import { t } from '../i18n';
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

/** Labels resolve when read, so they follow the active language. */
export const QUALITY_LABEL: Record<QualityLevel, string> = {
  get unknown() {
    return t('voice.quality.unknown');
  },
  get good() {
    return t('voice.quality.good');
  },
  get fair() {
    return t('voice.quality.fair');
  },
  get poor() {
    return t('voice.quality.poor');
  },
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

// ---- remote participants (VOX-UI-003) ----
//
// A remote person's tile reads the inbound audio stats of the receiver that carries their microphone line (the
// SFU labels each forwarded line with the speaker's user id, so the engine already knows whose receiver it is):
// no protocol addition is needed. Loss and bitrate come from the change since the previous reading, jitter from
// the receiver's own running estimate.

/** One inbound-rtp reading (cumulative counters, as the browser reports them). */
export interface InboundSample {
  packets: number;
  lost: number;
  bytes: number;
  jitterMs: number | null;
  /** Reading time in ms (a monotonic clock). */
  at: number;
}

/** What a remote person's tile shows. */
export interface RemoteQuality {
  level: QualityLevel;
  /** Share of the packets since the last reading that never arrived, percent. */
  lossPct: number | null;
  jitterMs: number | null;
  /** Received bitrate since the last reading, kbit/s. */
  kbps: number | null;
}

export const UNKNOWN_REMOTE: RemoteQuality = {
  level: 'unknown',
  lossPct: null,
  jitterMs: null,
  kbps: null,
};

/** The audio `inbound-rtp` entry of a receiver's stats report, or null when there is none yet. */
export function inboundSample(report: StatsLike, at: number): InboundSample | null {
  let out: InboundSample | null = null;
  report.forEach((s) => {
    if (s.type !== 'inbound-rtp' || (s.kind ?? s.mediaType) === 'video') return;
    const packets = num(s.packetsReceived);
    if (packets === null) return;
    const jitter = num(s.jitter);
    out = {
      packets,
      lost: Math.max(0, num(s.packetsLost) ?? 0),
      bytes: num(s.bytesReceived) ?? 0,
      jitterMs: jitter === null ? null : Math.round(jitter * 1000),
      at,
    };
  });
  return out;
}

/** Level of a remote line: heavy loss or jitter is poor, moderate is fair. */
export function remoteLevel(lossPct: number | null, jitterMs: number | null): QualityLevel {
  if (lossPct === null && jitterMs === null) return 'unknown';
  const loss = lossPct ?? 0;
  const jitter = jitterMs ?? 0;
  if (loss >= 10 || jitter >= 80) return 'poor';
  if (loss >= 3 || jitter >= 30) return 'fair';
  return 'good';
}

/**
 * Quality from two readings. A pause in the stream (the far side is muted and Opus stops sending) gives no
 * new packets: the previous verdict stands instead of flapping to "unknown".
 */
export function remoteQuality(
  prev: InboundSample | null,
  cur: InboundSample,
  before: RemoteQuality = UNKNOWN_REMOTE,
): RemoteQuality {
  if (!prev)
    return { ...UNKNOWN_REMOTE, jitterMs: cur.jitterMs, level: remoteLevel(null, cur.jitterMs) };
  const got = cur.packets - prev.packets;
  const lost = cur.lost - prev.lost;
  if (got <= 0 && lost <= 0) return before;
  const lossPct =
    got + lost > 0 ? Math.min(100, Math.max(0, (Math.max(0, lost) / (got + lost)) * 100)) : null;
  const secs = (cur.at - prev.at) / 1000;
  const kbps = secs > 0 ? Math.round(((cur.bytes - prev.bytes) * 8) / secs / 1000) : null;
  const rounded = lossPct === null ? null : Math.round(lossPct * 10) / 10;
  return {
    level: remoteLevel(rounded, cur.jitterMs),
    lossPct: rounded,
    jitterMs: cur.jitterMs,
    kbps,
  };
}
