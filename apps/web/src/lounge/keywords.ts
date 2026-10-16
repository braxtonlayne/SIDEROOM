/**
 * Notification keywords (`GET/PUT /users/@me/notification-keywords`): words that notify like a mention. The list
 * is fetched once per session and kept in the store; `SR_NOTIFICATION_KEYWORDS_UPDATE` replaces it. A message
 * that contains one as a whole word or phrase (case-insensitive, like the server's push decision) is
 * highlighted like a mention.
 */
import { useEffect, useMemo } from 'react';
import { apiFetch } from '../api/client';
import { useStore } from '../store';

const EMPTY: string[] = [];

export const fetchNotificationKeywords = () =>
  apiFetch<{ keywords: string[] }>('/users/@me/notification-keywords', { retries: 0 });

/** The viewer's keywords; loads them on first use. */
export function useNotificationKeywords(): string[] {
  const list = useStore((s) => s.notificationKeywords);
  useEffect(() => {
    if (useStore.getState().notificationKeywords !== null) return;
    fetchNotificationKeywords()
      .then((r) =>
        useStore
          .getState()
          .applyDispatch('SR_NOTIFICATION_KEYWORDS_UPDATE', { keywords: r.keywords }),
      )
      .catch(() => undefined);
  }, []);
  return list ?? EMPTY;
}

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** One matcher for the whole list: whole word or phrase, case-insensitive; null when there are no words. */
export function keywordMatcher(keywords: readonly string[]): RegExp | null {
  const words = keywords.map((k) => k.trim()).filter(Boolean);
  if (words.length === 0) return null;
  const alt = words.map((w) => escapeRe(w).replace(/\s+/g, '\\s+')).join('|');
  return new RegExp(`(?<![\\p{L}\\p{N}_])(?:${alt})(?![\\p{L}\\p{N}_])`, 'iu');
}

export function useKeywordMatcher(): RegExp | null {
  const list = useNotificationKeywords();
  return useMemo(() => keywordMatcher(list), [list]);
}

export const hasKeyword = (matcher: RegExp | null, content: string): boolean =>
  matcher !== null && matcher.test(content);
