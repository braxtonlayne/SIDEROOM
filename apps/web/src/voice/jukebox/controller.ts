// Synced local playback of the voice jukebox (MUS-JB-002). The server owns the schedule: a track started at
// `started_at_ms` on the server clock, so every listener derives the same position, `serverNow - started_at`,
// from its own clock offset (the NTP-style estimator of the Watch Party) and keeps its element within a few
// hundred milliseconds of it (the Watch Party's drift policy: ignore, nudge the rate, hard seek).
//
// What a member hears is decided by the server per listener: the presign is refused (404) for a track in a
// bucket the member cannot read, and then the controller stays silent and says "not shared with you".
import { ApiError } from '../../api/client';
import { onDispatch } from '../../store/events';
import {
  ClockEstimator,
  planCorrection,
  SETTLE_MS,
  type Correction,
} from '../../watchparty/sync';
import type { JukeboxApi, JukeboxState, JukeboxUpdateEvent } from './types';

/** What the controller reports to the UI. */
export type JukeboxStatus =
  | 'off' // not listening: nothing plays, no heartbeat
  | 'idle' // listening, nothing is playing
  | 'loading'
  | 'playing'
  | 'paused'
  | 'notShared' // the playing track is in a bucket this member cannot read
  | 'streamed' // stream mode: the audio comes through the call
  | 'blocked' // the browser refused to start sound without a gesture
  | 'error';

export interface JukeboxSnapshot {
  state: JukeboxState | null;
  status: JukeboxStatus;
  /** Last measured drift of the element against the schedule, ms (positive = ahead). */
  driftMs: number | null;
}

/** The part of an `<audio>` element the controller uses (a fake in tests). */
export interface AudioLike {
  src: string;
  currentTime: number;
  playbackRate: number;
  readonly paused: boolean;
  readonly readyState: number;
  play(): Promise<void>;
  pause(): void;
  addEventListener(type: string, fn: () => void): void;
  removeEventListener(type: string, fn: () => void): void;
}

export interface JukeboxDeps {
  api: JukeboxApi;
  audio: AudioLike;
  /** Local clock, ms. */
  now?: () => number;
  setInterval?: (fn: () => void, ms: number) => unknown;
  clearInterval?: (h: unknown) => void;
  /** Subscribe to gateway dispatches; returns the unsubscribe. */
  subscribe?: (fn: (event: string, data: unknown) => void) => () => void;
}

/** How often the element is compared with the schedule. */
export const TICK_MS = 500;
/** How often a listener says it is still there (the server counts a listener for 60 s). */
export const HEARTBEAT_MS = 30_000;
/** A track that should have ended this long ago makes the controller ask the server what is playing. */
const OVERRUN_MS = 800;

export class JukeboxController {
  private readonly api: JukeboxApi;
  private readonly audio: AudioLike;
  private readonly now: () => number;
  private readonly setTimer: (fn: () => void, ms: number) => unknown;
  private readonly clearTimer: (h: unknown) => void;
  private readonly subscribeEvents: (fn: (event: string, data: unknown) => void) => () => void;
  private readonly clock = new ClockEstimator();
  private readonly listeners = new Set<() => void>();

  private channelId: string | null = null;
  private snap: JukeboxSnapshot = { state: null, status: 'off', driftMs: null };
  private tickHandle: unknown = null;
  private beatHandle: unknown = null;
  private offEvents: (() => void) | null = null;
  private loadedItem: string | null = null;
  private loadSeq = 0;
  private nudging = false;
  private settleUntil = 0;
  private overrunAsked: string | null = null;
  private fetching = false;
  private stale = false;

  constructor(deps: JukeboxDeps) {
    this.api = deps.api;
    this.audio = deps.audio;
    this.now = deps.now ?? (() => Date.now());
    this.setTimer = deps.setInterval ?? ((fn, ms) => setInterval(fn, ms));
    this.clearTimer = deps.clearInterval ?? ((h) => clearInterval(h as ReturnType<typeof setInterval>));
    this.subscribeEvents = deps.subscribe ?? onDispatch;
  }

  // ---- store surface (useSyncExternalStore) -------------------------------------------------------

  subscribe = (fn: () => void): (() => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  getSnapshot = (): JukeboxSnapshot => this.snap;

  private set(patch: Partial<JukeboxSnapshot>): void {
    this.snap = { ...this.snap, ...patch };
    for (const l of [...this.listeners]) l();
  }

  /** Server time now, from the clock estimate. */
  serverNow(): number {
    return this.clock.serverNow(this.now());
  }

  /** Where the schedule says the current track is now (null when nothing is scheduled to play). */
  expectedMs(): number | null {
    const s = this.snap.state;
    if (!s?.current || s.paused || s.started_at_ms === undefined) return null;
    return Math.max(0, this.serverNow() - s.started_at_ms);
  }

  // ---- lifecycle ----------------------------------------------------------------------------------

  /**
   * Join the jukebox of a channel: tell the server this member is listening, read the state and follow the
   * gateway. Call from a user gesture so the browser lets sound start.
   */
  async start(channelId: string): Promise<void> {
    if (this.channelId === channelId && this.snap.status !== 'off') return;
    this.stop(false);
    this.channelId = channelId;
    this.set({ status: 'idle' });
    this.offEvents = this.subscribeEvents((event, data) => {
      if (event !== 'JUKEBOX_UPDATE') return;
      const e = data as JukeboxUpdateEvent;
      if (e.channel_id !== this.channelId) return;
      if (this.snap.state && e.version <= this.snap.state.version) return;
      void this.refresh();
    });
    this.tickHandle = this.setTimer(() => this.tick(), TICK_MS);
    this.beatHandle = this.setTimer(() => void this.heartbeat(), HEARTBEAT_MS);
    await this.exchange(() => this.api.listen(channelId));
  }

  /** Leave: stop the sound, tell the server (your vote and the decks go) and unfollow. */
  async leave(): Promise<void> {
    const channel = this.channelId;
    this.stop(true);
    if (channel) await this.api.leave(channel).catch(() => undefined);
  }

  private stop(clearState: boolean): void {
    if (this.tickHandle !== null) this.clearTimer(this.tickHandle);
    if (this.beatHandle !== null) this.clearTimer(this.beatHandle);
    this.tickHandle = this.beatHandle = null;
    this.offEvents?.();
    this.offEvents = null;
    this.silence();
    this.channelId = null;
    this.loadSeq++;
    if (clearState) this.set({ state: null, status: 'off', driftMs: null });
  }

  /** Unmount without telling the server (the heartbeat simply lapses). */
  dispose(): void {
    this.stop(true);
  }

  private silence(): void {
    this.audio.pause();
    if (this.audio.src) this.audio.src = '';
    this.loadedItem = null;
    this.nudging = false;
  }

  // ---- reading the server -------------------------------------------------------------------------

  /** Run a call whose answer is the state, measuring the server clock on the way. */
  private async exchange(call: () => Promise<JukeboxState>): Promise<JukeboxState | null> {
    const sentMs = this.now();
    try {
      const s = await call();
      this.clock.add({ serverMs: s.server_now_ms, sentMs, receivedMs: this.now() });
      this.apply(s);
      return s;
    } catch {
      // A failed heartbeat or refetch must not silence a track that is playing; the next one tries again.
      if (!this.snap.state) this.set({ status: 'error' });
      return null;
    }
  }

  async refresh(): Promise<void> {
    const channel = this.channelId;
    if (!channel) return;
    if (this.fetching) {
      this.stale = true;
      return;
    }
    this.fetching = true;
    try {
      await this.exchange(() => this.api.state(channel));
    } finally {
      this.fetching = false;
      if (this.stale) {
        this.stale = false;
        void this.refresh();
      }
    }
  }

  private async heartbeat(): Promise<void> {
    const channel = this.channelId;
    if (channel) await this.exchange(() => this.api.listen(channel));
  }

  /** Run a member action and take the state it answers with. */
  async act(call: (api: JukeboxApi, channelId: string) => Promise<JukeboxState>): Promise<void> {
    const channel = this.channelId;
    if (!channel) return;
    await this.exchange(() => call(this.api, channel));
  }

  // ---- following the schedule ---------------------------------------------------------------------

  private apply(s: JukeboxState): void {
    this.set({ state: s });
    const cur = s.current;
    if (!cur) {
      this.silence();
      this.set({ status: 'idle', driftMs: null });
      return;
    }
    if (s.mode === 'stream' && !s.i_am_dj) {
      // The DJ sends the audio through the call; this member hears it there.
      this.silence();
      this.set({ status: 'streamed', driftMs: null });
      return;
    }
    if (!cur.playable) {
      this.silence();
      this.set({ status: 'notShared', driftMs: null });
      return;
    }
    if (s.paused) {
      this.audio.pause();
      this.set({ status: 'paused', driftMs: null });
      return;
    }
    if (this.loadedItem !== cur.id) {
      void this.load(cur.id);
    } else if (this.audio.paused) {
      void this.begin();
    }
  }

  private async load(itemId: string): Promise<void> {
    const channel = this.channelId;
    if (!channel) return;
    const seq = ++this.loadSeq;
    this.loadedItem = itemId;
    this.overrunAsked = null;
    this.nudging = false;
    this.audio.pause();
    this.set({ status: 'loading' });
    let url: string;
    try {
      url = (await this.api.presign(channel, itemId)).url;
    } catch (e) {
      if (seq !== this.loadSeq) return;
      this.loadedItem = null;
      this.set({ status: e instanceof ApiError && e.status === 404 ? 'notShared' : 'error' });
      return;
    }
    if (seq !== this.loadSeq) return;
    this.audio.src = url;
    await this.begin();
  }

  /** Start (or restart) the element at the scheduled position. */
  private async begin(): Promise<void> {
    const seq = this.loadSeq;
    const at = this.expectedMs();
    if (at === null) return;
    try {
      this.audio.currentTime = at / 1000;
    } catch {
      /* metadata not loaded yet: the first ticks seek */
    }
    this.settleUntil = this.now() + SETTLE_MS;
    try {
      await this.audio.play();
      if (seq !== this.loadSeq) return;
      this.set({ status: 'playing' });
    } catch (e) {
      if (seq !== this.loadSeq) return;
      const blocked = e instanceof DOMException && e.name === 'NotAllowedError';
      this.set({ status: blocked ? 'blocked' : 'error' });
    }
  }

  /** A gesture after the browser refused sound: try again. */
  async resume(): Promise<void> {
    if (this.snap.status === 'blocked') await this.begin();
  }

  private tick(): void {
    const s = this.snap.state;
    const expected = this.expectedMs();
    if (!s?.current || expected === null) return;
    // The schedule says the track is over: ask the server (which settles) once.
    const len = s.current.duration_ms;
    if (len && expected > len + OVERRUN_MS && this.overrunAsked !== s.current.id) {
      this.overrunAsked = s.current.id;
      void this.act((api, c) => api.ended(c, s.current!.id));
      return;
    }
    if (this.snap.status !== 'playing' || this.audio.paused) return;
    if (this.now() < this.settleUntil) return;
    const drift = this.audio.currentTime * 1000 - expected;
    this.set({ driftMs: Math.round(drift) });
    this.correct(planCorrection(drift, this.nudging, 1, expected));
  }

  private correct(c: Correction): void {
    switch (c.kind) {
      case 'none':
        return;
      case 'nudge':
        this.nudging = true;
        this.audio.playbackRate = c.rate;
        return;
      case 'settle':
        this.nudging = false;
        this.audio.playbackRate = 1;
        return;
      case 'seek':
        this.nudging = false;
        this.audio.playbackRate = 1;
        this.audio.currentTime = c.toMs / 1000;
        this.settleUntil = this.now() + SETTLE_MS;
    }
  }

  /** The element reached its end: tell the server, which decides (its own clock) whether the track is over. */
  onElementEnded = (): void => {
    const cur = this.snap.state?.current;
    if (!cur) return;
    void this.act((api, c) => api.ended(c, cur.id));
  };
}
