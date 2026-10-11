/**
 * Deterministic mock backend for the Lounge shell. Replaced by the real gateway-backed store (see data.ts).
 * Never imported by components, only by routes/lounge.tsx and tests.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  LoungeDataContext,
  type Channel,
  type DmChannel,
  type Guild,
  type LoungeActions,
  type LoungeData,
  type LoungeUser,
  type Member,
  type Message,
  type MessagesState,
  type Presence,
  type RailEntry,
  type Role,
  type Snowflake,
  type VoiceMember,
} from './data';

export const ME: LoungeUser = { id: '1', username: 'braxton', displayName: 'Braxton' };

const PEOPLE: Array<[string, string, string, boolean?]> = [
  ['2', 'mara', 'Mara'],
  ['3', 'doge', 'Doge'],
  ['4', 'ines', 'Ines'],
  ['5', 'kestrel', 'Kestrel'],
  ['6', 'orla', 'Orla'],
  ['7', 'pim', 'Pim'],
  ['8', 'sable', 'Sable'],
  ['9', 'tobias', 'Tobias'],
  ['10', 'door-bot', 'Doorman', true],
  ['11', 'wren', 'Wren'],
  ['12', 'yusuf', 'Yusuf'],
];

const USERS: Record<Snowflake, LoungeUser> = Object.fromEntries([
  [ME.id, ME],
  ...PEOPLE.map(([id, username, displayName, bot]) => [
    id,
    {
      id,
      username,
      displayName,
      bot: !!bot,
      ...(id === '3' ? { tag: 'PoE' } : {}),
    } satisfies LoungeUser,
  ]),
]);

const ALL_PERMS = { manageGuild: true, manageChannels: true, createInvite: true, moderate: true };
const BASIC_PERMS = {
  manageGuild: false,
  manageChannels: false,
  createInvite: true,
  moderate: false,
};

export interface MockGuildSeed {
  guild: Guild;
  channels: Channel[];
}

function ch(
  guildId: string,
  id: string,
  type: Channel['type'],
  name: string,
  position: number,
  extra: Partial<Channel> = {},
): Channel {
  return {
    id,
    guildId,
    type,
    name,
    position,
    unread: false,
    mentionCount: 0,
    canSend: true,
    ...extra,
  };
}

function buildGuilds(): MockGuildSeed[] {
  const g = (id: string, name: string, extra: Partial<Guild> = {}, perms = BASIC_PERMS): Guild => ({
    id,
    name,
    ownerId: '2',
    unread: false,
    mentionCount: 0,
    permissions: perms,
    ...extra,
  });
  return [
    {
      guild: g(
        '1001',
        'Side Room HQ',
        { ownerId: ME.id, isNew: true, voiceConnected: true },
        ALL_PERMS,
      ),
      channels: [
        ch('1001', '2000', 'category', 'Text Channels', 0),
        ch('1001', '2001', 'text', 'general', 1, {
          parentId: '2000',
          topic: 'Pull up a chair. Keep it civil.',
        }),
        ch('1001', '2002', 'text', 'watch-club', 2, {
          parentId: '2000',
          unread: true,
          lastReadMessageId: '2002-m110',
        }),
        ch('1001', '2003', 'text', 'reading-list', 3, {
          parentId: '2000',
          unread: true,
          mentionCount: 3,
        }),
        ch('1001', '2004', 'announcement', 'house-notes', 4, {
          parentId: '2000',
          canSend: false,
          topic: 'Read only.',
        }),
        ch('1001', '2005', 'text', 'off-topic', 5, { parentId: '2000', muted: true }),
        ch('1001', '2010', 'category', 'Voice Channels', 6),
        ch('1001', '2011', 'voice', 'General', 7, { parentId: '2010', userLimit: 10 }),
        ch('1001', '2012', 'voice', 'Movie night', 8, { parentId: '2010' }),
        ch('1001', '2013', 'category', 'Archive', 9),
        ch('1001', '2014', 'text', 'old-news', 10, { parentId: '2013' }),
      ],
    },
    {
      guild: g('1002', 'Watch Club', { unread: true }),
      channels: [
        ch('1002', '2101', 'text', 'general', 0, { unread: true }),
        ch('1002', '2102', 'text', 'recommendations', 1),
        ch('1002', '2103', 'voice', 'Screening room', 2),
      ],
    },
    {
      guild: g('1003', 'Reading Club', { mentionCount: 12, unread: true }),
      channels: [
        ch('1003', '2201', 'text', 'chapter-talk', 0, { unread: true, mentionCount: 12 }),
        ch('1003', '2202', 'text', 'spoilers', 1),
      ],
    },
    {
      guild: g('1004', 'Staff Server', { isStaff: true }, ALL_PERMS),
      channels: [
        ch('1004', '2301', 'text', 'staff-chat', 0),
        ch('1004', '2302', 'text', 'reports', 1),
      ],
    },
    {
      guild: g('1005', 'Gaming', { unread: true }),
      channels: [
        ch('1005', '2401', 'text', 'lfg', 0, { unread: true }),
        ch('1005', '2402', 'voice', 'Party', 1),
      ],
    },
    {
      guild: g('1006', 'Study Group'),
      channels: [ch('1006', '2501', 'text', 'notes', 0)],
    },
  ];
}

const SAMPLE_LINES = [
  'Has anyone seen the new episode yet?',
  'I **really** liked the ending, no spoilers though.',
  'Check `npm run build` before you push.',
  'lol',
  'Movie night at 9, <#2012> is open.',
  'Hey <@1>, did you get my message?',
  '> that is a bold take\nand I stand by it',
  'This one is ||a surprise||.',
  'Reminder: be kind to each other.',
  '```ts\nconst door = open("side-room");\n```',
  'brb',
  'Honestly *peak* cinema.',
  '~~scratch that~~ it was fine actually',
  'https://example.com/some/very/long/link/that/should/wrap/nicely/in/the/column',
];

function hash(n: number) {
  let x = (n + 0x9e3779b9) | 0;
  x = Math.imul(x ^ (x >>> 16), 0x85ebca6b);
  x = Math.imul(x ^ (x >>> 13), 0xc2b2ae35);
  return (x ^ (x >>> 16)) >>> 0;
}

/** Messages for one channel, ascending; `total` of them ending at `endTime`. */
export function generateHistory(channelId: string, total: number, endTime: number): Message[] {
  const seed = Number(channelId) || 1;
  const ids = ['2', '3', '4', '5', '1', '6', '10'];
  const out: Message[] = [];
  let t = endTime - total * 9 * 60_000;
  let author = ids[0]!;
  for (let i = 0; i < total; i++) {
    const h = hash(seed * 1000 + i);
    // Bursts by the same author (grouping), with the occasional long gap (date dividers).
    const sameAuthor = h % 3 !== 0;
    if (!sameAuthor) author = ids[h % ids.length]!;
    t += sameAuthor ? 20_000 + (h % 90_000) : 2 * 60_000 + (h % (40 * 60_000));
    if (h % 37 === 0) t += 20 * 3600_000;
    const msg: Message = {
      id: `${channelId}-m${100 + i}`,
      channelId,
      author: USERS[author]!,
      content: SAMPLE_LINES[(h >>> 4) % SAMPLE_LINES.length]!,
      createdAt: new Date(Math.min(t, endTime - (total - i) * 1000)).toISOString(),
      ...(h % 17 === 0 ? { editedAt: new Date(t + 60_000).toISOString() } : {}),
      ...(i > 3 && h % 11 === 0 ? { replyToId: `${channelId}-m${100 + i - 3}` } : {}),
      ...(h % 13 === 0
        ? { reactions: [{ emoji: '👍', count: 1 + (h % 4), me: h % 2 === 0 }] }
        : {}),
    };
    out.push(msg);
  }
  return out;
}

const PAGE = 50;
const HISTORY_TOTAL = 260;

export interface MockDataProviderProps {
  children: ReactNode;
  /** Pin "now" so tests are deterministic. */
  now?: number;
  /** Messages per text channel (default 260). */
  historySize?: number;
}

export function MockDataProvider({
  children,
  now,
  historySize = HISTORY_TOTAL,
}: MockDataProviderProps) {
  const nowRef = useRef(now ?? Date.now());
  const seeds = useMemo(buildGuilds, []);
  const history = useRef<Record<string, Message[]>>({});
  const nextId = useRef(1);

  const [channelsByGuild, setChannels] = useState<Record<string, Channel[]>>(() =>
    Object.fromEntries(seeds.map((s) => [s.guild.id, s.channels])),
  );
  const [guilds, setGuilds] = useState<Guild[]>(() => seeds.map((s) => s.guild));
  const [messages, setMessages] = useState<Record<string, MessagesState>>(() => {
    const out: Record<string, MessagesState> = {};
    for (const s of seeds) {
      for (const c of s.channels) {
        if (c.type === 'category' || c.type === 'voice') continue;
        const all = generateHistory(c.id, historySize, nowRef.current);
        history.current[c.id] = all;
        out[c.id] = {
          messages: all.slice(-PAGE),
          hasMore: all.length > PAGE,
          loadingOlder: false,
          loading: false,
        };
      }
    }
    return out;
  });
  const [dms, setDms] = useState<DmChannel[]>(() => [
    {
      id: '3001',
      recipientIds: ['2'],
      lastMessageAt: new Date(nowRef.current - 60_000).toISOString(),
      unread: true,
      mentionCount: 2,
    },
    {
      id: '3002',
      recipientIds: ['3'],
      lastMessageAt: new Date(nowRef.current - 3600_000).toISOString(),
      unread: false,
      mentionCount: 0,
    },
    {
      id: '3003',
      recipientIds: ['4', '5', '6'],
      name: 'Movie crew',
      lastMessageAt: new Date(nowRef.current - 86400_000).toISOString(),
      unread: false,
      mentionCount: 0,
    },
    {
      id: '3004',
      recipientIds: ['7'],
      lastMessageAt: new Date(nowRef.current - 3 * 86400_000).toISOString(),
      unread: false,
      mentionCount: 0,
    },
  ]);
  const [typing, setTyping] = useState<Record<string, string[]>>({ '2001': ['3'] });

  useEffect(() => {
    // DM histories live beside channel histories.
    setMessages((prev) => {
      const next = { ...prev };
      for (const d of dms) {
        if (next[d.id]) continue;
        const all = generateHistory(d.id, 60, nowRef.current).map((m, i) => ({
          ...m,
          author: i % 2 ? ME : USERS[d.recipientIds[0]!]!,
        }));
        history.current[d.id] = all;
        next[d.id] = {
          messages: all.slice(-PAGE),
          hasMore: all.length > PAGE,
          loadingOlder: false,
          loading: false,
        };
      }
      return next;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const rail: RailEntry[] = useMemo(
    () => [
      { kind: 'guild', guildId: '1001' },
      { kind: 'guild', guildId: '1002' },
      { kind: 'guild', guildId: '1003' },
      {
        kind: 'folder',
        id: 'f1',
        name: 'Games & study',
        color: '#5865f2',
        guildIds: ['1005', '1006'],
      },
      { kind: 'guild', guildId: '1004' },
    ],
    [],
  );

  const roles: Record<string, Role[]> = useMemo(
    () =>
      Object.fromEntries(
        seeds.map((s) => [
          s.guild.id,
          [
            { id: `${s.guild.id}-r1`, name: 'Staff', color: '#e0a82e', hoist: true, position: 3 },
            {
              id: `${s.guild.id}-r2`,
              name: 'Regulars',
              color: '#3ba55d',
              hoist: true,
              position: 2,
            },
            { id: `${s.guild.id}-r3`, name: 'Guests', color: null, hoist: false, position: 1 },
          ] satisfies Role[],
        ]),
      ),
    [seeds],
  );

  const membersByGuild: Record<string, Member[]> = useMemo(() => {
    const all = [ME, ...PEOPLE.map(([id]) => USERS[id]!)];
    const out: Record<string, Member[]> = {};
    for (const s of seeds) {
      const gid = s.guild.id;
      out[gid] = all.map((user, i) => ({
        user,
        guildId: gid,
        roleIds: [i % 4 === 0 ? `${gid}-r1` : i % 4 === 1 ? `${gid}-r2` : `${gid}-r3`],
      }));
    }
    return out;
  }, [seeds]);

  const presence: Record<string, Presence> = useMemo(
    () => ({
      '1': { status: 'online', activity: { kind: 'custom', text: 'Glooping my glooper' } },
      '2': { status: 'online', activity: { kind: 'watching', text: 'Cowboy Bebop' } },
      '3': { status: 'idle' },
      '4': { status: 'dnd', activity: { kind: 'playing', text: 'Path of Exile' } },
      '5': { status: 'online' },
      '6': { status: 'offline' },
      '7': { status: 'offline' },
      '8': { status: 'online', activity: { kind: 'reading', text: 'Berserk ch. 12' } },
      '9': { status: 'offline' },
      '10': { status: 'online' },
      '11': { status: 'idle' },
      '12': { status: 'offline' },
    }),
    [],
  );

  const voice: Record<string, VoiceMember[]> = useMemo(
    () => ({
      '2011': [{ userId: '1' }, { userId: '2', speaking: true }, { userId: '3', muted: true }],
      '2012': [{ userId: '5', video: true, streaming: true }],
    }),
    [],
  );

  const patchChannel = useCallback((channelId: string, patch: Partial<Channel>) => {
    setChannels((prev) => {
      const next: typeof prev = {};
      for (const [gid, list] of Object.entries(prev)) {
        next[gid] = list.map((c) => (c.id === channelId ? { ...c, ...patch } : c));
      }
      return next;
    });
  }, []);

  const recomputeGuild = useCallback((chs: Record<string, Channel[]>) => {
    setGuilds((prev) =>
      prev.map((g) => {
        const list = (chs[g.id] ?? []).filter((c) => !c.muted);
        return {
          ...g,
          unread: list.some((c) => c.unread),
          mentionCount: list.reduce((n, c) => n + c.mentionCount, 0),
        };
      }),
    );
  }, []);

  useEffect(() => recomputeGuild(channelsByGuild), [channelsByGuild, recomputeGuild]);

  const actions: LoungeActions = useMemo(
    () => ({
      sendMessage(channelId, content, replyToId) {
        const id = `${channelId}-local${nextId.current++}`;
        const msg: Message = {
          id,
          channelId,
          author: ME,
          content,
          createdAt: new Date().toISOString(),
          ...(replyToId ? { replyToId } : {}),
        };
        setMessages((prev) => {
          const cur = prev[channelId] ?? {
            messages: [],
            hasMore: false,
            loadingOlder: false,
            loading: false,
          };
          return { ...prev, [channelId]: { ...cur, messages: [...cur.messages, msg] } };
        });
        setDms((prev) =>
          prev.map((d) => (d.id === channelId ? { ...d, lastMessageAt: msg.createdAt } : d)),
        );
      },
      editMessage(channelId, messageId, content) {
        setMessages((prev) => {
          const cur = prev[channelId];
          if (!cur) return prev;
          return {
            ...prev,
            [channelId]: {
              ...cur,
              messages: cur.messages.map((m) =>
                m.id === messageId ? { ...m, content, editedAt: new Date().toISOString() } : m,
              ),
            },
          };
        });
      },
      loadOlder(channelId) {
        setMessages((prev) => {
          const cur = prev[channelId];
          if (!cur || cur.loadingOlder || !cur.hasMore) return prev;
          return { ...prev, [channelId]: { ...cur, loadingOlder: true } };
        });
        setTimeout(() => {
          setMessages((prev) => {
            const cur = prev[channelId];
            const all = history.current[channelId];
            if (!cur || !all) return prev;
            const first = cur.messages[0];
            const idx = first ? all.findIndex((m) => m.id === first.id) : all.length;
            const start = Math.max(0, idx - PAGE);
            return {
              ...prev,
              [channelId]: {
                ...cur,
                loadingOlder: false,
                hasMore: start > 0,
                messages: [...all.slice(start, idx), ...cur.messages],
              },
            };
          });
        }, 0);
      },
      markRead(channelId) {
        patchChannel(channelId, { unread: false, mentionCount: 0 });
        setDms((prev) =>
          prev.map((d) => (d.id === channelId ? { ...d, unread: false, mentionCount: 0 } : d)),
        );
      },
      markGuildRead(guildId) {
        setChannels((prev) => ({
          ...prev,
          [guildId]: (prev[guildId] ?? []).map((c) => ({ ...c, unread: false, mentionCount: 0 })),
        }));
      },
      toggleReaction(channelId, messageId, emoji) {
        setMessages((prev) => {
          const cur = prev[channelId];
          if (!cur) return prev;
          return {
            ...prev,
            [channelId]: {
              ...cur,
              messages: cur.messages.map((m) => {
                if (m.id !== messageId) return m;
                const list = m.reactions ?? [];
                const found = list.find((r) => r.emoji === emoji);
                const next = found
                  ? list
                      .map((r) =>
                        r.emoji === emoji
                          ? { ...r, me: !r.me, count: r.count + (r.me ? -1 : 1) }
                          : r,
                      )
                      .filter((r) => r.count > 0)
                  : [...list, { emoji, count: 1, me: true }];
                return { ...m, reactions: next };
              }),
            },
          };
        });
      },
      startTyping(channelId) {
        setTyping((prev) => ({ ...prev, [channelId]: prev[channelId] ?? [] }));
      },
    }),
    [patchChannel],
  );

  const value: LoungeData = {
    me: ME,
    guilds,
    rail,
    channelsByGuild,
    dms,
    users: USERS,
    rolesByGuild: roles,
    membersByGuild,
    presence,
    typing,
    voice,
    messages,
    friendRequestCount: 2,
    messageRequestCount: 1,
    actions,
  };
  return <LoungeDataContext.Provider value={value}>{children}</LoungeDataContext.Provider>;
}
