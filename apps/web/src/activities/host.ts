// The client side of the Embedded App SDK protocol (DEV-ACT-003): answers one Activity iframe. Pure logic over
// a small `HostEnv`, so it is tested without a DOM. `ActivityFrame` wires it to a real iframe and checks that a
// message comes from that iframe's window before calling `handle`, which checks the origin.
import {
  EVENTS,
  PROTOCOL_VERSION,
  RPC_ERRORS,
  parseFrame,
  type AuthenticateResult,
  type EventName,
  type Frame,
  type Participant,
  type RpcErrorBody,
  type SpeakingUpdate,
  type VoiceStateUpdate,
} from '@sideroom/embedded-app-sdk';
import type { OAuthApi } from './api';
import type { ActivityInstance } from './types';

/** Scopes an Activity may ask for. `email` and the bot scopes are not an Activity's business. */
export const ACTIVITY_SCOPES = ['identify', 'guilds'] as const;

/** `setActivity` calls allowed per window; every one reaches the gateway and the person's friends. */
const SET_ACTIVITY_LIMIT = 5;
const SET_ACTIVITY_WINDOW_MS = 20_000;
const MAX_TEXT = 128;
const MAX_KEY = 256;

/** One person in the instance's voice channel, as the voice store reports them. */
export interface VoicePerson {
  userId: string;
  speaking: boolean;
  /** Muted by themselves or by a moderator. */
  muted: boolean;
  deafened: boolean;
  serverMuted: boolean;
  serverDeafened: boolean;
  suppressed: boolean;
}

/** The activity object the host sends over the gateway (presence activity of type Playing). */
export interface PresenceActivity {
  type: 0;
  name: string;
  application_id: string;
  platform: 'web';
  details?: string;
  state?: string;
  timestamps?: { start?: number; end?: number };
  party?: { id: string };
  assets?: {
    large_image?: string;
    large_text?: string;
    small_image?: string;
    small_text?: string;
  };
}

export interface HostEnv {
  /** The origin the Activity runs on; frames from anywhere else are dropped. */
  origin: string;
  applicationId: string;
  instance: () => ActivityInstance;
  channel: () => { id: string; guild_id: string; name: string };
  /** People by id (the live user store), unknown ids omitted. */
  users: (ids: string[]) => Participant[];
  oauth: OAuthApi;
  /** Ask the person; resolves false when they decline. */
  confirm: (
    ask: { kind: 'authorize'; scopes: string[] } | { kind: 'link'; url: string },
  ) => Promise<boolean>;
  openLink: (url: string) => void;
  post: (frame: Frame) => void;
  /** Set (or with `null` clear) the person's Rich Presence activity through the gateway. */
  setPresence: (activity: PresenceActivity | null) => void;
  /** The application's Rich Presence assets (name and id), for resolving `assets.*_image` names. */
  assets: () => Promise<{ id: string; name: string }[]>;
  /** Test seam. */
  now?: () => number;
}

class RpcError extends Error {
  readonly code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

const invalid = (m: string) => new RpcError(RPC_ERRORS.invalidArguments, m);

function obj(args: unknown): Record<string, unknown> {
  if (typeof args !== 'object' || args === null) throw invalid('Arguments must be an object.');
  return args as Record<string, unknown>;
}

/** `http` and `https` links only; anything else could run script or open a local file. */
export function safeExternalUrl(raw: unknown): string | null {
  if (typeof raw !== 'string' || raw.length > 2048) return null;
  try {
    const u = new URL(raw);
    return u.protocol === 'https:' || u.protocol === 'http:' ? u.toString() : null;
  } catch {
    return null;
  }
}

function text(v: unknown, field: string, max: number): string | undefined {
  if (v === undefined || v === null) return undefined;
  if (typeof v !== 'string') throw invalid(`${field} must be a string.`);
  const s = v.trim();
  if (s.length === 0) return undefined;
  if ([...s].length > max) throw invalid(`${field} must be ${max} characters or fewer.`);
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f]/.test(s))
    throw invalid(`${field} must not contain control characters.`);
  return s;
}

function millis(v: unknown, field: string): number | undefined {
  if (v === undefined || v === null) return undefined;
  if (typeof v !== 'number' || !Number.isFinite(v) || v < 0 || !Number.isInteger(v)) {
    throw invalid(`${field} must be a whole number of milliseconds.`);
  }
  return v;
}

/** The fields of an `ActivityInput` the host keeps (unknown fields are dropped), before asset resolution. */
interface CleanActivity {
  details?: string;
  state?: string;
  timestamps?: { start?: number; end?: number };
  party?: { id: string };
  assets?: {
    large_image?: string;
    large_text?: string;
    small_image?: string;
    small_text?: string;
  };
}

function cleanActivity(raw: unknown): CleanActivity {
  const a = obj(raw);
  const out: CleanActivity = {};
  const details = text(a.details, 'details', MAX_TEXT);
  if (details) out.details = details;
  const state = text(a.state, 'state', MAX_TEXT);
  if (state) out.state = state;
  if (a.timestamps !== undefined && a.timestamps !== null) {
    const t = obj(a.timestamps);
    const start = millis(t.start, 'timestamps.start');
    const end = millis(t.end, 'timestamps.end');
    if (start !== undefined || end !== undefined) {
      out.timestamps = {
        ...(start !== undefined ? { start } : {}),
        ...(end !== undefined ? { end } : {}),
      };
    }
  }
  if (a.party !== undefined && a.party !== null) {
    const id = text(obj(a.party).id, 'party.id', MAX_TEXT);
    if (id) out.party = { id };
  }
  if (a.assets !== undefined && a.assets !== null) {
    const s = obj(a.assets);
    const assets: NonNullable<CleanActivity['assets']> = {};
    for (const k of ['large_image', 'small_image'] as const) {
      const v = text(s[k], `assets.${k}`, MAX_KEY);
      if (v) assets[k] = v;
    }
    for (const k of ['large_text', 'small_text'] as const) {
      const v = text(s[k], `assets.${k}`, MAX_TEXT);
      if (v) assets[k] = v;
    }
    if (Object.keys(assets).length) out.assets = assets;
  }
  return out;
}

export interface ActivityHost {
  /** One message from the Activity; `origin` is the sender's. */
  handle(data: unknown, origin: string): Promise<void>;
  /** The instance changed: tell subscribers. */
  participantsChanged(): void;
  /**
   * The voice channel's people changed (voice states or speaking): tell subscribers of `VOICE_STATE_UPDATE`,
   * `SPEAKING_START` and `SPEAKING_STOP`. The first call only records the baseline.
   */
  voiceChanged(people: VoicePerson[]): void;
  /** The frame is going away (closed, left, instance ended): clear the Activity's presence. */
  dispose(): void;
}

export function createActivityHost(env: HostEnv): ActivityHost {
  let ready = false;
  let presenceSet = false;
  const subscribed = new Set<EventName>();
  const calls: number[] = [];
  const now = env.now ?? Date.now;
  let voiceBaseline: Map<string, VoicePerson> | null = null;

  const ok = (nonce: string, data: unknown) =>
    env.post({ sr: PROTOCOL_VERSION, kind: 'response', nonce, ok: true, data });
  const fail = (nonce: string, error: RpcErrorBody) =>
    env.post({ sr: PROTOCOL_VERSION, kind: 'response', nonce, ok: false, error });

  const participants = (): Participant[] =>
    env.users(env.instance().participants.map((p) => p.user_id));

  const emit = (event: EventName, data: unknown) => {
    if (ready && subscribed.has(event))
      env.post({ sr: PROTOCOL_VERSION, kind: 'event', event, data });
  };

  const setActivity = async (args: unknown): Promise<unknown> => {
    const raw = obj(args).activity;
    if (raw === null) {
      if (presenceSet) env.setPresence(null);
      presenceSet = false;
      return null;
    }
    const clean = cleanActivity(raw);
    const t = now();
    while (calls.length && t - (calls[0] ?? 0) > SET_ACTIVITY_WINDOW_MS) calls.shift();
    if (calls.length >= SET_ACTIVITY_LIMIT) {
      throw new RpcError(
        RPC_ERRORS.rateLimited,
        'setActivity was called too often. Try again soon.',
      );
    }
    calls.push(t);

    // Images are names of the application's Rich Presence assets; a name nobody uploaded is dropped.
    let assets = clean.assets;
    if (assets && (assets.large_image || assets.small_image)) {
      const known = await env.assets().catch(() => []);
      const byName = new Map(known.map((a) => [a.name, a.id]));
      assets = { ...assets };
      for (const k of ['large_image', 'small_image'] as const) {
        const name = assets[k];
        if (name === undefined) continue;
        const id = byName.get(name);
        if (id) assets[k] = id;
        else delete assets[k];
      }
      if (!Object.keys(assets).length) assets = undefined;
    }
    const inst = env.instance();
    const presence: PresenceActivity = {
      type: 0,
      name: inst.application_name,
      application_id: env.applicationId,
      platform: 'web',
      ...(clean.details ? { details: clean.details } : {}),
      ...(clean.state ? { state: clean.state } : {}),
      ...(clean.timestamps ? { timestamps: clean.timestamps } : {}),
      ...(clean.party ? { party: clean.party } : {}),
      ...(assets ? { assets } : {}),
    };
    env.setPresence(presence);
    presenceSet = true;
    return {
      ...(clean.details ? { details: clean.details } : {}),
      ...(clean.state ? { state: clean.state } : {}),
      ...(clean.timestamps ? { timestamps: clean.timestamps } : {}),
      ...(clean.party ? { party: clean.party } : {}),
      ...(assets ? { assets } : {}),
    };
  };

  const run = async (cmd: string, args: unknown): Promise<unknown> => {
    switch (cmd) {
      case 'authorize': {
        const scopes = obj(args).scope;
        if (
          !Array.isArray(scopes) ||
          scopes.length === 0 ||
          !scopes.every((s) => (ACTIVITY_SCOPES as readonly unknown[]).includes(s))
        ) {
          throw invalid(`scope must list some of: ${ACTIVITY_SCOPES.join(', ')}.`);
        }
        const wanted = [...new Set(scopes as string[])];
        if (!(await env.confirm({ kind: 'authorize', scopes: wanted }))) {
          throw new RpcError(RPC_ERRORS.denied, 'The person declined.');
        }
        const code = await env.oauth.authorizeCode(env.applicationId, wanted, `${env.origin}/`);
        return { code };
      }
      case 'authenticate': {
        const token = obj(args).access_token;
        if (typeof token !== 'string' || token.length === 0 || token.length > 512) {
          throw invalid('access_token must be a string.');
        }
        const info: AuthenticateResult = await env.oauth.tokenInfo(token);
        // A token of another application is not this Activity's business.
        if (info.application.id !== env.applicationId) {
          throw new RpcError(
            RPC_ERRORS.notAuthorized,
            'That token belongs to another application.',
          );
        }
        return info;
      }
      case 'getChannel':
        return env.channel();
      case 'getInstanceConnectedParticipants':
        return { participants: participants() };
      case 'openExternalLink': {
        const url = safeExternalUrl(obj(args).url);
        if (!url) throw invalid('url must be an http or https link.');
        if (!(await env.confirm({ kind: 'link', url }))) return { opened: false };
        env.openLink(url);
        return { opened: true };
      }
      case 'setActivity':
        return setActivity(args);
      case 'subscribe':
      case 'unsubscribe': {
        const a = obj(args);
        const ev = a.event;
        if (typeof ev !== 'string' || !(EVENTS as readonly string[]).includes(ev)) {
          throw invalid('Unknown event.');
        }
        if (cmd === 'subscribe') {
          // Voice events are about the instance's own channel and nothing else.
          if (a.args !== undefined && a.args !== null) {
            const channel = obj(a.args).channel_id;
            if (channel !== undefined && channel !== env.instance().channel_id) {
              throw new RpcError(
                RPC_ERRORS.notAuthorized,
                'Events are only available for the channel the Activity runs in.',
              );
            }
          }
          subscribed.add(ev as EventName);
        } else {
          subscribed.delete(ev as EventName);
        }
        return {};
      }
      default:
        throw new RpcError(RPC_ERRORS.unknownCommand, `Unknown command ${cmd}.`);
    }
  };

  const userOf = (id: string): Participant =>
    env.users([id])[0] ?? { id, username: null, global_name: null, avatar: null };

  return {
    async handle(data, origin) {
      if (origin !== env.origin) return;
      const f = parseFrame(data);
      if (!f) return;
      if (f.kind === 'handshake') {
        if (f.application_id !== env.applicationId) return;
        ready = true;
        const inst = env.instance();
        env.post({
          sr: PROTOCOL_VERSION,
          kind: 'ready',
          config: {
            application_id: env.applicationId,
            instance_id: inst.id,
            channel_id: inst.channel_id,
            guild_id: inst.guild_id,
          },
        });
        return;
      }
      if (f.kind !== 'request') return;
      if (!ready) {
        fail(f.nonce, { code: RPC_ERRORS.notAuthorized, message: 'Call ready() first.' });
        return;
      }
      try {
        ok(f.nonce, await run(f.cmd, f.args));
      } catch (e) {
        if (e instanceof RpcError) fail(f.nonce, { code: e.code, message: e.message });
        else fail(f.nonce, { code: RPC_ERRORS.failed, message: 'The request failed.' });
      }
    },
    participantsChanged() {
      emit('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE', { participants: participants() });
    },
    voiceChanged(people) {
      const next = new Map(people.map((p) => [p.userId, p]));
      const prev = voiceBaseline;
      voiceBaseline = next;
      if (!prev) return;
      const channel_id = env.instance().channel_id;
      for (const p of people) {
        const was = prev.get(p.userId);
        const stateChanged =
          !was ||
          was.muted !== p.muted ||
          was.deafened !== p.deafened ||
          was.serverMuted !== p.serverMuted ||
          was.serverDeafened !== p.serverDeafened ||
          was.suppressed !== p.suppressed;
        if (stateChanged) {
          const data: VoiceStateUpdate = {
            voice_state: {
              mute: p.serverMuted,
              deaf: p.serverDeafened,
              self_mute: p.muted && !p.serverMuted,
              self_deaf: p.deafened && !p.serverDeafened,
              suppress: p.suppressed,
            },
            user: userOf(p.userId),
            nick: null,
            volume: 100,
            mute: p.muted,
            pan: { left: 100, right: 100 },
          };
          emit('VOICE_STATE_UPDATE', data);
        }
        if ((was?.speaking ?? false) !== p.speaking) {
          const data: SpeakingUpdate = { channel_id, user_id: p.userId };
          emit(p.speaking ? 'SPEAKING_START' : 'SPEAKING_STOP', data);
        }
      }
      // Somebody who was speaking and left has stopped.
      for (const was of prev.values()) {
        if (was.speaking && !next.has(was.userId)) {
          emit('SPEAKING_STOP', { channel_id, user_id: was.userId } satisfies SpeakingUpdate);
        }
      }
    },
    dispose() {
      if (presenceSet) env.setPresence(null);
      presenceSet = false;
      subscribed.clear();
      ready = false;
    },
  };
}
