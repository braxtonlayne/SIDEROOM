/**
 * The text of a desktop notification (D-716, D-712): a message as plain text, never as typed. A spoiler is the
 * word for it, a mention is a name, a custom emoji is `:name:`, a link is its label, line breaks are spaces, and
 * the result is cut to 140 characters by the excerpt rule. When the markdown parser is not loaded the text is
 * empty, never the raw message.
 *
 * `reactionBody` is the text under a reaction notification: the stored message when loaded, else a session
 * cache of the last 20 messages, else one read of the message raced against a 2 second timer (D-712). Nothing
 * is written to the store.
 */
import type { Resolvers } from '@sideroom/markdown-react';
import { apiFetch } from '../../api/client';
import { useStore } from '../../store';
import { currentMarkdownEngine, parseMarkdown } from '../markdown';
import { excerptPlain, EXCERPT_CHARS } from '../msg/excerpt';
import { singleLineText } from '../msg/SingleLine';

/** `notificationText` of a message: plain, on one line, at most 140 characters, `''` when it cannot be made plain. */
export function notificationText(
  content: string,
  resolvers: Resolvers | undefined,
  spoilerWord: string,
): string {
  if (!currentMarkdownEngine()) return '';
  const line = singleLineText(parseMarkdown(content), resolvers, spoilerWord);
  // The line is plain text already: it has no markup to split, so it takes the plain cut.
  return excerptPlain(line, EXCERPT_CHARS) ?? '';
}

/** The names the store holds, without a hook: users, and the roles and channels of `guildId`. */
export function storeResolvers(guildId?: string | null): Resolvers {
  const st = useStore.getState();
  return {
    user: (id) => {
      const u = st.users[id];
      const name = u ? u.global_name || u.username : '';
      return name ? { name } : undefined;
    },
    role: (id) => {
      const r = st.roles[id];
      return r && (!guildId || r.guild_id === guildId) ? { name: r.name } : undefined;
    },
    channel: (id) => {
      const c = st.channels[id];
      return c?.name && (!guildId || c.guild_id === guildId) ? { name: c.name } : undefined;
    },
  };
}

/** How long a reaction notification waits for the message it is about. */
export const REACTION_READ_MS = 2000;
const CACHE_MAX = 20;
/** message id -> its plain text (`''`: unreadable or empty), oldest first. */
const cache = new Map<string, string>();
/** Reads in flight, one per message. */
const pending = new Map<string, Promise<string>>();

function remember(id: string, text: string): void {
  cache.delete(id);
  cache.set(id, text);
  while (cache.size > CACHE_MAX) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
}

/** Forget what was read (tests). */
export function resetReactionTextCache(): void {
  cache.clear();
  pending.clear();
}

function read(
  channelId: string,
  messageId: string,
  guildId: string | null | undefined,
  spoilerWord: string,
): Promise<string> {
  const hit = pending.get(messageId);
  if (hit) return hit;
  const p = apiFetch<{ content?: string }>(`/channels/${channelId}/messages/${messageId}`, {
    retries: 0,
  })
    .then((m) => {
      // Without the parser there is no plain text; do not remember that as "empty".
      if (!currentMarkdownEngine()) return '';
      const text = notificationText(m.content ?? '', storeResolvers(guildId), spoilerWord);
      remember(messageId, text);
      return text;
    })
    .catch(() => {
      // 404, 403 or offline: empty for this message for the rest of the session, no retry.
      remember(messageId, '');
      return '';
    })
    .finally(() => pending.delete(messageId));
  pending.set(messageId, p);
  return p;
}

/**
 * The body of a reaction notification for `messageId` in `channelId`. Call it only after the decision and the
 * permission checks said yes. Resolves within `REACTION_READ_MS` (empty when the read is late; a late answer is
 * kept for the next reaction).
 */
export async function reactionBody(
  channelId: string,
  messageId: string,
  guildId: string | null | undefined,
  spoilerWord: string,
  limitMs: number = REACTION_READ_MS,
): Promise<string> {
  const stored = useStore.getState().messages[messageId];
  if (stored) return notificationText(stored.content, storeResolvers(guildId), spoilerWord);
  const cached = cache.get(messageId);
  if (cached !== undefined) return cached;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<string>((resolve) => {
    timer = setTimeout(() => resolve(''), limitMs);
  });
  try {
    return await Promise.race([read(channelId, messageId, guildId, spoilerWord), late]);
  } finally {
    clearTimeout(timer);
  }
}
