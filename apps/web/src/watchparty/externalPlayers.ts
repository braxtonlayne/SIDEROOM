// The two players Watch Together uses for media outside the Cinema catalog (PRF-UNF-001), both as `SyncPlayer`s
// the sync controller drives exactly like the Cinema player: a provider's privacy-enhanced iframe (controlled
// through its `postMessage` commands, no provider script) and the browser's own media element (a direct file).
import type { SyncPlayer } from './controller';
import {
  FRAME_ORIGINS,
  YOUTUBE_RATES,
  commandMessage,
  listenMessages,
  parseMessage,
  type FrameCommand,
  type FrameProvider,
} from './frameProtocol';

type PlayerEvent = 'waiting' | 'playing' | 'ended';

/** What the stage registers with the host: a `SyncPlayer` that can be released. */
export interface ExternalBackend extends SyncPlayer {
  dispose(): void;
}

/** How long a play request may take before the browser is assumed to want a click. */
export const PLAY_CONFIRM_MS = 1500;

export interface FramePlayerDeps {
  /** Send one message to the provider frame (the frame's `contentWindow.postMessage` with its exact origin). */
  post(message: string): void;
  now(): number;
  /** Wait for `ms` (timers are injected so tests run on fake time). */
  sleep(ms: number): Promise<void>;
  /** Called once, when the frame first reports (the controller then re-applies the party state). */
  ready?(): void;
}

/**
 * Drives a provider frame. The frame reports its time a few times a second; between reports the position is
 * extrapolated with the rate, so the controller's drift arithmetic sees a smooth clock.
 */
export class FramePlayer implements ExternalBackend {
  private timeMs = 0;
  private at = 0;
  private base: 'unstarted' | 'playing' | 'paused' | 'ended' = 'unstarted';
  private buffering = false;
  private rate = 1;
  private ready = false;
  private announced = false;
  private listeners = new Set<(e: PlayerEvent) => void>();
  private disposed = false;
  /** YouTube snaps to a few fixed speeds, so a 5% nudge cannot be expressed; the controller seeks instead. */
  readonly coarseRate: boolean;

  constructor(
    readonly provider: FrameProvider,
    private readonly deps: FramePlayerDeps,
  ) {
    this.at = deps.now();
    this.coarseRate = provider === 'youtube';
  }

  /** The frame finished loading: ask it to start reporting. */
  onLoad(): void {
    for (const m of listenMessages(this.provider, 'load')) this.deps.post(m);
  }

  /** A message from the frame (already filtered to this frame's window and origin by the caller). */
  onMessage(data: unknown): void {
    if (this.disposed) return;
    for (const e of parseMessage(this.provider, data)) {
      switch (e.type) {
        case 'ready':
          if (!this.ready) {
            for (const m of listenMessages(this.provider, 'ready')) this.deps.post(m);
          }
          this.markReady();
          break;
        case 'time':
          this.timeMs = e.seconds * 1000;
          this.at = this.deps.now();
          this.markReady();
          break;
        case 'rate':
          this.rate = e.rate;
          break;
        case 'state':
          this.onState(e.state);
          break;
      }
    }
  }

  private markReady() {
    this.ready = true;
    if (!this.announced) {
      this.announced = true;
      this.deps.ready?.();
    }
  }

  private onState(state: 'unstarted' | 'playing' | 'paused' | 'buffering' | 'resumed' | 'ended') {
    // Re-anchor the extrapolation so a state change never moves the reported position.
    this.timeMs = this.positionMs();
    this.at = this.deps.now();
    const wasBuffering = this.buffering;
    if (state === 'buffering') {
      this.buffering = true;
      if (!wasBuffering) this.emit('waiting');
      return;
    }
    this.buffering = false;
    if (state !== 'resumed') this.base = state;
    if (state === 'ended') this.emit('ended');
    else if (wasBuffering || state === 'playing') this.emit('playing');
  }

  private emit(e: PlayerEvent) {
    for (const l of [...this.listeners]) l(e);
  }

  private send(c: FrameCommand) {
    if (!this.disposed) this.deps.post(commandMessage(this.provider, c));
  }

  positionMs(): number {
    if (this.base === 'playing' && !this.buffering)
      return this.timeMs + Math.max(0, this.deps.now() - this.at) * this.rate;
    return this.timeMs;
  }

  paused(): boolean {
    return this.base !== 'playing';
  }

  waiting(): boolean {
    return this.buffering;
  }

  private isPlaying(): boolean {
    return this.base === 'playing';
  }

  async play(): Promise<boolean> {
    if (this.isPlaying()) return true;
    this.send({ cmd: 'play' });
    const until = this.deps.now() + PLAY_CONFIRM_MS;
    while (this.deps.now() < until) {
      if (this.isPlaying()) return true;
      await this.deps.sleep(100);
    }
    return this.isPlaying();
  }

  pause(): void {
    this.send({ cmd: 'pause' });
    // The frame confirms a moment later; stop extrapolating at once so a repeated apply() does not re-send.
    if (this.base === 'playing') {
      this.timeMs = this.positionMs();
      this.at = this.deps.now();
      this.base = 'paused';
    }
  }

  seekMs(ms: number): void {
    this.send({ cmd: 'seek', seconds: Math.max(0, ms) / 1000 });
    this.timeMs = Math.max(0, ms);
    this.at = this.deps.now();
  }

  setRate(rate: number): void {
    const snapped =
      this.provider === 'youtube'
        ? YOUTUBE_RATES.reduce((a, b) => (Math.abs(b - rate) < Math.abs(a - rate) ? b : a))
        : rate;
    if (Math.abs(snapped - this.rate) < 1e-6) return;
    this.send({ cmd: 'rate', rate: snapped });
    this.timeMs = this.positionMs();
    this.at = this.deps.now();
    this.rate = snapped;
  }

  subscribe(fn: (e: PlayerEvent) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  dispose(): void {
    this.disposed = true;
    this.listeners.clear();
  }
}

/** The origin messages from a provider frame must carry. */
export const frameOrigin = (p: FrameProvider): string => FRAME_ORIGINS[p];

/** The browser's own player for a direct file (video or audio), as a `SyncPlayer`. */
export class MediaElementPlayer implements ExternalBackend {
  private listeners = new Set<(e: PlayerEvent) => void>();
  private off: (() => void)[] = [];

  constructor(
    readonly el: HTMLMediaElement,
    ready?: () => void,
  ) {
    const on = (name: string, fn: () => void) => {
      el.addEventListener(name, fn);
      this.off.push(() => el.removeEventListener(name, fn));
    };
    on('waiting', () => this.emit('waiting'));
    on('playing', () => this.emit('playing'));
    on('canplay', () => this.emit('playing'));
    on('ended', () => this.emit('ended'));
    if (ready) {
      if (el.readyState >= 1) ready();
      else on('loadedmetadata', ready);
    }
  }

  private emit(e: PlayerEvent) {
    for (const l of [...this.listeners]) l(e);
  }

  positionMs(): number {
    return this.el.currentTime * 1000;
  }

  paused(): boolean {
    return this.el.paused;
  }

  waiting(): boolean {
    return !this.el.paused && this.el.readyState < 3;
  }

  async play(): Promise<boolean> {
    try {
      await this.el.play();
      return true;
    } catch {
      // NotAllowedError: the browser wants a click first. Anything else (a broken file) reads the same way
      // to the controller; the stage shows the element's own error state.
      return false;
    }
  }

  pause(): void {
    this.el.pause();
  }

  seekMs(ms: number): void {
    this.el.currentTime = Math.max(0, ms) / 1000;
  }

  setRate(rate: number): void {
    this.el.playbackRate = rate;
  }

  subscribe(fn: (e: PlayerEvent) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  dispose(): void {
    for (const f of this.off.splice(0)) f();
    this.listeners.clear();
    this.el.pause();
  }
}
