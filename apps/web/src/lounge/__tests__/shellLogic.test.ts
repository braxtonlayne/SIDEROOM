import { describe, expect, it } from 'vitest';
import type { Channel, RailEntry } from '../data';
import { applyMutes } from '../shell/applyMutes';
import { planDrop, planStep } from '../shell/channelMoves';
import { fromWire, isMuted, type GuildNotif } from '../shell/notifSettings';
import {
  addToFolder,
  applyRailLayout,
  deleteFolder,
  dropZone,
  flatGuildIds,
  makeFolder,
  moveBy,
  moveEntry,
  moveGuildNextTo,
  removeFromFolder,
  updateFolder,
} from '../shell/railLayout';
import { applyDrop } from '../shell/useRailDnd';
import type { LiveSnapshot } from '../live';

const g = (guildId: string): RailEntry => ({ kind: 'guild', guildId });
const folder = (id: string, ...guildIds: string[]): RailEntry => ({
  kind: 'folder',
  id,
  name: null,
  color: '#fff',
  guildIds,
});
const keys = (rail: RailEntry[]) =>
  rail.map((e) => (e.kind === 'guild' ? e.guildId : `${e.id}[${e.guildIds.join(',')}]`));

describe('rail layout', () => {
  it('uses the server order when nothing is stored', () => {
    expect(keys(applyRailLayout([g('a'), g('b')], null))).toEqual(['a', 'b']);
  });

  it('keeps the stored order, drops departed guilds and appends new ones', () => {
    const stored = [g('c'), folder('f', 'x', 'a'), g('b')];
    const out = applyRailLayout([g('a'), g('b'), g('c'), g('d')], stored);
    expect(keys(out)).toEqual(['c', 'f[a]', 'b', 'd']);
  });

  it('removes a folder that lost every guild', () => {
    expect(keys(applyRailLayout([g('a')], [folder('f', 'x', 'y'), g('a')]))).toEqual(['a']);
  });

  it('does not list a guild twice', () => {
    expect(keys(applyRailLayout([g('a'), g('b')], [g('a'), folder('f', 'a', 'b')]))).toEqual([
      'a',
      'f[b]',
    ]);
  });

  it('moves entries before and after one another', () => {
    const rail = [g('a'), g('b'), g('c')];
    expect(keys(moveEntry(rail, 'a', 'c', 'after'))).toEqual(['b', 'c', 'a']);
    expect(keys(moveEntry(rail, 'c', 'a', 'before'))).toEqual(['c', 'a', 'b']);
    expect(keys(moveEntry(rail, 'a', 'a', 'before'))).toEqual(['a', 'b', 'c']);
    expect(keys(moveBy(rail, 'b', -1))).toEqual(['b', 'a', 'c']);
    expect(keys(moveBy(rail, 'a', -1))).toEqual(['a', 'b', 'c']);
  });

  it('creates a folder by dropping one guild on another', () => {
    const out = makeFolder([g('a'), g('b'), g('c')], 'c', 'a', { id: 'f1', color: '#111' });
    expect(keys(out)).toEqual(['f1[a,c]', 'b']);
  });

  it('adds to, removes from and dissolves folders', () => {
    const rail = [folder('f', 'a', 'b'), g('c')];
    expect(keys(addToFolder(rail, 'c', 'f'))).toEqual(['f[a,b,c]']);
    expect(keys(addToFolder(rail, 'c', 'f', 'b'))).toEqual(['f[a,c,b]']);
    expect(keys(removeFromFolder(rail, 'a'))).toEqual(['f[b]', 'a', 'c']);
    expect(keys(removeFromFolder([folder('f', 'a'), g('c')], 'a'))).toEqual(['a', 'c']);
    expect(keys(deleteFolder(rail, 'f'))).toEqual(['a', 'b', 'c']);
    expect(flatGuildIds(rail)).toEqual(['a', 'b', 'c']);
  });

  it('moves a guild out of a folder next to another entry', () => {
    const out = moveGuildNextTo([folder('f', 'a', 'b'), g('c')], 'a', 'c', 'after');
    expect(keys(out)).toEqual(['f[b]', 'c', 'a']);
  });

  it('renames and recolors a folder', () => {
    const [f] = updateFolder([folder('f', 'a')], 'f', { name: '  Games ', color: '#abc' });
    expect(f).toMatchObject({ name: 'Games', color: '#abc' });
    const [cleared] = updateFolder([f!], 'f', { name: '   ' });
    expect(cleared).toMatchObject({ name: null });
  });

  it('reads drop zones from the pointer offset', () => {
    expect(dropZone(2, 48)).toBe('before');
    expect(dropZone(24, 48)).toBe('into');
    expect(dropZone(46, 48)).toBe('after');
  });

  it('plans drops for guilds and folders', () => {
    const rail = [g('a'), g('b'), folder('f', 'c', 'd')];
    const top = (i: number): { kind: 'top'; entry: RailEntry } => ({
      kind: 'top',
      entry: rail[i]!,
    });
    expect(keys(applyDrop(rail, { kind: 'guild', id: 'a' }, top(1), 'after')!)).toEqual([
      'b',
      'a',
      'f[c,d]',
    ]);
    expect(keys(applyDrop(rail, { kind: 'guild', id: 'a' }, top(2), 'into')!)).toEqual([
      'b',
      'f[c,d,a]',
    ]);
    expect(applyDrop(rail, { kind: 'guild', id: 'a' }, top(1), 'into')!.length).toBe(2);
    // A folder cannot be dropped into a folder.
    expect(
      applyDrop(
        rail,
        { kind: 'folder', id: 'f' },
        { kind: 'inFolder', folderId: 'f', guildId: 'c', siblings: ['c', 'd'] },
        'before',
      ),
    ).toBeNull();
    expect(
      keys(
        applyDrop(
          rail,
          { kind: 'guild', id: 'a' },
          { kind: 'inFolder', folderId: 'f', guildId: 'd', siblings: ['c', 'd'] },
          'before',
        )!,
      ),
    ).toEqual(['b', 'f[c,a,d]']);
  });
});

const ch = (
  id: string,
  type: Channel['type'],
  position: number,
  parentId: string | null = null,
): Channel => ({
  id,
  guildId: 'g',
  type,
  name: id,
  parentId,
  position,
  unread: false,
  mentionCount: 0,
  canSend: true,
});

describe('channel moves', () => {
  const list = [
    ch('l1', 'text', 0),
    ch('catA', 'category', 1),
    ch('catB', 'category', 2),
    ch('a1', 'text', 0, 'catA'),
    ch('a2', 'text', 1, 'catA'),
    ch('b1', 'voice', 0, 'catB'),
  ];
  const apply = (moves: ReturnType<typeof planDrop>) =>
    Object.fromEntries((moves ?? []).map((m) => [m.id, [m.position, m.parentId]]));

  it('reorders inside a category', () => {
    expect(apply(planDrop(list, 'a2', 'a1', 'before'))).toEqual({
      a2: [0, 'catA'],
      a1: [1, 'catA'],
    });
  });

  it('re-parents a channel dropped on a channel of another category', () => {
    const m = apply(planDrop(list, 'a1', 'b1', 'after'));
    expect(m.a1).toEqual([1, 'catB']);
    expect(m.a2).toEqual([0, 'catA']);
  });

  it('puts a channel dropped on a category at its top', () => {
    const m = apply(planDrop(list, 'l1', 'catB', 'into'));
    expect(m.l1).toEqual([0, 'catB']);
    expect(m.b1).toEqual([1, 'catB']);
  });

  it('reorders categories and ignores a category dropped on a channel', () => {
    const m = apply(planDrop(list, 'catB', 'catA', 'before'));
    expect(m.catB).toEqual([1, null]);
    expect(m.catA).toEqual([2, null]);
    expect(planDrop(list, 'catA', 'a1', 'after')).toBeNull();
    expect(planDrop(list, 'a1', 'a1', 'after')).toBeNull();
  });

  it('steps up and down within siblings', () => {
    expect(apply(planStep(list, 'a2', -1))).toEqual({ a2: [0, 'catA'], a1: [1, 'catA'] });
    expect(planStep(list, 'a1', -1)).toBeNull();
    expect(planStep(list, 'catB', 1)).toBeNull();
    expect(planStep(list, 'l1', 1)).toBeNull();
  });
});

describe('notification settings', () => {
  const wire = {
    muted: true,
    mute_config: { end_time: '2026-06-01T00:00:00Z' },
    message_notifications: 1,
    suppress_everyone: true,
    suppress_roles: false,
    channel_overrides: [
      { channel_id: 'c1', muted: true, mute_config: null, message_notifications: 2 },
    ],
  };

  it('maps the wire shape', () => {
    expect(fromWire(wire)).toEqual({
      muted: true,
      endsAt: '2026-06-01T00:00:00Z',
      level: 1,
      suppressEveryone: true,
      suppressRoles: false,
      suppressHighlights: false,
      mobilePush: true,
      channels: { c1: { muted: true, endsAt: null, level: 2 } },
    });
  });

  it('ends a timed mute', () => {
    const now = Date.parse('2026-05-31T00:00:00Z');
    expect(isMuted({ muted: true, endsAt: '2026-06-01T00:00:00Z' }, now)).toBe(true);
    expect(isMuted({ muted: true, endsAt: '2026-05-30T00:00:00Z' }, now)).toBe(false);
    expect(isMuted({ muted: true, endsAt: null }, now)).toBe(true);
    expect(isMuted({ muted: false, endsAt: null }, now)).toBe(false);
    expect(isMuted(undefined, now)).toBe(false);
  });

  const snap = (): LiveSnapshot =>
    ({
      guilds: [
        { id: 'g', name: 'G', unread: true, mentionCount: 2 },
        { id: 'h', name: 'H', unread: true, mentionCount: 0 },
      ],
      channelsByGuild: {
        g: [
          { ...ch('k', 'category', 0), id: 'k' },
          { ...ch('c1', 'text', 0, 'k'), unread: true, mentionCount: 2 },
        ],
        h: [{ ...ch('c2', 'text', 0), unread: true }],
      },
    }) as unknown as LiveSnapshot;
  const notif = (over: Partial<GuildNotif>): GuildNotif => ({
    muted: false,
    endsAt: null,
    level: null,
    suppressEveryone: false,
    suppressRoles: false,
    suppressHighlights: false,
    mobilePush: true,
    channels: {},
    ...over,
  });

  it('returns the same snapshot when nothing is muted', () => {
    const s = snap();
    expect(applyMutes(s, {})).toBe(s);
    expect(applyMutes(s, { g: notif({}) })).toBe(s);
  });

  it('a muted server shows no unread or mentions and mutes its channels', () => {
    const out = applyMutes(snap(), { g: notif({ muted: true }) });
    expect(out.guilds[0]).toMatchObject({ muted: true, unread: false, mentionCount: 0 });
    expect(out.guilds[1]).toMatchObject({ unread: true });
    expect(out.channelsByGuild.g!.every((c) => c.muted)).toBe(true);
  });

  it('a muted category mutes its channels and recomputes the server badge', () => {
    const out = applyMutes(snap(), {
      g: notif({ channels: { k: { muted: true, endsAt: null, level: null } } }),
    });
    expect(out.channelsByGuild.g!.find((c) => c.id === 'c1')!.muted).toBe(true);
    expect(out.guilds[0]).toMatchObject({ unread: false, mentionCount: 0 });
  });
});
