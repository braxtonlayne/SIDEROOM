import { useEffect, useRef, useState } from 'react';
import { apiFetch } from '../api/client';
import { t, useT } from '../i18n';
import { useMarkdownReady } from './markdown';
import { cmpId } from '../store/reducer';
import { messageLineText, useHiddenKind } from './msg/lineText';

/** The messages next to a search hit (LNG-SRC-005): one before and one after, read when the row is on screen. */
export interface HitContext {
  before: Neighbor | null;
  after: Neighbor | null;
}
export interface Neighbor {
  id: string;
  author: string;
  /** The author's id: a blocked or ignored one is not quoted (D-730). */
  authorId?: string;
  /** The message's typed text, or (with `plain`) our own words for a message with only an attachment. */
  text: string;
  /** `text` is already plain: it is not run through the markdown (D-739). */
  plain?: true;
}

interface Wire {
  id: string;
  content?: string;
  attachments?: unknown[];
  author?: { id?: string; username: string; global_name?: string | null };
}

const cache = new Map<string, Promise<HitContext>>();
/** Test hook. */
export const resetHitContext = () => cache.clear();

const toNeighbor = (m: Wire | undefined): Neighbor | null =>
  m
    ? {
        id: m.id,
        author: m.author?.global_name || m.author?.username || t('lounge.searchContext.someone'),
        ...(m.author?.id ? { authorId: m.author.id } : {}),
        ...(m.content?.trim() || !m.attachments?.length
          ? { text: m.content?.trim() ?? '' }
          : { text: t('lounge.searchContext.attachment'), plain: true as const }),
      }
    : null;

/** Pure: picks the messages either side of `hitId` out of an `around` page. */
export function neighborsOf(page: readonly Wire[], hitId: string): HitContext {
  const sorted = [...page].sort((a, b) => cmpId(a.id, b.id));
  const at = sorted.findIndex((m) => m.id === hitId);
  if (at < 0) return { before: null, after: null };
  return { before: toNeighbor(sorted[at - 1]), after: toNeighbor(sorted[at + 1]) };
}

export function loadHitContext(channelId: string, hitId: string): Promise<HitContext> {
  const key = `${channelId}:${hitId}`;
  let p = cache.get(key);
  if (!p) {
    p = apiFetch<Wire[]>(`/channels/${channelId}/messages?around=${hitId}&limit=3`, {
      retries: 0,
    })
      .then((page) => neighborsOf(page, hitId))
      .catch(() => {
        cache.delete(key);
        return { before: null, after: null };
      });
    cache.set(key, p);
  }
  return p;
}

/** Loads the context once the element is visible (immediately where IntersectionObserver is missing). */
export function useHitContext(channelId: string, hitId: string) {
  const ref = useRef<HTMLLIElement | null>(null);
  const [ctx, setCtx] = useState<HitContext | null>(null);
  useEffect(() => {
    let live = true;
    const load = () => void loadHitContext(channelId, hitId).then((c) => live && setCtx(c));
    const el = ref.current;
    if (!el || typeof IntersectionObserver === 'undefined') {
      load();
      return () => {
        live = false;
      };
    }
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        io.disconnect();
        load();
      }
    });
    io.observe(el);
    return () => {
      live = false;
      io.disconnect();
    };
  }, [channelId, hitId]);
  return { ref, ctx };
}

export function ContextLine({
  n,
  side,
  guildId,
}: {
  n: Neighbor | null | undefined;
  side: 'before' | 'after';
  guildId?: string | null | undefined;
}) {
  const tt = useT();
  useMarkdownReady();
  const kind = useHiddenKind(n?.authorId);
  if (!n || !n.text) return null;
  if (kind) {
    return (
      <span className="lng-search-context lng-muted" data-side={side} data-hidden-line={kind}>
        {t('lounge.messageList.hiddenRun', { count: 1, kind })}
      </span>
    );
  }
  // The line is the plain text of the message (D-716, D-739): a spoiler hidden, a mention named, never the typed text.
  const text = n.plain ? n.text : messageLineText(n.text, guildId, tt('lounge.preview.spoiler'));
  if (!text) return null;
  return (
    <span className="lng-search-context" data-side={side}>
      <strong>{n.author}</strong> {text}
    </span>
  );
}
