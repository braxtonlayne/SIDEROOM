import { afterEach, describe, expect, it } from 'vitest';
import { groupChannels, navigableChannels, step } from '../channels';
import { insertAt, toggleWrap, typingLabel } from '../composerText';
import type { Channel, Member, Message, Presence, Role } from '../data';
import { isJumbo, parseMarkdown, setMarkdownEngine, testFallbackEngine } from '../markdown';
import { buildMemberRows } from '../memberGroups';
import { buildRows } from '../messageRows';
import {
  SHORTCUTS,
  isDrawerShortcut,
  isPlayerShortcut,
  matchCustomShortcut,
  matchShortcut,
} from '../shortcuts';
import { buildSwitcherItems, fuzzyScore, searchSwitcher } from '../switcherSearch';

const user = (id: string, name = id) => ({ id, username: name, displayName: name });
const msg = (id: string, author: string, at: string, extra: Partial<Message> = {}): Message => ({
  id,
  channelId: 'c',
  author: user(author),
  content: id,
  createdAt: at,
  ...extra,
});

describe('buildRows', () => {
  it('groups consecutive messages by the same author within 7 minutes', () => {
    const rows = buildRows(
      [
        msg('a', 'u1', '2026-03-01T10:00:00Z'),
        msg('b', 'u1', '2026-03-01T10:06:00Z'),
        msg('c', 'u1', '2026-03-01T10:14:00Z'),
        msg('d', 'u2', '2026-03-01T10:14:30Z'),
      ],
      { atStart: false },
    );
    const grouped = rows
      .filter((r) => r.kind === 'message')
      .map((r) => r.kind === 'message' && r.grouped);
    expect(grouped).toEqual([false, true, false, false]);
  });

  it('does not group replies, and breaks groups at date dividers', () => {
    const rows = buildRows(
      [
        msg('a', 'u1', '2026-03-01T10:00:00Z'),
        msg('b', 'u1', '2026-03-01T10:01:00Z', { replyToId: 'a' }),
        msg('c', 'u1', '2026-03-02T10:02:00Z'),
      ],
      { atStart: true },
    );
    expect(rows.map((r) => r.kind)).toEqual([
      'welcome',
      'date',
      'message',
      'message',
      'date',
      'message',
    ]);
    expect(
      rows.filter((r) => r.kind === 'message').map((r) => r.kind === 'message' && r.grouped),
    ).toEqual([false, false, false]);
  });

  it('inserts a NEW divider after the last read message, but not before own messages', () => {
    const list = [msg('a', 'u1', '2026-03-01T10:00:00Z'), msg('b', 'u2', '2026-03-01T10:01:00Z')];
    expect(buildRows(list, { atStart: false, lastReadId: 'a' }).map((r) => r.kind)).toEqual([
      'date',
      'message',
      'new',
      'message',
    ]);
    expect(
      buildRows(list, { atStart: false, lastReadId: 'a', selfId: 'u2' }).some(
        (r) => r.kind === 'new',
      ),
    ).toBe(false);
  });
});

describe('composer text', () => {
  it('wraps, unwraps and handles empty selections', () => {
    expect(toggleWrap('hello world', 0, 5, '**')).toEqual({
      value: '**hello** world',
      selStart: 2,
      selEnd: 7,
    });
    expect(toggleWrap('**hello** world', 2, 7, '**').value).toBe('hello world');
    expect(toggleWrap('', 0, 0, '*')).toEqual({ value: '**', selStart: 1, selEnd: 1 });
  });
  it('inserts at the caret', () => {
    expect(insertAt('ab', 1, 1, '😀')).toEqual({ value: 'a😀b', selStart: 3, selEnd: 3 });
  });
  it('labels typing', () => {
    expect(typingLabel([])).toBeNull();
    expect(typingLabel(['A'])).toBe('A is typing…');
    expect(typingLabel(['A', 'B'])).toBe('A and B are typing…');
    expect(typingLabel(['A', 'B', 'C', 'D'])).toBe('Several people are typing…');
  });
});

describe('markdown engine seam', () => {
  afterEach(() => setMarkdownEngine(testFallbackEngine));

  it('renders plain text, with line breaks, until the WASM engine is ready', () => {
    setMarkdownEngine(null);
    expect(parseMarkdown('**a**\nb')).toEqual([
      { type: 'text', text: '**a**' },
      { type: 'line_break' },
      { type: 'text', text: 'b' },
    ]);
    expect(isJumbo('😀')).toBe(false);
  });

  it('delegates parsing and jumbo detection to the active engine', () => {
    setMarkdownEngine({
      parse: () => [{ type: 'everyone' }],
      isJumbo: (t) => t === '🎉',
      expandShortcodes: (t) => t,
    });
    expect(parseMarkdown('anything')).toEqual([{ type: 'everyone' }]);
    expect(isJumbo('🎉')).toBe(true);
    expect(isJumbo('x')).toBe(false);
  });
});

describe('markdown test stand-in', () => {
  it('parses common Discord syntax', () => {
    const nodes = parseMarkdown('hi **bold** *it* `code` <@5> ||s|| <#7>');
    const types = nodes.map((n) => n.type);
    expect(types).toEqual([
      'text',
      'bold',
      'text',
      'italic',
      'text',
      'inline_code',
      'text',
      'user_mention',
      'text',
      'spoiler',
      'text',
      'channel_mention',
    ]);
  });
  it('parses code blocks and quotes', () => {
    expect(parseMarkdown('```ts\nlet a\n```')[0]).toEqual({
      type: 'code_block',
      lang: 'ts',
      code: 'let a',
    });
    expect(parseMarkdown('> hi')[0]?.type).toBe('quote');
  });
  it('does not treat plain text as jumbo', () => {
    expect(isJumbo('hello')).toBe(false);
  });
});

describe('channels', () => {
  const c = (
    id: string,
    type: Channel['type'],
    position: number,
    extra: Partial<Channel> = {},
  ): Channel => ({
    id,
    guildId: 'g',
    type,
    name: id,
    position,
    unread: false,
    mentionCount: 0,
    canSend: true,
    ...extra,
  });
  const list = [
    c('cat', 'category', 0),
    c('t1', 'text', 1, { parentId: 'cat' }),
    c('v1', 'voice', 2, { parentId: 'cat' }),
    c('m', 'text', 3, { parentId: 'cat', muted: true }),
  ];
  it('groups by category and can hide muted', () => {
    expect(groupChannels(list)[0]?.channels.map((x) => x.id)).toEqual(['t1', 'v1', 'm']);
    expect(groupChannels(list, { hideMuted: true })[0]?.channels.map((x) => x.id)).toEqual([
      't1',
      'v1',
    ]);
    expect(
      groupChannels(list, { hideMuted: true, activeId: 'm' })[0]?.channels.map((x) => x.id),
    ).toContain('m');
  });
  it('navigates text channels only and wraps', () => {
    const nav = navigableChannels(list);
    expect(nav.map((x) => x.id)).toEqual(['t1', 'm']);
    expect(step(nav, 1, 1)?.id).toBe('t1');
    expect(step(nav, -1, -1)?.id).toBe('m');
  });
});

describe('member groups', () => {
  const roles: Role[] = [
    { id: 'r1', name: 'Staff', color: '#f00', hoist: true, position: 2 },
    { id: 'r2', name: 'Regulars', color: null, hoist: false, position: 1 },
  ];
  const mem = (id: string, roleIds: string[]): Member => ({
    user: user(id, id),
    guildId: 'g',
    roleIds,
  });
  const presence: Record<string, Presence> = {
    a: { status: 'online' },
    b: { status: 'online' },
    c: { status: 'offline' },
    d: { status: 'idle' },
  };
  it('groups hoisted roles, then Online, then Offline', () => {
    const rows = buildMemberRows(
      [mem('a', ['r1']), mem('b', ['r2']), mem('c', ['r1']), mem('d', [])],
      roles,
      presence,
    );
    expect(
      rows
        .filter((r) => r.kind === 'header')
        .map((r) => r.kind === 'header' && `${r.label} ${r.count}`),
    ).toEqual(['Staff 1', 'Online 2', 'Offline 1']);
  });
  it('can hide offline', () => {
    const rows = buildMemberRows([mem('c', ['r1'])], roles, presence, { hideOffline: true });
    expect(rows).toEqual([]);
  });
});

describe('quick switcher search', () => {
  const perms = { manageGuild: false, manageChannels: false, createInvite: false, moderate: false };
  const items = buildSwitcherItems({
    guilds: [
      {
        id: 'g1',
        name: 'Side Room HQ',
        ownerId: '1',
        unread: false,
        mentionCount: 0,
        permissions: perms,
      },
    ],
    channelsByGuild: {
      g1: [
        {
          id: 'c1',
          guildId: 'g1',
          type: 'text',
          name: 'general',
          position: 0,
          unread: false,
          mentionCount: 0,
          canSend: true,
        },
        {
          id: 'c2',
          guildId: 'g1',
          type: 'voice',
          name: 'Movie night',
          position: 1,
          unread: false,
          mentionCount: 0,
          canSend: true,
        },
      ],
    },
    dms: [{ id: 'd1', recipientIds: ['2'], lastMessageAt: '', unread: false, mentionCount: 0 }],
    users: { '1': user('1', 'me'), '2': user('2', 'Mara') },
    meId: '1',
  });
  it('scores prefix > substring > subsequence', () => {
    expect(fuzzyScore('gen', 'general')!).toBeGreaterThan(fuzzyScore('era', 'general')!);
    expect(fuzzyScore('gnl', 'general')).not.toBeNull();
    expect(fuzzyScore('zzz', 'general')).toBeNull();
  });
  it('honours prefixes', () => {
    expect(searchSwitcher(items, '#gen', []).map((i) => i.label)).toEqual(['general']);
    expect(searchSwitcher(items, '!movie', []).map((i) => i.label)).toEqual(['Movie night']);
    expect(searchSwitcher(items, '*side', []).map((i) => i.kind)).toEqual(['guild']);
    expect(new Set(searchSwitcher(items, '@mar', []).map((i) => i.kind))).toEqual(
      new Set(['dm', 'user']),
    );
  });
  it('shows recents on an empty query', () => {
    expect(searchSwitcher(items, '', ['ch:c1'])[0]?.label).toBe('general');
  });
});

describe('the drawer owns Mod+K (UTL-SHL-001)', () => {
  const base = { ctrlKey: false, metaKey: false, altKey: false, shiftKey: false };
  it('treats the quickSwitcher id as a drawer shortcut, so the Lounge shell leaves it to the drawer gate', () => {
    expect(isDrawerShortcut('quickSwitcher')).toBe(true);
    expect(isDrawerShortcut('shortcutSheet')).toBe(false);
    expect(isDrawerShortcut('userSettings')).toBe(false);
    expect(isPlayerShortcut('quickSwitcher')).toBe(false);
  });
  it('still maps Ctrl+K and Cmd+K, and honours a rebind', () => {
    expect(matchCustomShortcut({ ...base, key: 'k', ctrlKey: true }, {})).toBe('quickSwitcher');
    expect(
      matchCustomShortcut({ ...base, key: 'j', ctrlKey: true }, { quickSwitcher: 'Mod+J' }),
    ).toBe('quickSwitcher');
    expect(
      matchCustomShortcut({ ...base, key: 'k', ctrlKey: true }, { quickSwitcher: 'Mod+J' }),
    ).toBe(null);
  });
  it('describes the shortcut as the drawer', () => {
    expect(SHORTCUTS.find((d) => d.id === 'quickSwitcher')?.description).toMatch(/drawer/i);
  });
});

describe('matchShortcut', () => {
  const base = { ctrlKey: false, metaKey: false, altKey: false, shiftKey: false };
  it('maps keys', () => {
    expect(matchShortcut({ ...base, key: 'k', ctrlKey: true })).toBe('quickSwitcher');
    expect(matchShortcut({ ...base, key: 'K', metaKey: true })).toBe('quickSwitcher');
    expect(matchShortcut({ ...base, key: '/', ctrlKey: true })).toBe('shortcutSheet');
    expect(matchShortcut({ ...base, key: 'ArrowDown', altKey: true })).toBe('nextChannel');
    expect(matchShortcut({ ...base, key: 'ArrowUp', altKey: true, shiftKey: true })).toBe(
      'prevUnread',
    );
    expect(matchShortcut({ ...base, key: 'ArrowDown', altKey: true, ctrlKey: true })).toBe(
      'nextGuild',
    );
    expect(matchShortcut({ ...base, key: 'Escape' })).toBe('markRead');
    // Any printable key focuses the message box (Q169); Ctrl+Shift+M is Discord's mute and is not ours.
    expect(matchShortcut({ ...base, key: 'a' })).toBe('focusComposer');
    expect(matchShortcut({ ...base, key: 'M', ctrlKey: true, shiftKey: true })).toBeNull();
    expect(matchShortcut({ ...base, key: 'Shift', shiftKey: true })).toBeNull();
  });
});
