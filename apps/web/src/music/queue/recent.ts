/**
 * MUS-LIB-001: records "recently played". A track counts as played once it has been the current track for
 * `PLAYED_AFTER_MS` (skipping through a queue does not fill the history). Started once at app start-up next
 * to `setPlayerPort`; it reads the player only through the port.
 */
import { getPlayerPort, type PlayerPort } from './port';

export const PLAYED_AFTER_MS = 5000;

export interface RecentTrackerOptions {
  record: (trackId: string) => Promise<unknown>;
  port?: () => PlayerPort;
  setTimeout?: (fn: () => void, ms: number) => unknown;
  clearTimeout?: (h: unknown) => void;
}

/** Returns a function that stops the tracker. */
export function startRecentTracker(opts: RecentTrackerOptions): () => void {
  const port = opts.port ?? getPlayerPort;
  const later = opts.setTimeout ?? ((fn, ms) => setTimeout(fn, ms));
  const cancel = opts.clearTimeout ?? ((h) => clearTimeout(h as ReturnType<typeof setTimeout>));
  let current: string | null = null;
  let timer: unknown = null;

  const currentId = (): string | null => {
    const s = port().getState();
    return s.index >= 0 ? (s.queueIds[s.index] ?? null) : null;
  };

  const check = () => {
    const id = currentId();
    if (id === current) return;
    if (timer !== null) cancel(timer);
    timer = null;
    current = id;
    if (id === null) return;
    timer = later(() => {
      timer = null;
      if (currentId() === id) void opts.record(id).catch(() => undefined);
    }, PLAYED_AFTER_MS);
  };

  // Start it after setPlayerPort: it watches the port that is registered at this moment.
  const off = port().subscribe(check);
  check();
  return () => {
    off();
    if (timer !== null) cancel(timer);
    timer = null;
    current = null;
  };
}
