/**
 * Keeps the two previews current from the gateway events the client already gets (D-688, D-689); no new event,
 * no new request.
 *
 * - The thread row under a message: a message create, update or delete in a thread whose starter row is loaded
 *   updates that row's latest message. A thread's messages reach a member who has not opened it (the gateway
 *   sends a channel's messages to everyone who can view it), so the row follows live; what the server sent at
 *   load time stands until then. A delete drops the preview when it was that message; the next load brings the
 *   one before back.
 * - The "started a thread" row (type 18, D-701) carries the same preview when the server embedded the thread. Its
 *   key is the thread's id, which is the row's `message_reference.message_id`. When the page also holds the
 *   starter message of that thread (same id) the chip sits under the starter only, and the row has none.
 * - A Message Request: the sender's next message replaces its preview, built here with the same excerpt rule
 *   the server uses. The preview names its message (D-699): deleting that message (or a bulk delete naming it)
 *   removes the preview at once, and an edit of it rebuilds the text. A preview without an id (an older server's
 *   answer in a cached state) is left alone.
 * - A "started a thread" row that arrives live (D-715): the gateway never carries the thread on a message, so the
 *   web builds `thread: { id, name, message_count }` from the thread the thread store already holds (THREAD_CREATE
 *   comes before the row; the other order works too). The one-chip rule (D-701) holds here as on the server: the
 *   chip goes under the starter message when it is loaded, on the row only when it is not.
 */
import { onDispatch } from '../store/events';
import { useStore } from '../store';
import type { Message, ThreadLastMessage } from '../store/types';
import { useThreads, type ThreadInfo } from './threads/threadStore';
import type { DmObject } from './shell/api';
import { useSocial } from './shell/social';
import { excerpt } from './msg/excerpt';

type Obj = Record<string, unknown>;

/** Default, reply, slash command and context menu: what a person wrote (the server's rule for the preview). */
const WRITTEN = new Set([0, 19, 20, 23]);

/** A system row: a thread was started in this channel (its reference is the thread). */
const THREAD_CREATED = 18;

const cmp = (a: string, b: string) =>
  a.length !== b.length ? a.length - b.length : a < b ? -1 : 1;

function written(d: Obj): boolean {
  const type = typeof d.type === 'number' ? d.type : 0;
  return WRITTEN.has(type) && typeof d.id === 'string' && typeof d.channel_id === 'string';
}

function lastOf(d: Obj): ThreadLastMessage | null {
  const author = d.author as Obj | undefined;
  if (!written(d) || !author || typeof author.id !== 'string') return null;
  const files = Array.isArray(d.attachments) ? d.attachments.length : 0;
  return {
    id: d.id as string,
    author: {
      id: author.id,
      ...(typeof author.username === 'string' ? { username: author.username } : {}),
      ...(typeof author.global_name === 'string' || author.global_name === null
        ? { global_name: author.global_name as string | null }
        : {}),
    },
    content: typeof d.content === 'string' ? (excerpt(d.content) ?? '') : '',
    attachments: files,
    timestamp: typeof d.timestamp === 'string' ? d.timestamp : new Date().toISOString(),
  };
}

/** The type 18 rows of the page that name this thread. */
function rowsOf(messages: Record<string, Message>, thread: string): string[] {
  return Object.keys(messages).filter((k) => {
    const m = messages[k]!;
    return m.type === THREAD_CREATED && m.message_reference?.message_id === thread;
  });
}

function dropThread(m: Message): Message {
  const { thread: _drop, ...rest } = m;
  void _drop;
  return rest as Message;
}

/**
 * Pure (D-715, D-701): the chip of a thread that started live. `threads` is the thread store's `byId`.
 *
 * - MESSAGE_CREATE of a type 18 row, and THREAD_CREATE (the row is already there): the thread's chip goes under
 *   the starter message when the page holds it (a thread's id is its starter's id) and it has none, else on the
 *   type 18 row that has none. Nothing for a private thread (the row is never written for one), for a thread the
 *   store does not hold, or one that already has its chip.
 * - MESSAGE_UPDATE that gives the starter its `thread`: a type 18 row that took the chip meanwhile gives it up.
 * - THREAD_DELETE: the chips of that thread go, as a reload would show.
 *
 * Returns the same object when nothing changed.
 */
export function applyThreadCreatedRow(
  messages: Record<string, Message>,
  threads: Record<string, ThreadInfo>,
  event: string,
  data: unknown,
): Record<string, Message> {
  const d = (data ?? {}) as Obj;
  const give = (id: string, name: string, count: number): Record<string, Message> => {
    const starter = messages[id];
    const open = (starter ? [id] : rowsOf(messages, id)).filter((k) => !messages[k]!.thread);
    if (open.length === 0) return messages;
    const out = { ...messages };
    for (const k of open) out[k] = { ...messages[k]!, thread: { id, name, message_count: count } };
    return out;
  };
  switch (event) {
    case 'MESSAGE_CREATE': {
      if (d.type !== THREAD_CREATED || typeof d.id !== 'string' || !messages[d.id]) return messages;
      const ref = (d.message_reference as Obj | undefined)?.message_id;
      const held = typeof ref === 'string' ? threads[ref] : undefined;
      if (!held || held.isPrivate) return messages;
      // `give` puts the chip under the starter when the page holds it (the row has none then), else on the row.
      return give(held.id, held.name, held.messageCount);
    }
    case 'THREAD_CREATE': {
      if (typeof d.id !== 'string' || d.type === 12) return messages;
      const held = threads[d.id];
      if (held?.isPrivate) return messages;
      // The payload is the thread as the server built it; the store may not have applied it yet.
      const name = held?.name ?? (typeof d.name === 'string' ? d.name : 'Thread');
      const count =
        held?.messageCount ?? (typeof d.message_count === 'number' ? d.message_count : 0);
      return give(d.id, name, count);
    }
    case 'MESSAGE_UPDATE': {
      const th = d.thread as Obj | null | undefined;
      if (typeof d.id !== 'string' || !th || th.id !== d.id) return messages;
      const keys = rowsOf(messages, d.id).filter((k) => messages[k]!.thread?.id === d.id);
      if (keys.length === 0) return messages;
      const out = { ...messages };
      for (const k of keys) out[k] = dropThread(messages[k]!);
      return out;
    }
    case 'THREAD_DELETE': {
      if (typeof d.id !== 'string') return messages;
      const keys = [...rowsOf(messages, d.id), ...(messages[d.id] ? [d.id] : [])].filter(
        (k) => messages[k]!.thread?.id === d.id,
      );
      if (keys.length === 0) return messages;
      const out = { ...messages };
      for (const k of keys) out[k] = dropThread(messages[k]!);
      return out;
    }
    default:
      return messages;
  }
}

/**
 * Pure: one MESSAGE_CREATE / UPDATE / DELETE(_BULK) applied to the loaded messages. The starter row of a thread has
 * the thread's id, so the event's channel is the key. Returns the same object when nothing changed.
 */
export function applyThreadPreview(
  messages: Record<string, Message>,
  event: string,
  data: unknown,
): Record<string, Message> {
  const d = (data ?? {}) as Obj;
  const channel = typeof d.channel_id === 'string' ? d.channel_id : '';
  const starter = channel ? messages[channel] : undefined;
  // The row that holds the thread: the starter message, else the "started a thread" rows that name it.
  const keys: string[] =
    starter?.thread && starter.thread.id === channel
      ? [channel]
      : channel
        ? Object.keys(messages).filter((k) => {
            const m = messages[k]!;
            return (
              m.type === THREAD_CREATED &&
              m.message_reference?.message_id === channel &&
              m.thread?.id === channel
            );
          })
        : [];
  const first = keys[0];
  const holder = first ? messages[first] : undefined;
  if (!first || !holder?.thread) return messages;
  const thread = holder.thread;
  const current = thread.sr_last_message ?? null;
  const put = (next: ThreadLastMessage | null): Record<string, Message> => {
    const { sr_last_message: _drop, ...rest } = thread;
    void _drop;
    const out = { ...messages };
    for (const k of keys) {
      out[k] = { ...messages[k]!, thread: next ? { ...rest, sr_last_message: next } : rest };
    }
    return out;
  };
  switch (event) {
    case 'MESSAGE_CREATE': {
      const next = lastOf(d);
      if (!next || (current && cmp(next.id, current.id) < 0)) return messages;
      return put(next);
    }
    case 'MESSAGE_UPDATE': {
      if (!current || d.id !== current.id || !('content' in d)) return messages;
      const next = lastOf({
        ...d,
        author: d.author ?? current.author,
        timestamp: current.timestamp,
      });
      return next ? put(next) : messages;
    }
    case 'MESSAGE_DELETE':
    case 'MESSAGE_DELETE_BULK': {
      const gone = event === 'MESSAGE_DELETE' ? [d.id] : ((d.ids as unknown[] | undefined) ?? []);
      return current && gone.includes(current.id) ? put(null) : messages;
    }
    default:
      return messages;
  }
}

/** Pure: a message from the sender of a pending request becomes that conversation's preview. */
export function applyRequestPreview(
  requests: DmObject[],
  event: string,
  data: unknown,
  selfId: string | null,
): DmObject[] {
  const d = (data ?? {}) as Obj;
  switch (event) {
    case 'MESSAGE_CREATE': {
      const author = d.author as Obj | undefined;
      if (!written(d) || !author || author.id === selfId) return requests;
      const i = requests.findIndex((r) => r.id === d.channel_id);
      if (i < 0) return requests;
      const text = typeof d.content === 'string' ? (excerpt(d.content) ?? '') : '';
      const files = Array.isArray(d.attachments) ? d.attachments.length : 0;
      const next = [...requests];
      next[i] = {
        ...requests[i]!,
        sr_request_preview: {
          message_id: d.id as string,
          content: text,
          attachments: files,
          timestamp: typeof d.timestamp === 'string' ? d.timestamp : new Date().toISOString(),
        },
      };
      return next;
    }
    case 'MESSAGE_UPDATE': {
      const i = requests.findIndex((r) => r.id === d.channel_id);
      const p = i < 0 ? null : requests[i]!.sr_request_preview;
      if (!p?.message_id || p.message_id !== d.id || !('content' in d)) return requests;
      const next = [...requests];
      next[i] = {
        ...requests[i]!,
        sr_request_preview: {
          ...p,
          content: typeof d.content === 'string' ? (excerpt(d.content) ?? '') : p.content,
          attachments: Array.isArray(d.attachments) ? d.attachments.length : p.attachments,
        },
      };
      return next;
    }
    case 'MESSAGE_DELETE':
    case 'MESSAGE_DELETE_BULK': {
      const i = requests.findIndex((r) => r.id === d.channel_id);
      const p = i < 0 ? null : requests[i]!.sr_request_preview;
      if (!p?.message_id) return requests;
      const gone = event === 'MESSAGE_DELETE' ? [d.id] : ((d.ids as unknown[] | undefined) ?? []);
      if (!gone.includes(p.message_id)) return requests;
      const next = [...requests];
      const { sr_request_preview: _drop, ...rest } = requests[i]!;
      void _drop;
      next[i] = rest;
      return next;
    }
    default:
      return requests;
  }
}

/** Mounts the dispatch subscription (the shell calls this once). */
export function startPreviewsLive(): () => void {
  return onDispatch((event, data) => {
    if (!event.startsWith('MESSAGE_') && event !== 'THREAD_CREATE' && event !== 'THREAD_DELETE')
      return;
    // The reducer has stored the event's message before any listener runs (store.ts `applyDispatch`).
    const messages = useStore.getState().messages;
    const row = applyThreadCreatedRow(messages, useThreads.getState().byId, event, data);
    const next = event.startsWith('MESSAGE_') ? applyThreadPreview(row, event, data) : row;
    if (next !== messages) useStore.setState({ messages: next });
    if (!event.startsWith('MESSAGE_')) return;
    const social = useSocial.getState();
    const requests = applyRequestPreview(
      social.messageRequests,
      event,
      data,
      useStore.getState().currentUserId,
    );
    if (requests !== social.messageRequests) social.setMessageRequests(requests);
  });
}
