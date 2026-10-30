import { describe, expect, it, vi, beforeEach } from 'vitest';
import { StaleRoom, type RoomApi, type ToolRoom, type ToolRoomEvent } from '../tierlist/roomApi';
import { RoomSync } from '../tierlist/sync';
import {
  UNRANKED,
  addItem,
  addTier,
  defaultTiers,
  itemsIn,
  moveItem,
  parseTierList,
  positionAt,
  removeTier,
  tierListText,
} from '../tierlist/model';
import {
  addOption,
  parsePoll,
  pollProblem,
  removeOption,
  setHead,
  setOption,
  starterPoll,
  toMessagePoll,
} from '../poll/model';

/** The server's version rule, in memory: a stale base is refused with the current room. */
class FakeServer implements RoomApi {
  room: ToolRoom;
  clients: RoomSync[] = [];
  calls = 0;
  constructor(state: Record<string, unknown> = {}) {
    this.room = {
      id: '1',
      channel_id: '9',
      guild_id: null,
      kind: 'tierlist',
      title: 'T',
      state,
      version: 1,
      created_by: null,
      created_at: 0,
      expires_at: 1,
    };
  }
  async get() {
    return structuredClone(this.room);
  }
  async patch(
    _c: string,
    _r: string,
    body: { base_version: number; set: Record<string, unknown>; title?: string },
  ) {
    this.calls++;
    await Promise.resolve();
    if (body.base_version !== this.room.version) throw new StaleRoom(structuredClone(this.room));
    const state = { ...this.room.state };
    for (const [k, v] of Object.entries(body.set)) {
      if (v === null) delete state[k];
      else state[k] = v;
    }
    this.room = {
      ...this.room,
      state,
      title: body.title ?? this.room.title,
      version: this.room.version + 1,
    };
    const ev: ToolRoomEvent = {
      id: '1',
      channel_id: '9',
      guild_id: null,
      kind: 'tierlist',
      version: this.room.version,
      title: this.room.title,
      actor_id: 'x',
      set: body.set,
    };
    // Delivered after the REST answer, like the gateway.
    setTimeout(() => this.clients.forEach((c) => void c.receive(ev)), 0);
    return structuredClone(this.room);
  }
  join() {
    const c = new RoomSync(structuredClone(this.room), this);
    this.clients.push(c);
    return c;
  }
}

const settle = () => new Promise((r) => setTimeout(r, 10));

describe('tier list model (UTL-COL-001)', () => {
  it('builds rows and items, and moves an item with one key', () => {
    let state: Record<string, unknown> = defaultTiers(['S', 'A']);
    const add = addItem(state, { text: 'pizza' })!;
    state = { ...state, ...add };
    const { tiers, items } = parseTierList(state);
    expect(tiers.map((t) => t.name)).toEqual(['S', 'A']);
    expect(items).toHaveLength(1);
    expect(items[0]!.tier).toBe(UNRANKED);
    const move = moveItem(state, items[0]!.id, tiers[0]!.id, 0)!;
    expect(Object.keys(move)).toEqual([`item.${items[0]!.id}`]);
    state = { ...state, ...move };
    expect(itemsIn(parseTierList(state).items, tiers[0]!.id).map((i) => i.text)).toEqual(['pizza']);
  });

  it('orders by position and inserts between neighbours without touching them', () => {
    expect(positionAt([], 0)).toBe(1024);
    expect(positionAt([{ pos: 10 }, { pos: 20 }], 1)).toBe(15);
    expect(positionAt([{ pos: 10 }], 5)).toBe(1034);
    expect(positionAt([{ pos: 10 }], 0)).toBe(-1014);
  });

  it('removing a row sends its items back to the pool', () => {
    let state: Record<string, unknown> = defaultTiers(['S']);
    state = { ...state, ...addItem(state, { text: 'a' })! };
    const tier = parseTierList(state).tiers[0]!;
    const item = parseTierList(state).items[0]!;
    state = { ...state, ...moveItem(state, item.id, tier.id, 0)! };
    const set = removeTier(state, tier.id);
    expect(set[`tier.${tier.id}`]).toBeNull();
    expect((set[`item.${item.id}`] as { tier: string }).tier).toBe(UNRANKED);
  });

  it('refuses empty items and tolerates malformed keys', () => {
    expect(addItem({}, { text: '  ' })).toBeNull();
    expect(addTier({}, ' ')).toBeNull();
    const { tiers, items } = parseTierList({
      'tier.x': 5,
      'item.y': 'no',
      'tier.ok': { name: 'S', pos: 1 },
    });
    expect(tiers.map((t) => t.id)).toEqual(['ok']);
    expect(items).toEqual([]);
  });

  it('keeps images as references and writes a text result', () => {
    const state = {
      ...defaultTiers(['S']),
      ...addItem({}, { img: { k: 'mount', mount: '5', key: 'a.png' } })!,
    };
    const { items } = parseTierList(state);
    expect(items[0]!.img).toEqual({ k: 'mount', mount: '5', key: 'a.png' });
    expect(JSON.stringify(state)).not.toMatch(/https?:/);
    expect(tierListText('Pizza', state, 'Unranked')).toContain('**Pizza**');
    expect(tierListText('Pizza', state, 'Unranked')).toContain('*Unranked*: [image]');
  });
});

describe('poll model (UTL-COL-001)', () => {
  it('validates, edits and turns into the message poll', () => {
    let state = starterPoll();
    expect(pollProblem(parsePoll(state))).toBe('question');
    state = { ...state, ...setHead(state, { question: 'Lunch?', multi: true, hours: 4 }) };
    expect(pollProblem(parsePoll(state))).toBe('options');
    const [a, b] = parsePoll(state).options;
    state = { ...state, ...setOption(state, a!.id, { text: 'Pizza', emoji: '🍕' })! };
    state = { ...state, ...setOption(state, b!.id, { text: 'Tacos' })! };
    expect(pollProblem(parsePoll(state))).toBeNull();
    expect(toMessagePoll(parsePoll(state))).toEqual({
      question: { text: 'Lunch?' },
      answers: [
        { poll_media: { text: 'Pizza', emoji: { name: '🍕' } } },
        { poll_media: { text: 'Tacos' } },
      ],
      duration: 4,
      allow_multiselect: true,
    });
  });

  it('keeps between 2 and 10 answers', () => {
    let state = starterPoll();
    expect(removeOption(state, parsePoll(state).options[0]!.id)).toBeNull();
    for (let i = 0; i < 8; i++) state = { ...state, ...addOption(state)! };
    expect(parsePoll(state).options).toHaveLength(10);
    expect(addOption(state)).toBeNull();
  });
});

describe('RoomSync, two people editing one room (UTL-COL-001)', () => {
  let server: FakeServer;
  beforeEach(() => {
    server = new FakeServer(defaultTiers(['S', 'A']));
  });

  it('applies an edit at once and confirms it with the server', async () => {
    const a = server.join();
    const tier = parseTierList(a.getView().state).tiers[0]!;
    const p = a.edit({ 'item.1': { text: 'x', tier: tier.id, pos: 1 } });
    expect(a.getView().state['item.1']).toBeTruthy(); // before the server answered
    expect(a.getStatus()).toBe('saving');
    await p;
    expect(a.getStatus()).toBe('live');
    expect(server.room.version).toBe(2);
    expect(a.dirty).toBe(false);
  });

  it('shows one person edit to the other', async () => {
    const a = server.join();
    const b = server.join();
    await a.edit({ 'item.1': { text: 'pizza', tier: '', pos: 1 } });
    await settle();
    expect(b.getView().state['item.1']).toEqual({ text: 'pizza', tier: '', pos: 1 });
    expect(b.room.version).toBe(2);
  });

  it('keeps both edits when two people change different keys at the same version', async () => {
    const a = server.join();
    const b = server.join();
    const pa = a.edit({ 'item.a': { text: 'from a', tier: '', pos: 1 } });
    const pb = b.edit({ 'item.b': { text: 'from b', tier: '', pos: 2 } });
    await Promise.all([pa, pb]);
    await settle();
    for (const c of [a, b, { getView: () => ({ state: server.room.state }) }]) {
      expect(c.getView().state['item.a']).toBeTruthy();
      expect(c.getView().state['item.b']).toBeTruthy();
    }
    expect(server.room.version).toBe(3);
  });

  it('is last writer wins per key when both change the same key', async () => {
    const a = server.join();
    const b = server.join();
    const pa = a.edit({ 'item.k': { text: 'a', tier: '', pos: 1 } });
    const pb = b.edit({ 'item.k': { text: 'b', tier: '', pos: 1 } });
    await Promise.all([pa, pb]);
    await settle();
    expect((server.room.state['item.k'] as { text: string }).text).toBe('b');
    expect((a.getView().state['item.k'] as { text: string }).text).toBe('b');
    expect((b.getView().state['item.k'] as { text: string }).text).toBe('b');
  });

  it('sends edits made while saving as the next patch, in order', async () => {
    const a = server.join();
    const first = a.edit({ n: 1 });
    void a.edit({ n: 2 });
    void a.edit({ m: 3 });
    await first;
    await settle();
    expect(server.room.state.n).toBe(2);
    expect(server.room.state.m).toBe(3);
    expect(server.calls).toBeLessThanOrEqual(2); // the two later edits travel together
  });

  it('refetches when it missed a version, and ignores its own echo', async () => {
    const a = server.join();
    const get = vi.spyOn(server, 'get');
    await a.receive({
      id: '1',
      channel_id: '9',
      guild_id: null,
      kind: 'tierlist',
      version: 5,
      title: 'T',
      actor_id: 'x',
      set: { z: 1 },
    });
    expect(get).toHaveBeenCalledTimes(1);
    await a.receive({
      id: '1',
      channel_id: '9',
      guild_id: null,
      kind: 'tierlist',
      version: 1,
      title: 'T',
      actor_id: 'x',
      set: { z: 2 },
    });
    expect(get).toHaveBeenCalledTimes(1);
  });

  it('goes offline on a failed save, keeps the edit visible and resends it on retry', async () => {
    const a = server.join();
    const real = server.patch.bind(server);
    let fail = true;
    server.patch = async (...args) => {
      if (fail) throw new Error('network');
      return real(...args);
    };
    await a.edit({ keep: 1 });
    expect(a.getStatus()).toBe('offline');
    expect(a.getView().state.keep).toBe(1);
    expect(a.dirty).toBe(true);
    fail = false;
    a.retry();
    await settle();
    await settle();
    expect(server.room.state.keep).toBe(1);
    expect(a.getStatus()).toBe('live');
  });

  it('stops when the room is deleted', async () => {
    const a = server.join();
    await a.receive({
      id: '1',
      channel_id: '9',
      guild_id: null,
      kind: 'tierlist',
      version: 3,
      title: 'T',
      actor_id: 'x',
      deleted: true,
    });
    expect(a.getStatus()).toBe('gone');
    await a.edit({ late: 1 });
    expect(server.calls).toBe(0);
  });
});
