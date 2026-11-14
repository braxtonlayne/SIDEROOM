/**
 * Keeps the two previews current from the gateway events the client already gets (D-688, D-689); no new event,
 * no new request.
 *
 * - The thread row under a message: a message create, update or delete in a thread whose starter row is loaded
 *   updates that row's latest message. A thread's messages reach a member who has not opened it (the gateway
 *   sends a channel's messages to everyone who can view it), so the row follows live; what the server sent at
 *   load time stands until then. A delete drops the preview when it was that message; the next load brings the
 *   one before back.
 * - A Message Request: the sender's next message replaces its preview, built here with the same excerpt rule
 *   the server uses. A conversation that already holds a preview from the list keeps showing it until then.
 */
import { onDispatch } from '../store/events';
import { useStore } from '../store';
import type { Message, ThreadLastMessage } from '../store/types';
import type { DmObject } from './shell/api';
import { useSocial } from './shell/social';
import { excerpt } from './msg/excerpt';

type Obj = Record<string, unknown>;

/** Default, reply, slash command and context menu: what a person wrote (the server's rule for the preview). */
const WRITTEN = new Set([0, 19, 20, 23]);

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
  if (!starter?.thread || starter.thread.id !== channel) return messages;
  const thread = starter.thread;
  const current = thread.sr_last_message ?? null;
  const put = (next: ThreadLastMessage | null): Record<string, Message> => {
    const { sr_last_message: _drop, ...rest } = thread;
    void _drop;
    return {
      ...messages,
      [channel]: { ...starter, thread: next ? { ...rest, sr_last_message: next } : rest },
    };
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
  if (event !== 'MESSAGE_CREATE') return requests;
  const d = (data ?? {}) as Obj;
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
      content: text,
      attachments: files,
      timestamp: typeof d.timestamp === 'string' ? d.timestamp : new Date().toISOString(),
    },
  };
  return next;
}

/** Mounts the dispatch subscription (the shell calls this once). */
export function startPreviewsLive(): () => void {
  return onDispatch((event, data) => {
    if (!event.startsWith('MESSAGE_')) return;
    const messages = useStore.getState().messages;
    const next = applyThreadPreview(messages, event, data);
    if (next !== messages) useStore.setState({ messages: next });
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
