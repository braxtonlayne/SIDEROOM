// The one place the sync controller's player and the theatre's stage meet for media outside the Cinema catalog
// (PRF-UNF-001). The controller talks to `externalHost.player`; the stage (a React component) registers the real
// backend once the viewer allowed the load, and releases it when it unmounts. Until then every call is a no-op
// and `play()` answers "needs a click", which the theatre already knows how to show.
import { createStore, type StoreApi } from 'zustand/vanilla';
import type { SyncPlayer } from './controller';
import type { ExternalBackend } from './externalPlayers';
import type { PartyMedia } from './types';

export interface ExternalHostState {
  /** The external media the party plays right now, `null` for a Cinema title or no media. */
  media: PartyMedia | null;
  /** The viewer allowed the third-party load for this media (a click, or the "Load embeds" setting). */
  allowed: boolean;
  /** A backend is attached and can be driven. */
  attached: boolean;
}

type Ev = 'waiting' | 'playing' | 'ended';

export class ExternalHost {
  readonly store: StoreApi<ExternalHostState> = createStore<ExternalHostState>(() => ({
    media: null,
    allowed: false,
    attached: false,
  }));
  private backend: ExternalBackend | null = null;
  private unsub: (() => void) | null = null;
  private listeners = new Set<(e: Ev) => void>();

  /** The player the controller drives. */
  readonly player: SyncPlayer;

  constructor() {
    // eslint-disable-next-line @typescript-eslint/no-this-alias
    const self = this;
    this.player = {
      positionMs: () => self.backend?.positionMs() ?? 0,
      paused: () => self.backend?.paused() ?? true,
      waiting: () => self.backend?.waiting() ?? false,
      play: () => self.backend?.play() ?? Promise.resolve(false),
      pause: () => self.backend?.pause(),
      seekMs: (ms) => self.backend?.seekMs(ms),
      setRate: (r) => self.backend?.setRate(r),
      get coarseRate() {
        return self.backend?.coarseRate ?? false;
      },
      subscribe: (fn) => {
        self.listeners.add(fn);
        return () => self.listeners.delete(fn);
      },
    };
  }

  /** The party moved to this media (or to a Cinema title / nothing, with `null`). */
  setMedia(media: PartyMedia | null): void {
    const cur = this.store.getState().media;
    const same = cur && media && cur.kind === media.kind && cur.url === media.url;
    if (same) return;
    this.release();
    this.store.setState({ media, allowed: false, attached: false });
  }

  /** The viewer clicked "Load" (or the setting makes the click unnecessary). */
  allow(): void {
    if (this.store.getState().media) this.store.setState({ allowed: true });
  }

  attach(backend: ExternalBackend): void {
    this.release();
    this.backend = backend;
    this.unsub = backend.subscribe((e) => {
      for (const l of [...this.listeners]) l(e);
    });
    this.store.setState({ attached: true });
  }

  detach(backend: ExternalBackend): void {
    if (this.backend !== backend) return;
    this.release();
    this.store.setState({ attached: false });
  }

  private release(): void {
    this.unsub?.();
    this.unsub = null;
    this.backend?.dispose();
    this.backend = null;
  }

  /** Tests, and leaving the party. */
  reset(): void {
    this.release();
    this.store.setState({ media: null, allowed: false, attached: false });
  }
}

const host = new ExternalHost();

export function externalHost(): ExternalHost {
  return host;
}
