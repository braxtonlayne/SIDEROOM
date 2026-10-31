import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CoopGuest, HOST_STALE_MS } from '../guest';
import { CoopHost, SEAT_IDLE_MS, STUCK_PAD_MS } from '../host';
import { PAD, bit } from '../input';
import { NO_SEAT, decode, encode } from '../protocol';
import { SeatTable } from '../seats';
import { Hub, RecordingCore } from './hub';

let clock = 0;
const now = () => clock;

function setup() {
  const hub = new Hub();
  const core = new RecordingCore();
  const host = new CoopHost({
    transport: hub.transport('1'),
    game: Object.assign(core, { title: 'Test ROM' }),
    selfId: '1',
    getPingMs: () => 12,
    now,
  });
  const guest = (id: string) => new CoopGuest({ transport: hub.transport(id), selfId: id, now });
  return { hub, core, host, guest };
}

beforeEach(() => {
  vi.useFakeTimers();
  clock = 0;
});
afterEach(() => vi.useRealTimers());

describe('port assignment', () => {
  it('hands out ports 1 to 3 (players 2 to 4) in order and never port 0', () => {
    const s = new SeatTable();
    expect(s.assign('10')).toBe(1);
    expect(s.assign('11')).toBe(2);
    expect(s.assign('12')).toBe(3);
    expect(s.assign('13')).toBeNull();
    expect(s.free).toBe(0);
  });

  it('gives a returning guest the same seat and reuses a freed one', () => {
    const s = new SeatTable();
    s.assign('10');
    s.assign('11');
    expect(s.assign('10')).toBe(1);
    s.release('10');
    expect(s.assign('12')).toBe(1);
    expect(s.entries()).toEqual([
      { userId: '12', port: 1 },
      { userId: '11', port: 2 },
    ]);
  });
});

describe('host and guests over a relay', () => {
  it('seats a guest, applies its pad to port 1 only, and releases on leave', () => {
    const { core, host, guest } = setup();
    host.start();
    const g = guest('2');
    g.open();
    expect(g.snapshot().hosts).toMatchObject([
      { userId: '1', title: 'Test ROM', free: 3, hostPingMs: 12 },
    ]);
    g.join('1');
    expect(g.snapshot()).toMatchObject({ phase: 'seated', port: 1 });

    g.setButtons(bit(PAD.A) | bit(PAD.RIGHT));
    expect(core.calls).toEqual([
      [1, PAD.RIGHT, 1],
      [1, PAD.A, 1],
    ]);
    g.setButtons(bit(PAD.RIGHT));
    expect(core.calls.at(-1)).toEqual([1, PAD.A, 0]);
    g.leave();
    expect(core.down.get(1)?.size).toBe(0);
    expect(host.snapshot().seats).toEqual([]);
  });

  it('puts the second and third guest on ports 2 and 3 and refuses the fourth', () => {
    const { core, host, guest } = setup();
    host.start();
    const gs = ['2', '3', '4', '5'].map(guest);
    gs.forEach((g) => {
      g.open();
      g.join('1');
    });
    expect(gs.map((g) => g.snapshot().port)).toEqual([1, 2, 3, null]);
    expect(gs[3]!.snapshot().phase).toBe('full');
    gs[1]!.setButtons(bit(PAD.START));
    gs[3]!.setButtons(bit(PAD.START)); // not seated: nothing is sent
    expect(core.calls).toEqual([[2, PAD.START, 1]]);
  });

  it('never lets a guest reach port 0: the port comes from the seat, not the packet', () => {
    const { core, host, hub } = setup();
    host.start();
    const evil = hub.transport('9');
    evil.join();
    // a pad before any seat is ignored
    void evil.send(encode({ type: 'pad', host: '1', seq: 1, buttons: 0xffff }));
    expect(core.calls).toEqual([]);
    void evil.send(encode({ type: 'join', host: '1' }));
    void evil.send(encode({ type: 'pad', host: '1', seq: 2, buttons: bit(PAD.A) }));
    expect(core.calls).toEqual([[1, PAD.A, 1]]);
    expect(core.calls.every(([port]) => port !== 0)).toBe(true);
  });

  it('ignores pads meant for another host, old sequence numbers and the host echo', () => {
    const { core, host, hub } = setup();
    host.start();
    const t = hub.transport('2');
    t.join();
    void t.send(encode({ type: 'join', host: '1' }));
    void t.send(encode({ type: 'pad', host: '77', seq: 1, buttons: bit(PAD.A) }));
    expect(core.calls).toEqual([]);
    void t.send(encode({ type: 'pad', host: '1', seq: 10, buttons: bit(PAD.A) }));
    void t.send(encode({ type: 'pad', host: '1', seq: 9, buttons: 0 })); // late: dropped
    void t.send(encode({ type: 'pad', host: '1', seq: 10, buttons: 0 })); // duplicate: dropped
    expect(core.calls).toEqual([[1, PAD.A, 1]]);
    void t.send(encode({ type: 'pad', host: '1', seq: 0, buttons: 0 })); // 10 -> 0 is wrap-around? no: behind
    expect(core.calls).toHaveLength(1);
  });

  it('accepts the sequence number wrapping around', () => {
    const { core, host, hub } = setup();
    host.start();
    const t = hub.transport('2');
    t.join();
    void t.send(encode({ type: 'join', host: '1' }));
    void t.send(encode({ type: 'pad', host: '1', seq: 65535, buttons: bit(PAD.B) }));
    void t.send(encode({ type: 'pad', host: '1', seq: 1, buttons: 0 }));
    expect(core.calls).toEqual([
      [1, PAD.B, 1],
      [1, PAD.B, 0],
    ]);
  });

  it('only a guest that has joined this host can send; a stranger cannot kick or seat others', () => {
    const { host, hub, guest } = setup();
    host.start();
    const g = guest('2');
    g.open();
    g.join('1');
    const evil = hub.transport('9');
    evil.join();
    void evil.send(encode({ type: 'kick', guest: '2' })); // not a host message
    void evil.send(encode({ type: 'seat', guest: '2', port: NO_SEAT })); // sender is not the host
    expect(g.snapshot().phase).toBe('seated');
    void evil.send(encode({ type: 'close' }));
    expect(g.snapshot().phase).toBe('seated'); // a close from someone else does not end our session
  });

  it('releases a stuck pad after the watchdog time and drops a silent guest', () => {
    const { core, host, hub } = setup();
    host.start();
    // a raw client: it never sends keepalives, like a frozen tab
    const t = hub.transport('2');
    t.join();
    void t.send(encode({ type: 'join', host: '1' }));
    void t.send(encode({ type: 'pad', host: '1', seq: 1, buttons: bit(PAD.A) }));
    clock += STUCK_PAD_MS + 1;
    vi.advanceTimersByTime(1000);
    expect(core.down.get(1)?.size).toBe(0);
    expect(host.snapshot().seats).toHaveLength(1); // still seated, just released
    clock += SEAT_IDLE_MS;
    vi.advanceTimersByTime(1000);
    expect(host.snapshot().seats).toHaveLength(0);
    expect(hub.wire.map(([, b]) => decode(b)).some((m) => m?.type === 'kick')).toBe(true);
  });

  it('keepalive packets stop the watchdog while a button is held', () => {
    const { core, host, guest } = setup();
    host.start();
    const g = guest('2');
    g.open();
    g.join('1');
    g.setButtons(bit(PAD.A));
    for (let i = 0; i < 6; i += 1) {
      clock += 1000;
      vi.advanceTimersByTime(1000);
    }
    expect(core.down.get(1)?.has(PAD.A)).toBe(true);
  });

  it('kick and stop release buttons and tell the guests', () => {
    const { core, host, guest } = setup();
    host.start();
    const a = guest('2');
    const b = guest('3');
    [a, b].forEach((g) => {
      g.open();
      g.join('1');
    });
    a.setButtons(bit(PAD.UP));
    b.setButtons(bit(PAD.DOWN));
    host.kick('2');
    expect(a.snapshot().phase).toBe('kicked');
    expect(core.down.get(1)?.size).toBe(0);
    host.stop();
    expect(b.snapshot().phase).toBe('closed');
    expect(core.down.get(2)?.size).toBe(0);
    expect(host.snapshot().running).toBe(false);
  });

  it('measures the round trip with ping and pong', () => {
    const { host, guest } = setup();
    host.start();
    const g = guest('2');
    g.open();
    g.join('1');
    clock = 5000;
    vi.advanceTimersByTime(1000); // the guest pings; the relay answers at once, so the clock has not moved
    expect(g.snapshot().rttMs).toBe(0);
    // a delayed relay: ping at t, pong 40 ms later
    const sent: Uint8Array[] = [];
    const fake = new CoopGuest({
      selfId: '2',
      now,
      transport: {
        join: () => undefined,
        leave: () => undefined,
        send: (b) => (sent.push(b), Promise.resolve(true)),
        onMessage: (cb) => {
          deliver = cb;
          return () => undefined;
        },
      },
    });
    let deliver: (s: string, b: Uint8Array) => void = () => undefined;
    fake.open();
    fake.join('1');
    deliver('1', encode({ type: 'seat', guest: '2', port: 1 }));
    clock = 10_000;
    vi.advanceTimersByTime(1000);
    const ping = sent.map((b) => decode(b)).find((m) => m?.type === 'ping');
    expect(ping).toMatchObject({ type: 'ping', host: '1', t: 10_000 });
    clock = 10_040;
    deliver('1', encode({ type: 'pong', guest: '2', t: 10_000 }));
    expect(fake.snapshot().rttMs).toBe(40);
    deliver('1', encode({ type: 'pong', guest: '3', t: 10_000 })); // not for us
    deliver('9', encode({ type: 'pong', guest: '2', t: 10_020 })); // not from our host
    expect(fake.snapshot().rttMs).toBe(40);
  });

  it('forgets a host that stops announcing and learns of a late one by discover', () => {
    const { host, guest } = setup();
    const g = guest('2');
    g.open();
    expect(g.snapshot().hosts).toEqual([]);
    host.start();
    expect(g.snapshot().hosts).toHaveLength(1);
    clock += HOST_STALE_MS + 1;
    vi.advanceTimersByTime(1000);
    expect(g.snapshot().hosts).toEqual([]);
    const late = guest('3');
    late.open(); // its discover makes the host announce at once
    expect(late.snapshot().hosts).toHaveLength(1);
  });

  it('the announce shows the free seats as guests come and go', () => {
    const { host, guest } = setup();
    host.start();
    const watcher = guest('9');
    watcher.open();
    const g = guest('2');
    g.open();
    g.join('1');
    expect(watcher.snapshot().hosts[0]?.free).toBe(2);
    g.leave();
    expect(watcher.snapshot().hosts[0]?.free).toBe(3);
  });

  it('a closed guest stops sending and listening', () => {
    const { hub, host, guest } = setup();
    host.start();
    const g = guest('2');
    g.open();
    g.join('1');
    g.close();
    const before = hub.wire.length;
    g.setButtons(bit(PAD.A));
    expect(hub.wire.length).toBe(before);
    expect(host.snapshot().seats).toEqual([]);
  });
});
