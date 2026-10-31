import type { Message, Snowflake } from './data';
import { sameDay } from './time';

/** Consecutive messages by the same author within this window collapse under one header (spec 6.1). */
export const GROUP_WINDOW_MS = 7 * 60_000;

export type Row =
  | { kind: 'welcome'; key: string }
  | { kind: 'date'; key: string; iso: string }
  | { kind: 'new'; key: string }
  | { kind: 'message'; key: string; message: Message; grouped: boolean }
  /** A run of messages from blocked or ignored people, collapsed (LNG-MSGUI-032). */
  | { kind: 'hidden'; key: string; count: number; blocked: number; ignored: number };

export interface HiddenAuthors {
  blocked: ReadonlySet<Snowflake>;
  ignored: ReadonlySet<Snowflake>;
}

/** Message ids are snowflakes (time ordered) in production; mock ids are not, so compare by position. */
export function buildRows(
  messages: readonly Message[],
  opts: {
    lastReadId?: Snowflake | null;
    atStart: boolean;
    selfId?: Snowflake;
    /** Authors whose messages collapse into a "N blocked messages" row. */
    hidden?: HiddenAuthors;
    /** Keys (`hidden:<first message id>`) of collapsed runs the viewer chose to show. */
    revealed?: ReadonlySet<string>;
  },
): Row[] {
  const rows: Row[] = [];
  const isHidden = (m: Message) =>
    !!opts.hidden && (opts.hidden.blocked.has(m.author.id) || opts.hidden.ignored.has(m.author.id));
  let runKey: string | null = null;
  let collapsed: Extract<Row, { kind: 'hidden' }> | null = null;
  if (opts.atStart) rows.push({ kind: 'welcome', key: 'welcome' });
  const readIdx = opts.lastReadId ? messages.findIndex((m) => m.id === opts.lastReadId) : -1;
  let prev: Message | null = null;
  let newInserted = false;
  messages.forEach((m, i) => {
    if (isHidden(m)) {
      runKey ??= `hidden:${m.id}`;
      if (!opts.revealed?.has(runKey)) {
        if (!collapsed) {
          collapsed = { kind: 'hidden', key: runKey, count: 0, blocked: 0, ignored: 0 };
          rows.push(collapsed);
        }
        collapsed.count += 1;
        if (opts.hidden!.blocked.has(m.author.id)) collapsed.blocked += 1;
        else collapsed.ignored += 1;
        return;
      }
    } else runKey = null;
    const afterCollapsed = collapsed !== null;
    collapsed = null;
    const d = new Date(m.createdAt);
    const newDay = !prev || !sameDay(new Date(prev.createdAt), d);
    if (newDay) rows.push({ kind: 'date', key: `date:${m.id}`, iso: m.createdAt });
    let divided = newDay || afterCollapsed;
    if (!newInserted && readIdx >= 0 && i === readIdx + 1 && m.author.id !== opts.selfId) {
      rows.push({ kind: 'new', key: 'new' });
      newInserted = true;
      divided = true;
    }
    const grouped =
      !divided &&
      prev !== null &&
      prev.author.id === m.author.id &&
      // Every Discord person arrives under the one bridge author: group by their Discord name too.
      (!m.author.bridge || prev.author.displayName === m.author.displayName) &&
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
    case 'hidden':
      return 28;
    case 'message':
      return row.grouped ? 24 : 56;
  }
}
