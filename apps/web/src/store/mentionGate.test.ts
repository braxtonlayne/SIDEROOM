// SEC-AUD-237 (D-721): the live mention count follows the server's rule: a role mention counts, and a gate the
// Lounge registers can leave an author out. Without a gate, direct, `@everyone` and role all count.
import { afterEach, describe, expect, it } from 'vitest';
import { mentionCounts, setMentionGate } from './mentionGate';
import { memberKey, reduceDispatch, emptyData } from './reducer';
import type { Message } from './types';

const base = () =>
  ({
    ...emptyData(),
    currentUserId: 'u1',
    channels: {
      c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', last_message_id: '100' },
    },
    members: { [memberKey('g1', 'u1')]: { guild_id: 'g1', user_id: 'u1', roles: ['r1'] } },
    readStates: { c1: { channel_id: 'c1', last_message_id: '100', mention_count: 0 } },
  }) as never;

const message = (over: Record<string, unknown> = {}) => ({
  id: '150',
  channel_id: 'c1',
  guild_id: 'g1',
  author: { id: 'u2', username: 'bob' },
  content: 'hi',
  timestamp: '2026-01-01T00:00:00Z',
  mentions: [],
  ...over,
});
const count = (over: Record<string, unknown>) =>
  reduceDispatch(base(), 'MESSAGE_CREATE', message(over)).readStates?.c1?.mention_count ?? 0;
const direct = { mentions: [{ id: 'u1', username: 'me' }] };

afterEach(() => setMentionGate(null));

describe('the live mention count (no gate registered)', () => {
  it('counts a direct mention, @everyone and a role the viewer holds', () => {
    expect(count(direct)).toBe(1);
    expect(count({ mention_everyone: true })).toBe(1);
    expect(count({ mention_roles: ['r1'] })).toBe(1);
  });
  it('counts nothing for a plain message, a role the viewer lacks, an own message or a restored one', () => {
    expect(count({})).toBe(0);
    expect(count({ mention_roles: ['r9'] })).toBe(0);
    expect(count({ ...direct, author: { id: 'u1', username: 'me' } })).toBe(0);
    expect(count({ ...direct, restored: true })).toBe(0);
  });
  it('counts no role when the viewer is not a member in the store, or in a DM', () => {
    const noMember = { ...(base() as object), members: {} } as never;
    const out = reduceDispatch(noMember, 'MESSAGE_CREATE', message({ mention_roles: ['r1'] }));
    expect(out.readStates).toBeUndefined();
    expect(count({ guild_id: undefined, mention_roles: ['r1'] })).toBe(0);
  });
});

describe('the registered gate', () => {
  it('can leave an author out, and is told which kinds the message holds', () => {
    const seen: unknown[] = [];
    setMentionGate((m, kind) => {
      seen.push(kind);
      return m.author.id !== 'u2';
    });
    expect(count(direct)).toBe(0);
    expect(count({ ...direct, author: { id: 'u3', username: 'cy' } })).toBe(1);
    expect(seen[0]).toEqual({ direct: true, everyone: false, role: false });
    const both = { mention_everyone: true, mention_roles: ['r1'] };
    count({ ...both, author: { id: 'u3', username: 'cy' } });
    expect(seen[2]).toEqual({ direct: false, everyone: true, role: true });
  });
  it('is not asked for an own or a restored message', () => {
    let asked = 0;
    setMentionGate(() => {
      asked += 1;
      return true;
    });
    count({ ...direct, restored: true });
    count({ ...direct, author: { id: 'u1', username: 'me' } });
    expect(asked).toBe(0);
  });
  it('answers the plain rule when none is registered or it was removed', () => {
    const m = message(direct) as unknown as Message;
    expect(mentionCounts(m, { direct: true, everyone: false, role: false })).toBe(true);
    expect(mentionCounts(m, { direct: false, everyone: false, role: false })).toBe(false);
    setMentionGate(() => false);
    expect(mentionCounts(m, { direct: true, everyone: false, role: false })).toBe(false);
    setMentionGate(null);
    expect(mentionCounts(m, { direct: false, everyone: false, role: true })).toBe(true);
  });
});
