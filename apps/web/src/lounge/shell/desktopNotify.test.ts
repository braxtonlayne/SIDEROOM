import { describe, expect, it } from 'vitest';
import { titleWithBadge } from '../../pwa/titleBadge';
import type { Message } from '../../store';
import {
  desktopDecision,
  reactionDecision,
  type NotifyContext,
  type ReactionContext,
  type ReactionEvent,
} from './desktopNotify';
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
  suppressHighlights: false,
  mobilePush: true,
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
  hidden: { blocked: new Set<string>(), ignored: new Set<string>() },
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

// SEC-AUD-228 (D-717): nothing from an account the viewer blocked or ignores.
describe('desktopDecision and hidden authors', () => {
  const hidden = (blocked: string[], ignored: string[]) => ({
    hidden: { blocked: new Set(blocked), ignored: new Set(ignored) },
  });
  const mention = { mentions: [{ id: 'u1', username: 'me' }] };
  it('a blocked author raises nothing, for a mention or a direct message', () => {
    expect(desktopDecision(msg(mention), ctx(hidden(['u2'], [])))).toBeNull();
    expect(
      desktopDecision(
        msg({ guild_id: undefined }),
        ctx({ scope: undefined, ...hidden(['u2'], []) }),
      ),
    ).toBeNull();
  });
  it('an ignored author raises nothing, for a mention or a direct message', () => {
    expect(desktopDecision(msg(mention), ctx(hidden([], ['u2'])))).toBeNull();
    expect(
      desktopDecision(
        msg({ guild_id: undefined }),
        ctx({ scope: undefined, ...hidden([], ['u2']) }),
      ),
    ).toBeNull();
  });
  it('someone else being blocked changes nothing', () => {
    expect(desktopDecision(msg(mention), ctx(hidden(['u7'], ['u8'])))).toBe('mention');
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

// SEC-AUD-221 (D-703)
describe('reactionDecision', () => {
  const ev = (over: Partial<ReactionEvent> = {}): ReactionEvent => ({
    user_id: 'u2',
    channel_id: 'c1',
    message_id: 'm1',
    guild_id: 'g1',
    emoji: { id: null, name: 'x' },
    message_author_id: 'u1',
    ...over,
  });
  const rctx = (over: Partial<ReactionContext> = {}): ReactionContext => ({
    ...ctx(),
    enabled: true,
    silenced: false,
    ...over,
  });

  it('notifies for a reaction on my own message', () => {
    expect(reactionDecision(ev(), rctx())).toBe(true);
  });
  it('not when the message is not mine or the author is not known', () => {
    expect(reactionDecision(ev({ message_author_id: 'u3' }), rctx())).toBe(false);
    expect(reactionDecision(ev({ message_author_id: undefined }), rctx())).toBe(false);
  });
  it('not for my own reaction, nor a blocked or ignored account (D-717)', () => {
    expect(reactionDecision(ev({ user_id: 'u1' }), rctx())).toBe(false);
    const hidden = (blocked: string[], ignored: string[]) => ({
      hidden: { blocked: new Set(blocked), ignored: new Set(ignored) },
    });
    expect(reactionDecision(ev(), rctx(hidden(['u2'], [])))).toBe(false);
    expect(reactionDecision(ev(), rctx(hidden([], ['u2'])))).toBe(false);
    expect(reactionDecision(ev(), rctx(hidden(['u9'], ['u8'])))).toBe(true);
  });
  it('not when the preference is off, Streamer Mode silences, or I am in do-not-disturb', () => {
    expect(reactionDecision(ev(), rctx({ enabled: false }))).toBe(false);
    expect(reactionDecision(ev(), rctx({ silenced: true }))).toBe(false);
    expect(reactionDecision(ev(), rctx({ status: 'dnd' }))).toBe(false);
  });
  it('not while that channel is open in a focused window, but yes elsewhere', () => {
    expect(reactionDecision(ev(), rctx({ windowFocused: true, activeChannelId: 'c1' }))).toBe(
      false,
    );
    expect(reactionDecision(ev(), rctx({ windowFocused: true, activeChannelId: 'c9' }))).toBe(true);
    expect(reactionDecision(ev(), rctx({ windowFocused: false, activeChannelId: 'c1' }))).toBe(
      true,
    );
  });
  it('not for a muted server or channel', () => {
    expect(reactionDecision(ev(), rctx({ scope: scope({ muted: true }) }))).toBe(false);
    const channel = { muted: true, endsAt: null, level: null };
    expect(reactionDecision(ev(), rctx({ scope: scope({ channels: { c1: channel } }) }))).toBe(
      false,
    );
  });
});
