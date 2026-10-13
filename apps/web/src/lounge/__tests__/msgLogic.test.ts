import { prefixLines } from '../composerText';
import type { Channel, Member, Role } from '../data';
import { toLoungeMessage } from '../liveMap';
import {
  applySuggestion,
  channelSuggestions,
  emojiSuggestions,
  findTrigger,
  mentionSuggestions,
  resolveTokens,
} from '../msg/autocomplete';
import { formatBytes, formatDuration, isHttpUrl, timeLeft } from '../msg/format';
import { validatePoll } from '../msg/PollModal';
import { runSlash } from '../msg/slashCommands';
import { uploadName } from '../msg/upload';
import { waveformBars } from '../msg/Attachments';
import { buildRows } from '../messageRows';
import { emptyData, reduceDispatch, type StoreData } from '../../store/reducer';
import type * as S from '../../store/types';

const user = (id: string, username: string, displayName = username) => ({
  id,
  username,
  displayName,
});
const member = (id: string, name: string, nick?: string): Member => ({
  user: user(id, name),
  guildId: 'g',
  nick: nick ?? null,
  roleIds: [],
});

describe('composer trigger detection', () => {
  it('finds @, #, : and a leading / under the caret', () => {
    expect(findTrigger('hi @bo', 6)).toEqual({ kind: '@', query: 'bo', start: 3 });
    expect(findTrigger('see #gen', 8)).toMatchObject({ kind: '#', query: 'gen' });
    expect(findTrigger('x :fir', 6)).toMatchObject({ kind: ':', query: 'fir' });
    expect(findTrigger('/sh', 3)).toMatchObject({ kind: '/', query: 'sh' });
  });

  it('ignores mid-word triggers, times, and slashes after the start', () => {
    expect(findTrigger('mail me@x', 9)).toBeNull();
    expect(findTrigger('at 10:30', 8)).toBeNull();
    expect(findTrigger('a :f', 4)).toBeNull(); // one character is not enough for emoji
    expect(findTrigger('go /sh', 6)).toBeNull();
    expect(findTrigger('done @bob ', 10)).toBeNull();
  });
});

describe('suggestions', () => {
  const roles: Role[] = [{ id: 'r1', name: 'Staff', color: '#f00', hoist: true, position: 2 }];
  const members = [member('1', 'braxton'), member('2', 'bob', 'Bobby'), member('3', 'alice')];

  it('ranks exact and prefix matches first, and matches nicknames', () => {
    const out = mentionSuggestions('b', members, roles, { everyone: false });
    expect(out.map((s) => s.label)).toEqual(['Bobby', 'braxton']);
    expect(out[0]!.token).toBe('<@2>');
  });

  it('includes roles, and @everyone only when allowed', () => {
    expect(
      mentionSuggestions('st', members, roles, { everyone: false }).map((s) => s.token),
    ).toEqual(['<@&r1>']);
    expect(mentionSuggestions('eve', members, roles, { everyone: false })).toEqual([]);
    expect(mentionSuggestions('eve', members, roles, { everyone: true })[0]).toMatchObject({
      label: '@everyone',
    });
  });

  it('suggests text and voice channels as <#id> tokens', () => {
    const ch = (id: string, name: string, type: Channel['type']) =>
      ({ id, name, type, guildId: 'g' }) as Channel;
    const out = channelSuggestions('gen', [
      ch('1', 'general', 'text'),
      ch('2', 'General Voice', 'voice'),
      ch('3', 'gen-cat', 'category'),
    ]);
    expect(out.map((s) => s.token)).toEqual(['<#1>', '<#2>']);
  });

  it('suggests emoji by name', () => {
    const all = [
      ['fire', '🔥'],
      ['fireworks', '🎆'],
      ['firefighter', '🧑‍🚒'],
    ] as const;
    expect(emojiSuggestions('fire', all)[0]).toMatchObject({ insert: '🔥' });
  });

  it('applies a suggestion and later resolves readable text to wire tokens', () => {
    const text = 'hi @bo';
    const trigger = findTrigger(text, text.length)!;
    const [s] = mentionSuggestions('bo', members, roles, { everyone: false });
    const r = applySuggestion(text, trigger, text.length, s!);
    expect(r.value).toBe('hi @Bobby ');
    expect(r.caret).toBe(r.value.length);
    const tokens = new Map([[s!.insert, s!.token!]]);
    expect(resolveTokens('hi @Bobby and @Bobbyx', tokens)).toBe('hi <@2> and @Bobbyx');
  });
});

describe('slash commands', () => {
  it('expands text commands', () => {
    expect(runSlash('/shrug')).toEqual({ kind: 'send', content: '¯\\_(ツ)_/¯' });
    expect(runSlash('/shrug hmm')).toEqual({ kind: 'send', content: 'hmm ¯\\_(ツ)_/¯' });
    expect(runSlash('/me waves')).toEqual({ kind: 'send', content: '_waves_' });
    expect(runSlash('/spoiler boo')).toEqual({ kind: 'send', content: '||boo||' });
    expect(runSlash('/tableflip')).toMatchObject({ kind: 'send' });
    expect(runSlash('/unflip')).toMatchObject({ kind: 'send' });
  });

  it('opens the poll dialog, rejects empty arguments, ignores unknown commands', () => {
    expect(runSlash('/poll')).toEqual({ kind: 'poll' });
    expect(runSlash('/me')).toMatchObject({ kind: 'error' });
    expect(runSlash('/nope x')).toEqual({ kind: 'none' });
    expect(runSlash('plain')).toEqual({ kind: 'none' });
  });
});

describe('small helpers', () => {
  it('formats sizes, durations and time left', () => {
    expect(formatBytes(900)).toBe('900 B');
    expect(formatBytes(1536)).toBe('1.5 KB');
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
    expect(formatDuration(83.2)).toBe('1:23');
    const now = Date.parse('2026-01-01T00:00:00Z');
    expect(timeLeft('2026-01-01T03:00:00Z', now)).toBe('3 hours left');
    expect(timeLeft('2025-12-31T00:00:00Z', now)).toBe('Poll closed');
    expect(timeLeft(null, now)).toBeNull();
  });

  it('only lets http(s) URLs through', () => {
    expect(isHttpUrl('https://a.example/x')).toBe(true);
    expect(isHttpUrl('javascript:alert(1)')).toBe(false);
    expect(isHttpUrl('data:text/html,hi')).toBe(false);
    expect(isHttpUrl(undefined)).toBe(false);
  });

  it('prefixes spoiler uploads and prefixes block quote lines', () => {
    expect(uploadName('a.png', true)).toBe('SPOILER_a.png');
    expect(uploadName('SPOILER_a.png', false)).toBe('a.png');
    expect(prefixLines('one\ntwo', 0, 7, '> ').value).toBe('> one\n> two');
    expect(prefixLines('> one\n> two', 0, 11, '> ').value).toBe('one\ntwo');
  });

  it('resamples waveform bytes to a fixed number of bars', () => {
    const bars = waveformBars([0, 255, 128], 6);
    expect(bars).toHaveLength(6);
    expect(Math.max(...bars)).toBe(1);
    expect(waveformBars(null, 4)).toEqual([0.3, 0.3, 0.3, 0.3]);
  });

  it('validates polls', () => {
    const ok = {
      question: 'Q?',
      answers: [{ text: 'a' }, { text: 'b' }],
      durationHours: 24,
      multi: false,
    };
    expect(validatePoll(ok)).toBeNull();
    expect(validatePoll({ ...ok, question: ' ' })).toMatch(/question/i);
    expect(validatePoll({ ...ok, answers: [{ text: 'a' }, { text: ' ' }] })).toMatch(/two answers/);
    expect(validatePoll({ ...ok, answers: [{ text: 'a' }, { text: 'x'.repeat(56) }] })).toMatch(
      /55/,
    );
  });
});

describe('store message mapping', () => {
  const base: S.Message = {
    id: '10',
    channel_id: 'c',
    author: { id: '2', username: 'bob' },
    content: 'hi',
    timestamp: '2026-01-01T00:00:00Z',
  };

  it('maps attachments, voice messages, spoilers and expiry', () => {
    const m = toLoungeMessage({
      ...base,
      flags: 8192 | 4,
      attachments: [
        {
          id: 'a',
          filename: 'SPOILER_x.png',
          size: 1,
          url: '/u',
          content_type: 'image/png',
          waveform: btoa(String.fromCharCode(1, 2, 3)),
          duration_secs: 3,
          expires_at: '2020-01-01T00:00:00Z',
        },
      ],
    });
    expect(m.voice).toBe(true);
    expect(m.suppressEmbeds).toBe(true);
    expect(m.attachments![0]).toMatchObject({
      spoiler: true,
      waveform: [1, 2, 3],
      durationSecs: 3,
      expired: true,
    });
  });

  it('maps polls, stickers, mentions and pins', () => {
    const m = toLoungeMessage({
      ...base,
      pinned: true,
      type: 6,
      mentions: [{ id: '1', username: 'me' }],
      mention_roles: ['r'],
      mention_everyone: true,
      sticker_items: [{ id: '9', name: 'wave', format_type: 1 }],
      poll: {
        question: { text: 'Q' },
        answers: [{ answer_id: 1, poll_media: { text: 'A', emoji: { name: '🍟' } } }],
        results: { is_finalized: true, answer_counts: [{ id: 1, count: 4, me_voted: true }] },
      },
    });
    expect(m).toMatchObject({
      pinned: true,
      type: 6,
      mentionIds: ['1'],
      mentionRoleIds: ['r'],
      mentionEveryone: true,
      stickers: [{ id: '9', name: 'wave', formatType: 1 }],
      poll: {
        question: 'Q',
        finalized: true,
        answers: [{ id: 1, count: 4, me: true, emoji: '🍟' }],
      },
    });
  });

  it('treats a forward as a snapshot, not a reply', () => {
    const m = toLoungeMessage({
      ...base,
      message_reference: { message_id: '5', type: 1 },
      message_snapshots: [{ message: { content: 'orig', timestamp: '2025-01-01T00:00:00Z' } }],
    });
    expect(m.replyToId).toBeNull();
    expect(m.forwarded).toMatchObject({ content: 'orig', createdAt: '2025-01-01T00:00:00Z' });
  });
});

describe('gateway events for reactions and poll votes', () => {
  const withMessage = (extra: Partial<S.Message> = {}): StoreData => ({
    ...emptyData(),
    currentUserId: '1',
    messages: {
      '10': {
        id: '10',
        channel_id: 'c',
        author: { id: '2', username: 'bob' },
        content: 'x',
        timestamp: 't',
        ...extra,
      } as S.Message,
    },
  });

  it('adds and removes other people reactions', () => {
    let s = withMessage();
    const ev = (name: string, user_id: string) => ({
      message_id: '10',
      channel_id: 'c',
      user_id,
      emoji: { id: null, name },
    });
    s = { ...s, ...reduceDispatch(s, 'MESSAGE_REACTION_ADD', ev('🔥', '2')) };
    s = { ...s, ...reduceDispatch(s, 'MESSAGE_REACTION_ADD', ev('🔥', '3')) };
    expect(s.messages['10']!.reactions).toEqual([
      { emoji: { id: null, name: '🔥' }, count: 2, me: false },
    ]);
    s = { ...s, ...reduceDispatch(s, 'MESSAGE_REACTION_REMOVE', ev('🔥', '3')) };
    expect(s.messages['10']!.reactions![0]!.count).toBe(1);
    s = { ...s, ...reduceDispatch(s, 'MESSAGE_REACTION_REMOVE_EMOJI', ev('🔥', '2')) };
    expect(s.messages['10']!.reactions).toEqual([]);
  });

  it("does not double count the viewer's own reaction already applied optimistically", () => {
    const s = withMessage({ reactions: [{ emoji: { id: null, name: '👍' }, count: 1, me: true }] });
    const next = reduceDispatch(s, 'MESSAGE_REACTION_ADD', {
      message_id: '10',
      user_id: '1',
      emoji: { id: null, name: '👍' },
    });
    expect(next.messages!['10']!.reactions![0]).toMatchObject({ count: 1, me: true });
  });

  it('removes all reactions', () => {
    const s = withMessage({
      reactions: [{ emoji: { id: null, name: '👍' }, count: 3, me: false }],
    });
    const next = reduceDispatch(s, 'MESSAGE_REACTION_REMOVE_ALL', { message_id: '10' });
    expect(next.messages!['10']!.reactions).toEqual([]);
  });

  it('tallies poll votes from the gateway', () => {
    const s = withMessage({
      poll: {
        question: { text: 'q' },
        answers: [{ answer_id: 1, poll_media: { text: 'a' } }],
        results: { is_finalized: false, answer_counts: [{ id: 1, count: 1, me_voted: false }] },
      },
    });
    const add = reduceDispatch(s, 'MESSAGE_POLL_VOTE_ADD', {
      message_id: '10',
      user_id: '1',
      answer_id: 1,
    });
    expect(add.messages!['10']!.poll!.results!.answer_counts[0]).toEqual({
      id: 1,
      count: 2,
      me_voted: true,
    });
    const rm = reduceDispatch({ ...s, ...add }, 'MESSAGE_POLL_VOTE_REMOVE', {
      message_id: '10',
      user_id: '3',
      answer_id: 1,
    });
    expect(rm.messages!['10']!.poll!.results!.answer_counts[0]).toMatchObject({
      count: 1,
      me_voted: true,
    });
  });
});

describe('message rows', () => {
  const m = (id: string, author: string, iso: string, extra = {}) => ({
    id,
    channelId: 'c',
    author: user(author, author),
    content: id,
    createdAt: iso,
    ...extra,
  });

  it('groups by author within seven minutes, breaks on replies and across days, and marks NEW', () => {
    const rows = buildRows(
      [
        m('1', 'a', '2026-01-01T10:00:00Z'),
        m('2', 'a', '2026-01-01T10:03:00Z'),
        m('3', 'a', '2026-01-01T10:20:00Z'),
        m('4', 'a', '2026-01-01T10:21:00Z', { replyToId: '1' }),
        m('5', 'b', '2026-01-01T10:22:00Z'),
        m('6', 'b', '2026-01-02T10:22:00Z'),
      ],
      { lastReadId: '4', atStart: true, selfId: 'me' },
    );
    const kinds = rows.map((r) =>
      r.kind === 'message' ? `${r.message.id}${r.grouped ? 'g' : ''}` : r.kind,
    );
    expect(kinds).toEqual(['welcome', 'date', '1', '2g', '3', '4', 'new', '5', 'date', '6']);
  });
});
