import { describe, expect, it } from 'vitest';
import {
  COOP_INSTANCE,
  MAX_TITLE_BYTES,
  MSG,
  clampTitle,
  decode,
  encode,
  type CoopMessage,
} from '../protocol';

const BIG = '9007199254740993'; // above Number.MAX_SAFE_INTEGER: ids must survive as u64

describe('co-op message format', () => {
  const samples: CoopMessage[] = [
    { type: 'announce', version: 1, free: 2, hostPingMs: 37, title: 'Pong, two paddles' },
    { type: 'discover' },
    { type: 'join', host: BIG },
    { type: 'seat', guest: BIG, port: 2 },
    { type: 'pad', host: BIG, seq: 65535, buttons: 0x8101 },
    { type: 'leave', host: BIG },
    { type: 'ping', host: BIG, t: 4294967295 },
    { type: 'pong', guest: BIG, t: 12345 },
    { type: 'close' },
    { type: 'kick', guest: BIG },
  ];

  it.each(samples)('round-trips %j', (m) => {
    expect(decode(encode(m))).toEqual(m);
  });

  it('has the documented byte layout of a PAD packet', () => {
    const b = encode({ type: 'pad', host: '1', seq: 0x0102, buttons: 0x0a0b });
    expect([...b]).toEqual([MSG.PAD, 0, 0, 0, 0, 0, 0, 0, 1, 0x01, 0x02, 0x0a, 0x0b]);
  });

  it('keeps the whole pad in 16 bits and the sequence in 16 bits', () => {
    const m = decode(encode({ type: 'pad', host: '1', seq: 0x10005, buttons: 0x1ffff }));
    expect(m).toEqual({ type: 'pad', host: '1', seq: 5, buttons: 0xffff });
  });

  it('drops malformed packets instead of throwing', () => {
    expect(decode(new Uint8Array())).toBeNull();
    expect(decode(Uint8Array.of(99))).toBeNull();
    expect(decode(Uint8Array.of(MSG.PAD, 1, 2))).toBeNull();
    expect(decode(Uint8Array.of(MSG.DISCOVER, 0))).toBeNull();
    const longPad = new Uint8Array(14);
    longPad[0] = MSG.PAD;
    expect(decode(longPad)).toBeNull();
    // an announce whose title is not UTF-8
    expect(decode(Uint8Array.of(MSG.ANNOUNCE, 1, 1, 0, 0, 0xff, 0xfe))).toBeNull();
  });

  it('refuses ids that do not fit a u64', () => {
    expect(() => encode({ type: 'join', host: '18446744073709551616' })).toThrow(RangeError);
    expect(() => encode({ type: 'join', host: '-1' })).toThrow();
  });

  it('cuts a long title on a character boundary', () => {
    const title = 'é'.repeat(100); // 2 bytes each
    const cut = clampTitle(title);
    expect(cut.length).toBeLessThanOrEqual(MAX_TITLE_BYTES);
    expect(new TextDecoder('utf-8', { fatal: true }).decode(cut)).toBe('é'.repeat(32));
    const m = decode(encode({ type: 'announce', version: 1, free: 3, hostPingMs: 0, title }));
    expect(m?.type === 'announce' && m.title).toBe('é'.repeat(32));
  });

  it('clamps the announced ping and the free count', () => {
    const m = decode(
      encode({ type: 'announce', version: 1, free: 999, hostPingMs: 99999, title: '' }),
    );
    expect(m).toMatchObject({ free: 255, hostPingMs: 65535 });
  });

  it('uses an instance id no activity snowflake can have (top bit set)', () => {
    expect(BigInt(COOP_INSTANCE) >> 63n).toBe(1n);
    expect(BigInt(COOP_INSTANCE) < 1n << 64n).toBe(true);
  });
});
