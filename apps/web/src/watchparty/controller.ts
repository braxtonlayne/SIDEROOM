// The watch party sync controller (WP-FE-001, 003, 005): lives outside React, like the player session, so the
// party keeps its clock, heartbeat and drift correction running while the viewer moves around the app (the
// player goes to the mini player; the voice call is untouched).
//
// Server-authoritative: the only things that move the player are (a) the server's playback state, applied
// with drift correction, and (b) nothing else. Controls never touch the player; they send a command and the
// answer (or the gateway event, whichever arrives first) is applied like any other state. That is also what
// keeps the loop from echoing: applying a state never sends a command.
import { createStore, type StoreApi } from 'zustand/vanilla';
import { ApiError } from '../api/client';
import { AGE_CONFIRMATION_REQUIRED } from '../lib/ageGate';
import type { PartyApi } from './api';
import {
  BUFFER_REPORT_MS,
  ClockEstimator,
  DRIFT_IGNORE_MS,
  DRIFT_SEEK_MS,
  SETTLE_MS,
  expectedPositionMs,
  planCorrection,
} from './sync';
import type {
  CommandInput,
  MessageEvent,
  Participant,
  Party,
  PartyMedia,
  PartyUpdateEvent,
  Playback,
  PlaybackEvent,
  ReactionEvent,
} from './types';

/** The player as the controller sees it. The real one wraps the Cinema player session. */
export interface SyncPlayer {
  positionMs(): number;
  paused(): boolean;
  waiting(): boolean;
  /** `false` when the browser refused to start without a user gesture. */
  play(): Promise<boolean>;
  pause(): void;
  seekMs(ms: number): void;
  /** The effective playback rate (the party rate times any nudge). */
  setRate(rate: number): void;
  subscribe(fn: (e: 'waiting' | 'playing' | 'ended') => void): () => void;
}

export interface ControllerDeps {
  api: PartyApi;
  player: SyncPlayer;
  /** Open the party's media in the player (idempotent for the media that is already open). */
  load(media: PartyMedia, startMs: number, autoplay: boolean): Promise<void>;
  /** Local clock in ms. */
  now(): number;
  /** The signed-in account's id. */
  me(): string;
  /** Subscribe to gateway dispatches. */
  onDispatch(fn: (event: string, data: unknown) => void): () => void;
  /** Keep the gateway connection open while following a party; returns the release function. */
  hold?(): () => void;
}

export type PartyErrorKind = 'age' | 'gone' | 'removed' | 'ended' | 'other';

export interface Reaction {
  id: number;
  userId: string;
  emoji: string;
}
export interface ChatLine {
  id: number;
  userId: string;
  content: string;
  atMs: number;
}

export interface PartyState {
  status: 'idle' | 'opening' | 'active' | 'ended' | 'error';
  party: Party | null;
  appliedVersion: number;
  offsetMs: number;
  rttMs: number | null;
  /** Player position minus expected position, in ms (positive = ahead). */
  driftMs: number;
  /** The browser wants a click before it plays. */
  needsGesture: boolean;
  /** This player has been stalled long enough to be reported. */
  localBuffering: boolean;
  /** Usernames of everybody stalled ("Waiting for ..."). */
  waitingFor: string[];
  error: { kind: PartyErrorKind; message: string } | null;
  reactions: Reaction[];
  messages: ChatLine[];
  /** Participants who asked for control (a raised hand). */
  hands: string[];
  /** Last command problem, for a toast. */
  notice: string | null;
}

export const HEARTBEAT_MS = 15_000;
export const TICK_MS = 500;
export const HAND = '✋';
const REACTION_MS = 4_000;
const HAND_MS = 60_000;
const MAX_LINES = 100;

const initial = (): PartyState => ({
  status: 'idle',
  party: null,
  appliedVersion: 0,
  offsetMs: 0,
  rttMs: null,
  driftMs: 0,
  needsGesture: false,
  localBuffering: false,
  waitingFor: [],
  error: null,
  reactions: [],
  messages: [],
  hands: [],
  notice: null,
});

const nameOf = (p: Participant) => p.display_name || p.username;

function classify(e: unknown): { kind: PartyErrorKind; message: string } {
  if (e instanceof ApiError) {
    if (e.status === 403 && e.body?.code === AGE_CONFIRMATION_REQUIRED)
      return { kind: 'age', message: 'This party is showing age-restricted content.' };
    if (e.status === 404) return { kind: 'gone', message: 'This watch party is not available.' };
    if (e.status === 409) return { kind: 'other', message: e.message };
    return { kind: 'other', message: e.message };
  }
  return { kind: 'other', message: 'Something went wrong.' };
}

export class PartyController {
  readonly store: StoreApi<PartyState>;
  private clock = new ClockEstimator();
  private unsubs: (() => void)[] = [];
  private tickTimer: ReturnType<typeof setInterval> | null = null;
  private beatTimer: ReturnType<typeof setInterval> | null = null;
  private bufTimer: ReturnType<typeof setTimeout> | null = null;
  private reported = false;
  private nudging = false;
  private settleUntil = 0;
  private loadedKey = '';
  private endedSentFor = '';
  private seq = 0;
  private lastRtt: number | null = null;
  private token = 0;
  private loading = false;
  private release: (() => void) | null = null;

  constructor(private readonly deps: ControllerDeps) {
    this.store = createStore<PartyState>(() => initial());
  }

  get state(): PartyState {
    return this.store.getState();
  }
  private set(p: Partial<PartyState>) {
    this.store.setState(p);
  }

  /* ------------------------------------------------------------- open / close */

  /** Join (if needed) and start following a party. Resolves once the media was handed to the player. */
  async open(id: string, opts: { code?: string | null } = {}): Promise<void> {
    if (this.state.party?.id === id && this.state.status === 'active') return;
    this.teardown();
    const token = ++this.token;
    this.store.setState(initial(), true);
    this.release = this.deps.hold?.() ?? null;
    this.set({ status: 'opening' });
    try {
      await this.sampleClock(3);
      let party: Party;
      try {
        party = await this.deps.api.get(id);
        if (!party.you?.member) party = await this.deps.api.join(id, opts.code);
      } catch (e) {
        // A private party is invisible until joined: try the code.
        if (e instanceof ApiError && e.status === 404 && opts.code)
          party = await this.deps.api.join(id, opts.code);
        else throw e;
      }
      if (token !== this.token) return;
      this.adopt(party);
      this.set({ appliedVersion: party.playback.version });
      this.listen();
      this.tickTimer = setInterval(() => this.tick(), TICK_MS);
      this.beatTimer = setInterval(() => void this.beat(), HEARTBEAT_MS);
      this.set({ status: 'active' });
      await this.openMedia(party.media, true);
    } catch (e) {
      if (token !== this.token) return;
      this.teardown();
      this.set({ status: 'error', error: classify(e) });
    }
  }

  /** Stop following. `leave: true` also leaves the party on the server. */
  async close(opts: { leave?: boolean } = {}): Promise<void> {
    const id = this.state.party?.id;
    this.teardown();
    this.token++;
    if (opts.leave && id) await this.deps.api.leave(id).catch(() => undefined);
    this.deps.player.pause();
    this.store.setState(initial(), true);
  }

  private teardown() {
    for (const u of this.unsubs.splice(0)) u();
    if (this.tickTimer) clearInterval(this.tickTimer);
    if (this.beatTimer) clearInterval(this.beatTimer);
    if (this.bufTimer) clearTimeout(this.bufTimer);
    this.tickTimer = this.beatTimer = this.bufTimer = null;
    this.release?.();
    this.release = null;
    this.reported = false;
    this.nudging = false;
    this.loadedKey = '';
  }

  /* ------------------------------------------------------------------ clock */

  private async sampleClock(n: number) {
    for (let i = 0; i < n; i++) {
      const sent = this.deps.now();
      const r = await this.deps.api.time();
      const received = this.deps.now();
      this.clock.add({ serverMs: r.server_time_ms, sentMs: sent, receivedMs: received });
      this.lastRtt = received - sent;
    }
    this.set({ offsetMs: this.clock.offsetMs, rttMs: this.clock.rttMs });
  }

  private serverNow() {
    return this.clock.serverNow(this.deps.now());
  }

  private async beat() {
    const party = this.state.party;
    if (!party || this.state.status !== 'active') return;
    try {
      const sent = this.deps.now();
      const r = await this.deps.api.heartbeat(party.id, {
        rtt_ms: this.lastRtt != null ? Math.round(this.lastRtt) : undefined,
      });
      const received = this.deps.now();
      this.clock.add({ serverMs: r.server_time_ms, sentMs: sent, receivedMs: received });
      this.lastRtt = received - sent;
      this.set({ offsetMs: this.clock.offsetMs, rttMs: this.clock.rttMs });
      if (!r.member) {
        this.finish('removed', 'You are no longer in this watch party.');
        return;
      }
      // A lost gateway event is repaired here.
      this.receive(r.playback, 'heartbeat');
    } catch {
      /* the next beat tries again */
    }
  }

  /* ----------------------------------------------------------------- state */

  private adopt(party: Party) {
    const prev = this.state.party;
    // Never step back to an older playback than the one already applied.
    const playback =
      prev && prev.playback.version > party.playback.version ? prev.playback : party.playback;
    const merged: Party = {
      ...party,
      playback,
      invite_code: party.invite_code ?? prev?.invite_code ?? null,
      you: party.you ?? prev?.you ?? null,
    };
    this.set({
      party: merged,
      waitingFor: merged.participants.filter((p) => p.buffering).map(nameOf),
    });
  }

  /** A party object from REST or a gateway update: adopt it, then act on a newer playback inside it. */
  private absorb(party: Party) {
    const applied = this.state.appliedVersion;
    this.adopt(party);
    if (party.playback.version > applied) {
      this.set({ appliedVersion: party.playback.version });
      if (this.mediaKey(party.media) === this.loadedKey) void this.apply();
    }
    if (this.mediaKey(party.media) !== this.loadedKey) void this.openMedia(party.media, false);
  }

  private finish(kind: 'removed' | 'ended', message: string) {
    this.teardown();
    this.deps.player.pause();
    this.set({ status: 'ended', error: { kind, message } });
  }

  /** The `you` of a gateway party object, from what the last REST read said plus what the event shows. */
  private async refreshYou(party: Party) {
    try {
      const fresh = await this.deps.api.get(party.id);
      if (this.state.party?.id === fresh.id) this.absorb(fresh);
    } catch {
      /* keep what we have */
    }
  }

  /* -------------------------------------------------------------- gateway */

  private listen() {
    this.unsubs.push(
      this.deps.onDispatch((event, data) => {
        const id = this.state.party?.id;
        if (!id) return;
        switch (event) {
          case 'WATCH_PARTY_UPDATE':
            this.onUpdate(data as PartyUpdateEvent);
            break;
          case 'WATCH_PARTY_PLAYBACK': {
            const e = data as PlaybackEvent;
            if (e.party_id === id) this.receive(e.playback, e.reason);
            break;
          }
          case 'WATCH_PARTY_REACTION': {
            const e = data as ReactionEvent;
            if (e.party_id === id) this.onReaction(e);
            break;
          }
          case 'WATCH_PARTY_MESSAGE': {
            const e = data as MessageEvent;
            if (e.party_id === id) this.onMessage(e);
            break;
          }
        }
      }),
      this.deps.player.subscribe((e) => this.onPlayer(e)),
    );
  }

  private onUpdate(e: PartyUpdateEvent) {
    const cur = this.state.party;
    if (!cur) return;
    if ('party_id' in e && e.type === 'removed') {
      if (e.party_id !== cur.id) return;
      const age = e.reason === 'age_restricted';
      this.teardown();
      this.deps.player.pause();
      this.set({
        status: 'ended',
        error: {
          kind: age ? 'age' : 'removed',
          message: age
            ? 'This party is showing age-restricted content.'
            : 'You were removed from this watch party.',
        },
      });
      return;
    }
    if (!('party' in e) || e.party.id !== cur.id) return;
    const next = e.party;
    if (next.ended) {
      this.finish('ended', 'The host ended this watch party.');
      return;
    }
    const me = this.deps.me();
    const mineBefore = cur.participants.find((p) => p.user_id === me);
    const mineAfter = next.participants.find((p) => p.user_id === me);
    if (!mineAfter) {
      this.finish('removed', 'You are no longer in this watch party.');
      return;
    }
    const rolesChanged =
      next.host_id !== cur.host_id ||
      mineBefore?.is_controller !== mineAfter.is_controller ||
      next.settings.shared_control !== cur.settings.shared_control ||
      next.settings.queue_policy !== cur.settings.queue_policy;
    this.absorb({ ...next, you: cur.you });
    if (rolesChanged) void this.refreshYou(next);
    // Participants who stopped being stalled are no longer waited for.
    this.set({ waitingFor: next.participants.filter((p) => p.buffering).map(nameOf) });
  }

  private onReaction(e: ReactionEvent) {
    if (e.emoji === HAND) {
      if (!this.state.hands.includes(e.user_id))
        this.set({ hands: [...this.state.hands, e.user_id] });
      setTimeout(() => this.lowerHand(e.user_id), HAND_MS);
    }
    const id = ++this.seq;
    this.set({ reactions: [...this.state.reactions, { id, userId: e.user_id, emoji: e.emoji }] });
    setTimeout(
      () => this.set({ reactions: this.state.reactions.filter((r) => r.id !== id) }),
      REACTION_MS,
    );
  }

  private onMessage(e: MessageEvent) {
    const line = { id: ++this.seq, userId: e.user_id, content: e.content, atMs: e.at_ms };
    this.set({ messages: [...this.state.messages, line].slice(-MAX_LINES) });
  }

  lowerHand(userId: string) {
    this.set({ hands: this.state.hands.filter((h) => h !== userId) });
  }

  /* -------------------------------------------------------------- playback */

  private mediaKey(m: PartyMedia | null) {
    return m ? `${m.asset_id}:${m.title_id}:${m.episode_id}` : '';
  }

  private async openMedia(media: PartyMedia | null, first: boolean) {
    // An empty room has nothing to open; the theatre asks its host to pick something.
    if (!media) {
      this.loadedKey = '';
      if (first) this.set({ needsGesture: false });
      return;
    }
    const key = this.mediaKey(media);
    if (key === this.loadedKey) return;
    this.loadedKey = key;
    this.endedSentFor = '';
    const pb = this.state.party?.playback;
    if (!pb) return;
    const startMs = expectedPositionMs(pb, this.serverNow());
    const autoplay = pb.playing && !pb.auto_paused;
    this.loading = true;
    try {
      await this.deps.load(media, startMs, autoplay);
    } catch (e) {
      this.set({ error: classify(e) });
      return;
    } finally {
      this.loading = false;
    }
    if (first) this.set({ needsGesture: false });
    await this.apply();
  }

  /** A playback state from an event, a command answer or a heartbeat. Older or equal versions are ignored. */
  receive(pb: Playback, reason = 'event') {
    const party = this.state.party;
    if (!party || pb.version <= this.state.appliedVersion) return;
    this.set({
      appliedVersion: pb.version,
      party: { ...party, playback: pb },
    });
    if (reason === 'media') {
      // The party update carries the new media; make sure it is loaded before applying.
      if (this.mediaKey(party.media) !== this.loadedKey) return;
    }
    void this.apply();
  }

  private async apply() {
    const party = this.state.party;
    if (!party || !party.media || this.state.status !== 'active' || this.loading) return;
    const pb = party.playback;
    const p = this.deps.player;
    const expected = expectedPositionMs(pb, this.serverNow());
    this.nudging = false;
    if (pb.playing && !pb.auto_paused) {
      const drift = p.positionMs() - expected;
      const wasPaused = p.paused();
      p.setRate(pb.rate);
      // Starting or resuming lands on the expected position; a running player keeps going unless far off.
      if (Math.abs(drift) > DRIFT_SEEK_MS || (wasPaused && Math.abs(drift) > DRIFT_IGNORE_MS)) {
        p.seekMs(Math.round(expected));
        this.settleUntil = this.deps.now() + SETTLE_MS;
      }
      if (wasPaused) {
        const ok = await p.play();
        this.set({ needsGesture: !ok });
      }
    } else {
      p.pause();
      p.setRate(pb.rate);
      if (Math.abs(p.positionMs() - pb.position_ms) > 100) p.seekMs(pb.position_ms);
      this.set({ driftMs: 0 });
    }
  }

  /** The viewer clicked the "join playback" button after the browser refused to autoplay. */
  async resumeFromGesture() {
    const ok = await this.deps.player.play();
    this.set({ needsGesture: !ok });
    if (ok) void this.apply();
  }

  /** Every 500 ms while playing: measure the drift and correct it. */
  tick() {
    const party = this.state.party;
    if (!party || this.state.status !== 'active') return;
    const pb = party.playback;
    if (!pb.playing || pb.auto_paused || pb.live) return;
    const p = this.deps.player;
    if (p.waiting() || p.paused() || this.deps.now() < this.settleUntil) return;
    const expected = expectedPositionMs(pb, this.serverNow());
    const drift = p.positionMs() - expected;
    this.set({ driftMs: Math.round(drift) });
    const c = planCorrection(drift, this.nudging, pb.rate, expected);
    switch (c.kind) {
      case 'seek':
        p.seekMs(c.toMs);
        p.setRate(pb.rate);
        this.nudging = false;
        this.settleUntil = this.deps.now() + SETTLE_MS;
        break;
      case 'nudge':
        p.setRate(c.rate);
        this.nudging = true;
        break;
      case 'settle':
        p.setRate(pb.rate);
        this.nudging = false;
        break;
      case 'none':
        break;
    }
  }

  /* ----------------------------------------------------------- buffering */

  private onPlayer(e: 'waiting' | 'playing' | 'ended') {
    const party = this.state.party;
    if (!party || this.state.status !== 'active') return;
    if (e === 'waiting') {
      if (this.bufTimer || this.reported) return;
      this.bufTimer = setTimeout(() => {
        this.bufTimer = null;
        if (!this.deps.player.waiting()) return;
        this.reported = true;
        this.set({ localBuffering: true });
        void this.deps.api.heartbeat(party.id, { buffering: true }).catch(() => undefined);
      }, BUFFER_REPORT_MS);
    } else if (e === 'playing') {
      if (this.bufTimer) {
        clearTimeout(this.bufTimer);
        this.bufTimer = null;
      }
      if (this.reported) {
        this.reported = false;
        this.set({ localBuffering: false });
        void this.deps.api.heartbeat(party.id, { buffering: false }).catch(() => undefined);
      }
    } else if (e === 'ended') {
      const key = this.mediaKey(party.media);
      if (this.endedSentFor === key) return;
      this.endedSentFor = key;
      void this.send({ action: 'ended' });
    }
  }

  /* ------------------------------------------------------------- commands */

  /** Send a playback command and apply the answer. A stale `expected_version` is answered with the state. */
  async send(c: CommandInput): Promise<void> {
    const party = this.state.party;
    if (!party) return;
    try {
      const pb = await this.deps.api.command(party.id, c);
      this.receive(pb, 'command');
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) {
        const pb = (e.body as { playback?: Playback } | null)?.playback;
        if (pb) this.receive(pb, 'conflict');
        this.set({ notice: 'The playback just changed, so your action was not applied.' });
      } else if (e instanceof ApiError && e.status === 403) {
        this.set({ notice: 'You cannot control playback in this party.' });
      } else if (e instanceof ApiError && e.status === 429) {
        this.set({ notice: 'Slow down a little.' });
      } else {
        this.set({ notice: 'That did not work. Try again.' });
      }
    }
  }

  /** Play or pause, guarded by the version the viewer saw so a double toggle cannot undo a newer state. */
  toggle(): Promise<void> {
    const party = this.state.party;
    if (!party) return Promise.resolve();
    const pb = party.playback;
    const running = pb.playing || pb.auto_paused;
    return this.send({
      action: running ? 'pause' : 'play',
      expected_version: this.state.appliedVersion,
    });
  }

  seek(ms: number): Promise<void> {
    return this.send({ action: 'seek', position_ms: Math.max(0, Math.round(ms)) });
  }

  setRate(rate: number): Promise<void> {
    return this.send({ action: 'rate', rate });
  }

  /** Skip to the next queue entry (controllers). */
  async skip(): Promise<void> {
    const id = this.state.party?.id;
    if (!id) return;
    try {
      this.adoptRest(await this.deps.api.next(id));
    } catch (e) {
      this.set({
        notice:
          e instanceof ApiError && e.status === 409 ? 'The queue is empty.' : 'Could not skip.',
      });
    }
  }

  /** Mark the viewer ready (or not) for the start. */
  async setReady(ready: boolean): Promise<void> {
    const id = this.state.party?.id;
    if (!id) return;
    await this.deps.api.heartbeat(id, { ready }).catch(() => undefined);
  }

  get meId(): string {
    return this.deps.me();
  }

  clearNotice() {
    this.set({ notice: null });
  }

  /** Raise a hand: everyone sees it, the host can grant control. */
  requestControl(): Promise<void> {
    const id = this.state.party?.id;
    return id ? this.deps.api.react(id, HAND).catch(() => undefined) : Promise.resolve();
  }

  react(emoji: string): Promise<void> {
    const id = this.state.party?.id;
    return id ? this.deps.api.react(id, emoji).catch(() => undefined) : Promise.resolve();
  }

  say(content: string): Promise<void> {
    const id = this.state.party?.id;
    return id ? this.deps.api.say(id, content) : Promise.resolve();
  }

  /** Adopt a party object returned by a REST call (queue edits, settings). */
  adoptRest(party: Party) {
    if (party.id === this.state.party?.id) this.absorb(party);
  }
}
