// Test doubles for watch parties: a player with its own (drifting) clock, and an in-memory server that does the
// same playback arithmetic as crates/sr-watchparty/src/state.rs, so controller tests exercise the real protocol
// without a backend.
import { vi } from 'vitest';
import { ApiError } from '../../api/client';
import type { PartyApi } from '../api';
import type { ControllerDeps, SyncPlayer } from '../controller';
import type {
  CommandInput,
  HeartbeatInput,
  HeartbeatResult,
  Participant,
  Party,
  PartyMedia,
  Playback,
} from '../types';

/** A player whose media clock runs at `speed` (1 = exact) from the shared (fake) time. */
export class FakePlayer implements SyncPlayer {
  private pos = 0;
  private at: number;
  private isPaused = true;
  private rate = 1;
  isWaiting = false;
  blocked = false;
  /** How much faster (+) or slower (-) than real time the media clock runs. */
  speedError = 0;
  seeks: number[] = [];
  rates: number[] = [];
  plays = 0;
  private listeners = new Set<(e: 'waiting' | 'playing' | 'ended') => void>();

  constructor(private readonly now: () => number) {
    this.at = now();
  }

  private settle() {
    const t = this.now();
    if (!this.isPaused && !this.isWaiting)
      this.pos += (t - this.at) * this.rate * (1 + this.speedError);
    this.at = t;
  }
  positionMs() {
    this.settle();
    return this.pos;
  }
  paused() {
    return this.isPaused;
  }
  waiting() {
    return this.isWaiting;
  }
  async play() {
    this.plays++;
    if (this.blocked) return false;
    this.settle();
    this.isPaused = false;
    return true;
  }
  pause() {
    this.settle();
    this.isPaused = true;
  }
  seekMs(ms: number) {
    this.settle();
    this.pos = ms;
    this.seeks.push(ms);
  }
  setRate(r: number) {
    this.settle();
    this.rate = r;
    this.rates.push(r);
  }
  currentRate() {
    return this.rate;
  }
  subscribe(fn: (e: 'waiting' | 'playing' | 'ended') => void) {
    this.listeners.add(fn);
    return () => void this.listeners.delete(fn);
  }
  emit(e: 'waiting' | 'playing' | 'ended') {
    if (e === 'waiting') this.isWaiting = true;
    if (e === 'playing') this.isWaiting = false;
    for (const f of [...this.listeners]) f(e);
  }
}

export const MEDIA: PartyMedia = {
  title_id: '10',
  episode_id: '0',
  variant_id: null,
  asset_id: '501',
  title: 'Akira',
  episode_label: null,
  duration_ms: 7_200_000,
  age_restricted: false,
  live: false,
};

export function person(id: string, o: Partial<Participant> = {}): Participant {
  return {
    user_id: id,
    username: `user${id}`,
    display_name: null,
    joined_at_ms: 1,
    is_host: false,
    is_controller: false,
    ready: false,
    buffering: false,
    rtt_ms: null,
    ...o,
  };
}

/** In-memory server for one party. `now` is the SERVER clock. */
export class FakeServer {
  pb: Playback;
  party: Party;
  commands: CommandInput[] = [];
  heartbeats: HeartbeatInput[] = [];
  member = true;
  failNext: ApiError | null = null;
  listeners = new Set<(event: string, data: unknown) => void>();
  latency = 0;

  constructor(
    public now: () => number,
    me = '1',
    o: { host?: boolean; control?: boolean; media?: PartyMedia } = {},
  ) {
    const t = now();
    this.pb = {
      version: 1,
      playing: false,
      position_ms: 0,
      rate: 1,
      at_ms: t,
      server_time_ms: t,
      expected_position_ms: 0,
      auto_paused: false,
      duration_ms: (o.media ?? MEDIA).duration_ms,
      live: false,
    };
    const host = o.host ?? true;
    this.party = {
      id: '900',
      host_id: host ? me : '2',
      binding: 'private',
      guild_id: null,
      channel_id: null,
      invite_code: 'abc',
      media: o.media ?? MEDIA,
      playback: this.pb,
      settings: {
        shared_control: 'host',
        queue_policy: 'controllers',
        auto_pause_buffering: true,
        max_wait_ms: 10_000,
        persistent: false,
        max_participants: 50,
      },
      participants: [
        person(me, { is_host: host }),
        ...(host ? [person('2')] : [person('2', { is_host: true })]),
      ],
      queue: [],
      created_at_ms: t,
      ended: false,
      you: {
        member: true,
        host,
        can_control: o.control ?? host,
        can_queue: o.control ?? host,
        can_manage: host,
      },
    };
  }

  position(at = this.now()) {
    const p = this.pb;
    let pos = p.position_ms;
    if (p.playing) pos += Math.max(0, at - p.at_ms) * p.rate;
    if (p.duration_ms != null) pos = Math.min(pos, p.duration_ms);
    return pos;
  }

  view(): Playback {
    const t = this.now();
    return { ...this.pb, server_time_ms: t, expected_position_ms: this.position(t) };
  }

  /** Apply a state change like the Rust state machine does (rebase at now, bump the version). */
  change(patch: Partial<Playback>): Playback {
    const t = this.now();
    this.pb = {
      ...this.pb,
      position_ms: this.position(t),
      at_ms: t,
      version: this.pb.version + 1,
      ...patch,
    };
    this.party = { ...this.party, playback: this.pb };
    return this.view();
  }

  emitPlayback(reason: string, by: string | null = null) {
    this.emit('WATCH_PARTY_PLAYBACK', {
      party_id: this.party.id,
      reason,
      by,
      playback: this.view(),
    });
  }

  emit(event: string, data: unknown) {
    for (const l of [...this.listeners]) l(event, data);
  }

  api(): PartyApi {
    // The returned methods are plain object members, so they need a handle on the server.
    // eslint-disable-next-line @typescript-eslint/no-this-alias
    const s = this;
    const guard = () => {
      if (s.failNext) {
        const e = s.failNext;
        s.failNext = null;
        throw e;
      }
    };
    const party = () => ({ ...s.party, playback: s.view() });
    return {
      time: async () => ({ server_time_ms: s.now() }),
      get: async () => {
        guard();
        return party();
      },
      mine: async () => party(),
      inChannel: async () => party(),
      create: async () => party(),
      join: async () => party(),
      leave: vi.fn(async () => undefined),
      end: vi.fn(async () => undefined),
      updateSettings: async (_id, p) => {
        s.party = { ...s.party, settings: { ...s.party.settings, ...p } };
        return party();
      },
      command: async (_id, c) => {
        guard();
        s.commands.push(c);
        if (c.expected_version != null && c.expected_version !== s.pb.version) {
          throw new ApiError(409, 'stale', {
            code: 900010,
            message: 'stale',
            playback: s.view(),
          } as never);
        }
        const before = s.pb.version;
        switch (c.action) {
          case 'play':
            if (!s.pb.playing) s.change({ playing: true, auto_paused: false });
            break;
          case 'pause':
            if (s.pb.playing || s.pb.auto_paused) s.change({ playing: false, auto_paused: false });
            break;
          case 'seek':
            s.change({ position_ms: c.position_ms ?? 0 });
            break;
          case 'rate':
            s.change({ rate: c.rate ?? 1 });
            break;
          case 'ended':
            break;
        }
        // Like the gateway: everybody hears about it, the commander also gets the answer.
        if (s.pb.version !== before) s.emitPlayback(c.action, '1');
        return s.view();
      },
      changeMedia: async () => party(),
      queueAdd: async () => party(),
      queueMove: async () => party(),
      queueRemove: async () => party(),
      next: async () => party(),
      heartbeat: async (_id, b): Promise<HeartbeatResult> => {
        s.heartbeats.push(b);
        return { server_time_ms: s.now(), playback: s.view(), member: s.member };
      },
      react: vi.fn(async () => undefined),
      say: vi.fn(async () => undefined),
      transferHost: async () => party(),
      setController: async () => party(),
      kick: async () => undefined,
    };
  }
}

export function deps(
  server: FakeServer,
  player: SyncPlayer,
  o: Partial<ControllerDeps> = {},
): ControllerDeps & { load: ReturnType<typeof vi.fn> } {
  const load = vi.fn(async () => undefined);
  return {
    api: server.api(),
    player,
    load,
    now: () => Date.now(),
    me: () => '1',
    onDispatch: (fn) => {
      server.listeners.add(fn);
      return () => void server.listeners.delete(fn);
    },
    ...o,
  } as ControllerDeps & { load: ReturnType<typeof vi.fn> };
}
