import type { Message, Snowflake } from './data';
import { sameDay } from './time';

/** Consecutive messages by the same author within this window collapse under one header (spec 6.1). */
export const GROUP_WINDOW_MS = 7 * 60_000;

export type Row =
  | { kind: 'welcome'; key: string }
  | { kind: 'date'; key: string; iso: string }
  | { kind: 'new'; key: string }
  | { kind: 'message'; key: string; message: Message; grouped: boolean };

/** Message ids are snowflakes (time ordered) in production; mock ids are not, so compare by position. */
export function buildRows(
  messages: readonly Message[],
  opts: { lastReadId?: Snowflake | null; atStart: boolean; selfId?: Snowflake },
): Row[] {
  const rows: Row[] = [];
  if (opts.atStart) rows.push({ kind: 'welcome', key: 'welcome' });
  const readIdx = opts.lastReadId ? messages.findIndex((m) => m.id === opts.lastReadId) : -1;
  let prev: Message | null = null;
  let newInserted = false;
  messages.forEach((m, i) => {
    const d = new Date(m.createdAt);
    const newDay = !prev || !sameDay(new Date(prev.createdAt), d);
    if (newDay) rows.push({ kind: 'date', key: `date:${m.id}`, iso: m.createdAt });
    let divided = newDay;
    if (!newInserted && readIdx >= 0 && i === readIdx + 1 && m.author.id !== opts.selfId) {
      rows.push({ kind: 'new', key: 'new' });
      newInserted = true;
      divided = true;
    }
    const grouped =
      !divided &&
      prev !== null &&
      prev.author.id === m.author.id &&
      !m.replyToId &&
      new Date(m.createdAt).getTime() - new Date(prev.createdAt).getTime() <= GROUP_WINDOW_MS;
    rows.push({ kind: 'message', key: m.id, message: m, grouped });
    prev = m;
  });
  return rows;
}

export function estimateRow(row: Row): number {
  switch (row.kind) {
    case 'welcome':
      return 140;
    case 'date':
    case 'new':
      return 28;
    case 'message':
      return row.grouped ? 24 : 56;
  }
}
