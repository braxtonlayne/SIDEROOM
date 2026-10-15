/**
 * Built-in slash commands (LNG-CMP-015). Text commands expand locally; `poll` opens the poll dialog; `nick`,
 * `thread`, `msg`, `remind` and `tts` return an action for the composer to run. There is no GIF provider
 * (LNG-EXP-005), so there is no /giphy.
 */

export interface SlashCommand {
  name: string;
  description: string;
  /** Takes an argument (shown as a hint, kept in the box when picked). */
  arg?: string;
  /** Only offered where this holds. */
  needs?: 'guild' | 'channel' | 'tts';
}

export const SLASH_COMMANDS: readonly SlashCommand[] = [
  { name: 'shrug', description: 'Appends ¯\\_(ツ)_/¯ to your message', arg: 'message' },
  { name: 'tableflip', description: 'Appends (╯°□°）╯︵ ┻━┻ to your message', arg: 'message' },
  { name: 'unflip', description: 'Appends ┬─┬ ノ( ゜-゜ノ) to your message', arg: 'message' },
  { name: 'me', description: 'Displays text with emphasis', arg: 'message' },
  { name: 'spoiler', description: 'Marks your message as a spoiler', arg: 'message' },
  { name: 'poll', description: 'Create a poll' },
  { name: 'tts', description: 'Sends a message that is read aloud', arg: 'message', needs: 'tts' },
  {
    name: 'nick',
    description: 'Change your nickname in this server',
    arg: 'nickname',
    needs: 'guild',
  },
  { name: 'thread', description: 'Start a thread in this channel', arg: 'name', needs: 'channel' },
  { name: 'msg', description: 'Send someone a direct message', arg: '@user message' },
  { name: 'remind', description: 'Remind yourself later (for example 1h30m)', arg: 'time message' },
  { name: 'rank', description: 'Show your level and the leaderboard', needs: 'guild' },
  { name: 'suggest', description: 'Post a suggestion to this server', needs: 'guild' },
  { name: 'ticket', description: 'Open a ticket with the staff', needs: 'guild' },
];

const SHRUG = '¯\\_(ツ)_/¯';
const TABLEFLIP = '(╯°□°）╯︵ ┻━┻';
const UNFLIP = '┬─┬ ノ( ゜-゜ノ)';

/** Where the composer is, for commands that only make sense in some places. */
export interface SlashContext {
  /** In a server (not a DM). */
  guild: boolean;
  /** In a channel that can hold threads (not a DM, not already a thread). */
  threads: boolean;
  /** The viewer may use /tts here. */
  tts: boolean;
}

export type SlashResult =
  | { kind: 'send'; content: string; tts?: boolean }
  | { kind: 'poll' }
  | { kind: 'none' }
  | { kind: 'error'; message: string }
  | { kind: 'nick'; nick: string | null }
  | { kind: 'thread'; name: string }
  | { kind: 'msg'; userId: string; content: string }
  | { kind: 'remind'; ms: number; text: string }
  | { kind: 'community'; tab: 'rank' | 'suggestions' | 'tickets' };

const UNITS: Record<string, number> = {
  s: 1000,
  m: 60_000,
  h: 3_600_000,
  d: 86_400_000,
  w: 7 * 86_400_000,
};
const UNIT_WORDS: Record<string, string> = {
  sec: 's',
  secs: 's',
  second: 's',
  seconds: 's',
  min: 'm',
  mins: 'm',
  minute: 'm',
  minutes: 'm',
  hr: 'h',
  hrs: 'h',
  hour: 'h',
  hours: 'h',
  day: 'd',
  days: 'd',
  week: 'w',
  weeks: 'w',
};

/** "90m", "1h30m", "in 2 hours" at the start of `text` -> milliseconds and the rest of the text. */
export function parseDuration(text: string): { ms: number; rest: string } | null {
  let s = text.trim().replace(/^in\s+/i, '');
  let ms = 0;
  let matched = false;
  for (;;) {
    const m = /^(\d+(?:\.\d+)?)\s*([a-z]+)\s*/i.exec(s);
    if (!m) break;
    const unit = UNITS[m[2]!.toLowerCase()] ? m[2]!.toLowerCase() : UNIT_WORDS[m[2]!.toLowerCase()];
    const factor = unit ? UNITS[unit] : undefined;
    if (!factor) break;
    ms += Number(m[1]) * factor;
    matched = true;
    s = s.slice(m[0].length);
  }
  return matched && ms > 0 ? { ms: Math.round(ms), rest: s.trim() } : null;
}

/** Longest reminder: a year. */
export const MAX_REMIND_MS = 365 * 86_400_000;
export const MIN_REMIND_MS = 5_000;

/** Evaluates a message that starts with `/`. Unknown commands are sent as plain text. */
export function runSlash(
  text: string,
  ctx: SlashContext = { guild: true, threads: true, tts: true },
): SlashResult {
  const m = /^\/([a-z]+)(?:\s+([\s\S]*))?$/.exec(text.trim());
  if (!m) return { kind: 'none' };
  const name = m[1]!;
  const arg = (m[2] ?? '').trim();
  const withSuffix = (suffix: string) => ({
    kind: 'send' as const,
    content: arg ? `${arg} ${suffix}` : suffix,
  });
  switch (name) {
    case 'shrug':
      return withSuffix(SHRUG);
    case 'tableflip':
      return withSuffix(TABLEFLIP);
    case 'unflip':
      return withSuffix(UNFLIP);
    case 'me':
      return arg
        ? { kind: 'send', content: `_${arg.replace(/_/g, '\\_')}_` }
        : { kind: 'error', message: '/me needs a message.' };
    case 'spoiler':
      return arg
        ? { kind: 'send', content: `||${arg}||` }
        : { kind: 'error', message: '/spoiler needs a message.' };
    case 'poll':
      return { kind: 'poll' };
    case 'tts':
      if (!ctx.tts)
        return { kind: 'error', message: 'You do not have permission to use /tts here.' };
      return arg
        ? { kind: 'send', content: arg, tts: true }
        : { kind: 'error', message: '/tts needs a message.' };
    case 'nick':
      if (!ctx.guild) return { kind: 'error', message: '/nick only works in a server.' };
      if (arg.length > 32)
        return { kind: 'error', message: 'Nicknames are 32 characters or fewer.' };
      // No argument resets the nickname.
      return { kind: 'nick', nick: arg || null };
    case 'thread':
      if (!ctx.threads) return { kind: 'error', message: 'You cannot start a thread here.' };
      return arg
        ? { kind: 'thread', name: arg.slice(0, 100) }
        : { kind: 'error', message: '/thread needs a name.' };
    case 'msg': {
      const who = /^<@!?(\w+)>\s*([\s\S]*)$/.exec(arg);
      if (!who)
        return { kind: 'error', message: '/msg needs a person: pick them from the @ list.' };
      return who[2]!.trim()
        ? { kind: 'msg', userId: who[1]!, content: who[2]!.trim() }
        : { kind: 'error', message: '/msg needs a message after the name.' };
    }
    case 'remind': {
      const d = parseDuration(arg);
      if (!d)
        return { kind: 'error', message: '/remind needs a time such as 30m or 2h, then a note.' };
      if (d.ms < MIN_REMIND_MS || d.ms > MAX_REMIND_MS)
        return { kind: 'error', message: 'Reminders can be 5 seconds to a year away.' };
      return { kind: 'remind', ms: d.ms, text: d.rest || 'Reminder' };
    }
    case 'rank':
    case 'suggest':
    case 'ticket':
      if (!ctx.guild) return { kind: 'error', message: `/${name} only works in a server.` };
      return {
        kind: 'community',
        tab: name === 'rank' ? 'rank' : name === 'suggest' ? 'suggestions' : 'tickets',
      };
    default:
      return { kind: 'none' };
  }
}

export function commandSuggestions(
  query: string,
  ctx: SlashContext = { guild: true, threads: true, tts: true },
) {
  const q = query.toLowerCase();
  const ok = (c: SlashCommand) =>
    c.needs === 'guild'
      ? ctx.guild
      : c.needs === 'channel'
        ? ctx.threads
        : c.needs === 'tts'
          ? ctx.tts
          : true;
  return SLASH_COMMANDS.filter((c) => c.name.startsWith(q) && ok(c)).map((c) => ({
    key: `/${c.name}`,
    label: `/${c.name}`,
    hint: c.description,
    insert: `/${c.name}${c.arg ? ' ' : ''}`,
  }));
}
