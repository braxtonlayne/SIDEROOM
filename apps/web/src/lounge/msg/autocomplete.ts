/**
 * Composer autocomplete (LNG-CMP-003): finds the `@`, `#`, `:` or `/` token under the caret and ranks
 * suggestions. Pure, so it is unit-tested without a DOM.
 */
import type { Channel, Member, Role } from '../data';

export type TriggerKind = '@' | '#' | ':' | '/';

export interface Trigger {
  kind: TriggerKind;
  /** Text typed after the trigger character. */
  query: string;
  /** Index of the trigger character. */
  start: number;
}

const QUERY_CHARS = /^[\p{L}\p{N}_.\-']*$/u;

/** The trigger the caret is inside, if any. `:` needs two characters so "10:30" and "re:" stay quiet. */
export function findTrigger(text: string, caret: number): Trigger | null {
  const before = text.slice(0, caret);
  for (let i = before.length - 1; i >= 0; i--) {
    const ch = before[i]!;
    if (/\s/.test(ch)) return null;
    if (ch === '@' || ch === '#' || ch === ':' || ch === '/') {
      const prev = i === 0 ? '' : before[i - 1]!;
      if (ch === '/') {
        // Slash commands only at the very start of the message.
        if (i !== 0) return null;
      } else if (prev && !/\s/.test(prev)) {
        return null;
      }
      const query = before.slice(i + 1);
      if (!QUERY_CHARS.test(query)) return null;
      if (ch === ':' && query.length < 2) return null;
      return { kind: ch, query, start: i };
    }
  }
  return null;
}

export interface Suggestion {
  /** Stable key. */
  key: string;
  /** Row text. */
  label: string;
  /** Secondary text (username, role, type). */
  hint?: string;
  /** Text inserted in the box (what the viewer sees). */
  insert: string;
  /** Wire token the inserted text becomes when the message is sent (mentions and channels). */
  token?: string;
  /** Colour dot for roles. */
  color?: string | null;
}

const rank = (name: string, q: string): number => {
  const n = name.toLowerCase();
  if (!q) return 2;
  if (n === q) return 0;
  if (n.startsWith(q)) return 1;
  if (n.includes(q)) return 3;
  return -1;
};

function best(q: string, ...names: (string | null | undefined)[]): number {
  let out = -1;
  for (const n of names) {
    if (!n) continue;
    const r = rank(n, q);
    if (r >= 0 && (out < 0 || r < out)) out = r;
  }
  return out;
}

export function mentionSuggestions(
  query: string,
  members: readonly Member[],
  roles: readonly Role[],
  opts: {
    everyone: boolean;
    limit?: number;
    /** Discord members of a bridged channel: mentioned with the same `<@id>` token (owner direction). */
    bridged?: readonly { id: string; name: string; color?: string | undefined }[];
  },
): Suggestion[] {
  const q = query.toLowerCase();
  const scored: { s: Suggestion; r: number }[] = [];
  for (const m of members) {
    const name = m.nick || m.user.displayName;
    const r = best(q, name, m.user.username, m.user.displayName);
    if (r >= 0)
      scored.push({
        r,
        s: {
          key: `u:${m.user.id}`,
          label: name,
          hint: m.user.username,
          insert: `@${name}`,
          token: `<@${m.user.id}>`,
        },
      });
  }
  for (const b of opts.bridged ?? []) {
    const r = best(q, b.name);
    if (r >= 0)
      scored.push({
        r: r + 0.25,
        s: {
          key: `b:${b.id}`,
          label: b.name,
          hint: 'Discord',
          insert: `@${b.name}`,
          token: `<@${b.id}>`,
          color: b.color ?? null,
        },
      });
  }
  for (const role of roles) {
    if (role.id === role.name) continue;
    const r = best(q, role.name);
    if (r >= 0 && role.name !== '@everyone')
      scored.push({
        r: r + 0.5,
        s: {
          key: `r:${role.id}`,
          label: role.name,
          hint: 'Role',
          insert: `@${role.name}`,
          token: `<@&${role.id}>`,
          color: role.color ?? null,
        },
      });
  }
  if (opts.everyone) {
    for (const word of ['everyone', 'here']) {
      const r = best(q, word);
      if (r >= 0)
        scored.push({
          r: r + 4,
          s: {
            key: `e:${word}`,
            label: `@${word}`,
            hint: word === 'here' ? 'Notify online members' : 'Notify everyone',
            insert: `@${word}`,
          },
        });
    }
  }
  scored.sort((a, b) => a.r - b.r || a.s.label.localeCompare(b.s.label));
  return scored.slice(0, opts.limit ?? 8).map((x) => x.s);
}

export function channelSuggestions(
  query: string,
  channels: readonly Channel[],
  limit = 8,
): Suggestion[] {
  const q = query.toLowerCase();
  return channels
    .filter((c) => c.type === 'text' || c.type === 'announcement' || c.type === 'voice')
    .map((c) => ({ c, r: best(q, c.name) }))
    .filter((x) => x.r >= 0)
    .sort((a, b) => a.r - b.r || a.c.name.localeCompare(b.c.name))
    .slice(0, limit)
    .map(({ c }) => ({
      key: `c:${c.id}`,
      label: `#${c.name}`,
      hint: c.type === 'voice' ? 'Voice' : 'Text',
      insert: `#${c.name}`,
      token: `<#${c.id}>`,
    }));
}

export function emojiSuggestions(
  query: string,
  all: readonly (readonly [string, string])[],
  limit = 8,
): Suggestion[] {
  const q = query.toLowerCase();
  return all
    .map(([name, ch]) => ({ name, ch, r: rank(name, q) }))
    .filter((x) => x.r >= 0)
    .sort((a, b) => a.r - b.r || a.name.length - b.name.length)
    .slice(0, limit)
    .map((x) => ({
      key: `e:${x.name}`,
      label: `:${x.name}:`,
      hint: x.ch,
      insert: x.ch,
    }));
}

/** Replaces the trigger text with the suggestion (plus a trailing space for mentions). */
export function applySuggestion(
  text: string,
  trigger: Trigger,
  caret: number,
  s: Suggestion,
): { value: string; caret: number } {
  const tail = s.token || s.insert.startsWith('@') || s.insert.startsWith('#') ? ' ' : '';
  const value = text.slice(0, trigger.start) + s.insert + tail + text.slice(caret);
  return { value, caret: trigger.start + s.insert.length + tail.length };
}

/**
 * Swaps the readable `@Name` / `#channel` text the viewer typed back for wire tokens just before sending.
 * Longest names first, and only when not followed by more name characters.
 */
export function resolveTokens(text: string, tokens: ReadonlyMap<string, string>): string {
  let out = text;
  const keys = [...tokens.keys()].sort((a, b) => b.length - a.length);
  for (const key of keys) {
    const esc = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    out = out.replace(new RegExp(`(^|\\s)${esc}(?![\\p{L}\\p{N}_])`, 'gu'), (_m, pre: string) => {
      return `${pre}${tokens.get(key)}`;
    });
  }
  return out;
}
