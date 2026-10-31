import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CoopGuest } from '../guest';
import { CoopHost } from '../host';
import { PAD, bit } from '../input';
import { SEAT_NOT_ALLOWED, decode, encode } from '../protocol';
import { holdsViewGames, viewGamesGate, type GateRole } from '../seatGate';
import { Hub, RecordingCore } from './hub';

const G = '100';
const OWNER = '1';
const roles: GateRole[] = [
  { id: G, permissions: '0' }, // @everyone
  { id: '201', permissions: '0', sr_storage_grants: ['view_games'] },
  { id: '202', permissions: '0', sr_storage_grants: ['view_cinema'] },
  { id: '203', permissions: String(1n << 3n) }, // ADMINISTRATOR
  { id: '204', permissions: String(1n << 5n) }, // MANAGE_GUILD (implies manage)
  { id: '205', permissions: '0', sr_storage_grants: ['manage'] },
];

describe('holdsViewGames (mirror of sr_storage::scopes::grants_for)', () => {
  it('admits the owner, ADMINISTRATOR, MANAGE_GUILD, manage and view_games, nobody else', () => {
    expect(holdsViewGames(G, OWNER, OWNER, roles, [])).toBe(true);
    expect(holdsViewGames(G, OWNER, '7', roles, ['201'])).toBe(true);
    expect(holdsViewGames(G, OWNER, '7', roles, ['203'])).toBe(true);
    expect(holdsViewGames(G, OWNER, '7', roles, ['204'])).toBe(true);
    expect(holdsViewGames(G, OWNER, '7', roles, ['205'])).toBe(true);
    expect(holdsViewGames(G, OWNER, '7', roles, [])).toBe(false);
    expect(holdsViewGames(G, OWNER, '7', roles, ['202'])).toBe(false);
    expect(holdsViewGames(G, OWNER, '7', roles, ['999'])).toBe(false);
  });

  it('counts a grant on @everyone for every member', () => {
    const open = roles.map((r) => (r.id === G ? { ...r, sr_storage_grants: ['view_games'] } : r));
    expect(holdsViewGames(G, OWNER, '7', open, [])).toBe(true);
  });

  it('treats a malformed permission string as no bits', () => {
    expect(holdsViewGames(G, OWNER, '7', [{ id: G, permissions: 'x' }], [])).toBe(false);
  });
});

describe('viewGamesGate', () => {
  it('reads roles and the member at each JOIN and fails closed', async () => {
    const fetcher = vi.fn(async (path: string) => {
      if (path === `/guilds/${G}/roles`) return roles;
      if (path === `/guilds/${G}/members/7`) return { roles: ['201'] };
      if (path === `/guilds/${G}/members/8`) return { roles: [] };
      if (path === `/guilds/${G}/members/9`) return { roles: ['201'], pending: true };
      throw new Error('404');
    }) as unknown as <T>(path: string) => Promise<T>;
    const gate = viewGamesGate(G, OWNER, fetcher);
    expect(await gate('7')).toBe(true);
    expect(await gate('8')).toBe(false);
    expect(await gate('9')).toBe(false); // membership screening not passed
    expect(await gate('10')).toBe(false); // the member read failed
    expect(await gate(OWNER)).toBe(true);
    expect(await viewGamesGate(null, OWNER, fetcher)('7')).toBe(false);
  });
});

describe('the host applies the seat rule', () => {
  let clock = 0;
  beforeEach(() => {
    vi.useFakeTimers();
    clock = 0;
  });
  afterEach(() => vi.useRealTimers());

  function setup(may: (id: string) => boolean | Promise<boolean>) {
    const hub = new Hub();
    const core = new RecordingCore();
    const host = new CoopHost({
      transport: hub.transport('1'),
      game: Object.assign(core, { title: 'Test ROM' }),
      selfId: '1',
      mayTakeSeat: may,
      now: () => clock,
    });
    const guest = (id: string) =>
      new CoopGuest({ transport: hub.transport(id), selfId: id, now: () => clock });
    return { hub, core, host, guest };
  }

  it('refuses a guest without the grant (phase denied) and its pad reaches no port', async () => {
    const { core, host, guest } = setup((id) => id === '3');
    host.start();
    const g = guest('2');
    g.open();
    g.join('1');
    await vi.waitFor(() => expect(g.snapshot().phase).toBe('denied'));
    g.setButtons(bit(PAD.A));
    expect(core.calls).toEqual([]);
    expect(host.snapshot().seats).toEqual([]);
  });

  it('seats a guest with the grant once the answer arrives', async () => {
    const { core, host, guest } = setup(async (id) => id === '3');
    host.start();
    const g = guest('3');
    g.open();
    g.join('1');
    await vi.waitFor(() => expect(g.snapshot()).toMatchObject({ phase: 'seated', port: 1 }));
    g.setButtons(bit(PAD.A));
    expect(core.calls).toEqual([[1, PAD.A, 1]]);
  });

  it('refuses when the check throws or rejects', async () => {
    const { host, guest } = setup(() => {
      throw new Error('boom');
    });
    host.start();
    const g = guest('3');
    g.open();
    g.join('1');
    await vi.waitFor(() => expect(g.snapshot().phase).toBe('denied'));
  });

  it('asks once while an answer is pending, however many JOINs arrive', async () => {
    let release: (ok: boolean) => void = () => {};
    const may = vi.fn(() => new Promise<boolean>((r) => (release = r)));
    const { hub, host } = setup(may);
    host.start();
    const raw = hub.transport('4');
    raw.join();
    void raw.send(encode({ type: 'join', host: '1' }));
    void raw.send(encode({ type: 'join', host: '1' }));
    expect(may).toHaveBeenCalledTimes(1);
    const seen: number[] = [];
    raw.onMessage((_s, b) => {
      const m = decode(b);
      if (m?.type === 'seat') seen.push(m.port);
    });
    release(false);
    await vi.waitFor(() => expect(seen).toEqual([SEAT_NOT_ALLOWED]));
  });
});
