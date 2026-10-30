// The ring feed: one fetch shared by every avatar that wants to know whether its owner has a live ring. A ring
// lives 24 hours, so the feed is refreshed when it is needed again after a minute, after the viewer posts or
// deletes one, and after a ring is opened. A failed fetch leaves the last feed (or none): no avatar breaks.
import { useEffect } from 'react';
import { create } from 'zustand';
import { getRingFeed, type RingFeedEntry } from './api';

const STALE_MS = 60_000;

interface FeedState {
  /** `null` until the first answer. */
  entries: RingFeedEntry[] | null;
  loadedAt: number;
}

export const useRingFeedStore = create<FeedState>(() => ({ entries: null, loadedAt: 0 }));

let inflight: Promise<void> | null = null;

/** Fetch the feed now (shared with a fetch already under way). Never rejects. */
export function refreshRings(): Promise<void> {
  inflight ??= getRingFeed()
    .then((entries) => {
      useRingFeedStore.setState({ entries, loadedAt: Date.now() });
    })
    .catch(() => {
      // Signed out, rings switched off, or offline: keep what we have.
      useRingFeedStore.setState((s) => ({ ...s, loadedAt: Date.now() }));
    })
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

/** Forget the feed (sign-out), so the next account never sees the last one's rings. */
export function resetRings(): void {
  inflight = null;
  useRingFeedStore.setState({ entries: null, loadedAt: 0 });
}

/** The feed, fetched on first use and again when it is a minute old. */
export function useRingFeed(): RingFeedEntry[] {
  const entries = useRingFeedStore((s) => s.entries);
  const loadedAt = useRingFeedStore((s) => s.loadedAt);
  useEffect(() => {
    if (Date.now() - loadedAt > STALE_MS) void refreshRings();
  }, [loadedAt]);
  return entries ?? [];
}

/** The live rings of one person as the viewer sees them, or `undefined` when there are none to show. */
export function useRingsOf(userId: string | undefined): RingFeedEntry | undefined {
  const feed = useRingFeed();
  if (!userId) return undefined;
  return feed.find((e) => e.author_id === userId);
}
