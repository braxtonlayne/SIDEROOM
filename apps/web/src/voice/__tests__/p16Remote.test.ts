// Per-tile connection quality of remote people (VOX-UI-003): inbound stats of the receiver that carries their
// microphone line, mapped to the person by the msid stream id the SFU already sets.
import { describe, expect, it } from 'vitest';
import { inboundSample, remoteLevel, remoteQuality } from '../quality';

const sample = (
  over: Partial<{
    packets: number;
    lost: number;
    bytes: number;
    jitterMs: number;
    at: number;
  }> = {},
) => ({ packets: 100, lost: 0, bytes: 8000, jitterMs: 5, at: 0, ...over });

describe('remote quality maths', () => {
  it('reads the audio inbound-rtp entry', () => {
    const rows: Record<string, unknown>[] = [
      { type: 'inbound-rtp', kind: 'video', packetsReceived: 9 },
      {
        type: 'inbound-rtp',
        kind: 'audio',
        packetsReceived: 50,
        packetsLost: 2,
        bytesReceived: 4000,
        jitter: 0.012,
      },
    ];
    expect(inboundSample({ forEach: (cb) => rows.forEach(cb) }, 7)).toEqual({
      packets: 50,
      lost: 2,
      bytes: 4000,
      jitterMs: 12,
      at: 7,
    });
    expect(inboundSample({ forEach: () => undefined }, 1)).toBeNull();
  });

  it('judges loss and jitter since the last reading', () => {
    const a = sample();
    expect(remoteQuality(a, sample({ packets: 200, bytes: 18000, at: 2000 }))).toEqual({
      level: 'good',
      lossPct: 0,
      jitterMs: 5,
      kbps: 40,
    });
    // 20 of 100 new packets lost
    const bad = remoteQuality(a, sample({ packets: 180, lost: 20, at: 2000 }));
    expect(bad.level).toBe('poor');
    expect(bad.lossPct).toBe(20);
    // 4% loss is fair
    expect(remoteQuality(a, sample({ packets: 196, lost: 4, at: 2000 })).level).toBe('fair');
    // high jitter alone is poor
    expect(remoteQuality(a, sample({ packets: 200, jitterMs: 120, at: 2000 })).level).toBe('poor');
  });

  it('keeps the previous verdict while the far side sends nothing', () => {
    const before = { level: 'fair', lossPct: 4, jitterMs: 31, kbps: 12 } as const;
    expect(remoteQuality(sample(), sample({ at: 2000 }), before)).toBe(before);
  });

  it('is unknown with no readings and uses jitter alone on the first one', () => {
    expect(remoteLevel(null, null)).toBe('unknown');
    expect(remoteQuality(null, sample({ jitterMs: 40 })).level).toBe('fair');
  });
});
