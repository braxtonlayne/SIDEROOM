import { describe, expect, it } from 'vitest';
import { titleWithBadge } from '../../pwa/titleBadge';
import type { Message } from '../../store';
import { desktopDecision, type NotifyContext } from './desktopNotify';
import type { GuildNotif } from './notifSettings';

const msg = (over: Partial<Message> = {}): Message =>
  ({
    id: 'm1',
    channel_id: 'c1',
    guild_id: 'g1',
    author: { id: 'u2', username: 'bob' },
    content: 'hi',
    timestamp: '2026-01-01T00:00:00Z',
    mentions: [],
    ...over,
  }) as Message;

const scope = (over: Partial<GuildNotif> = {}): GuildNotif => ({
  muted: false,
  endsAt: null,
  level: null,
  suppressEveryone: false,
  suppressRoles: false,
  channels: {},
  ...over,
});

const ctx = (over: Partial<NotifyContext> = {}): NotifyContext => ({
  me: 'u1',
  activeChannelId: undefined,
  windowFocused: false,
  status: 'online',
  scope: scope(),
  now: Date.parse('2026-01-01T00:00:00Z'),
  ...over,
});

describe('desktopDecision', () => {
  it('notifies mentions by default in servers, not plain messages', () => {
    expect(desktopDecision(msg(), ctx())).toBeNull();
    expect(desktopDecision(msg({ mentions: [{ id: 'u1', username: 'me' }] }), ctx())).toBe(
      'mention',
    );
    expect(desktopDecision(msg({ mention_everyone: true }), ctx())).toBe('mention');
  });

  it('notifies every DM unless the DM scope says otherwise', () => {
    expect(desktopDecision(msg({ guild_id: undefined }), ctx({ scope: undefined }))).toBe(
      'message',
    );
  });

  it('honours level, suppress everyone, mutes, focus and Do Not Disturb', () => {
    const all = ctx({ scope: scope({ level: 0 }) });
    expect(desktopDecision(msg(), all)).toBe('message');
    expect(desktopDecision(msg(), ctx({ scope: scope({ level: 2 }) }))).toBeNull();
    expect(
      desktopDecision(
        msg({ mention_everyone: true }),
        ctx({ scope: scope({ suppressEveryone: true }) }),
      ),
    ).toBeNull();
    const muted = scope({ level: 0, channels: { c1: { muted: true, endsAt: null, level: null } } });
    expect(desktopDecision(msg(), ctx({ scope: muted }))).toBeNull();
    const expired = scope({
      level: 0,
      channels: { c1: { muted: true, endsAt: '2025-12-31T00:00:00Z', level: null } },
    });
    expect(desktopDecision(msg(), ctx({ scope: expired }))).toBe('message');
    expect(desktopDecision(msg(), { ...all, status: 'dnd' })).toBeNull();
    expect(
      desktopDecision(msg(), { ...all, windowFocused: true, activeChannelId: 'c1' }),
    ).toBeNull();
    expect(desktopDecision(msg(), { ...all, windowFocused: false, activeChannelId: 'c1' })).toBe(
      'message',
    );
    expect(desktopDecision(msg({ author: { id: 'u1', username: 'me' } }), all)).toBeNull();
    expect(desktopDecision(msg({ type: 7 }), all)).toBeNull();
  });
});

describe('titleWithBadge', () => {
  it('adds, replaces and removes the unread prefix', () => {
    expect(titleWithBadge('SIDE ROOM', 3)).toBe('(3) SIDE ROOM');
    expect(titleWithBadge('(3) SIDE ROOM', 5)).toBe('(5) SIDE ROOM');
    expect(titleWithBadge('(5) SIDE ROOM', 0)).toBe('SIDE ROOM');
    expect(titleWithBadge('x', 150)).toBe('(99+) x');
  });
});
