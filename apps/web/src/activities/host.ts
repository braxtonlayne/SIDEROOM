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
  b64,
  unb64,
  MAX_DATA_BYTES,
  SAVE_FILE_MAX_BYTES,
  SAVE_FILE_TYPES,
  STORAGE_CAPABILITY,
  STORAGE_LIMITS,
  BYOS_READ_CAPABILITY,
  SHELF_OBJECT_BYTES_MAX,
  isShelfObjectRef,
  isPickShelfObjectArgs,
  isShelfObjectPick,
  type ShelfObjectPick,
  type ShelfObjectUrl,
  type RpcErrorBody,
  type SpeakingUpdate,
  type VoiceStateUpdate,
  type HostChanged,
} from '@sideroom/embedded-app-sdk';
import { bridgeInput, recordTick } from '../crash/ringBuffer';
import type { OAuthApi } from './api';
import { hostOf, type ActivityInstance } from './types';
import { pickable } from './shelfPick';

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

/**
 * The origin a message from a sandboxed Activity frame carries. The frame has `sandbox` without
 * `allow-same-origin`, so its origin is opaque and the browser names it "null"; the host tells frames apart by
 * the window a message came from (`ActivityFrame` checks that), never by this string.
 */
export const OPAQUE_ORIGIN = 'null';

/** The member's per-application key-value store behind the `storage` capability (a browser-side port). */
export interface ActivityStorage {
  all(): Record<string, string>;
  set(key: string, value: string): void;
  remove(key: string): void;
}

/** Most characters of one request's arguments; larger frames are refused before any command runs. */
export const MAX_REQUEST_CHARS = 12 * 1024 * 1024;
/** Requests allowed per window from one frame; a flood is answered `RATE_LIMITED` and costs the host little. */
const REQUEST_LIMIT = 4000;
const REQUEST_WINDOW_MS = 10_000;

/** Why a request frame is malformed, or `null`: lengths of the envelope fields and a bounded argument size. */
export function requestProblem(f: { nonce: string; cmd: string; args?: unknown }): string | null {
  if (f.nonce.length === 0 || f.nonce.length > 64) return 'nonce must be 1 to 64 characters.';
  if (f.cmd.length === 0 || f.cmd.length > 64) return 'cmd must be 1 to 64 characters.';
  if (f.args === undefined) return null;
  let size = 0;
  let nodes = 0;
  const stack: unknown[] = [f.args];
  while (stack.length) {
    const v = stack.pop();
    if (++nodes > 20_000) return 'args are too large.';
    if (typeof v === 'string') size += v.length;
    else if (typeof v === 'number' || typeof v === 'boolean') size += 8;
    else if (Array.isArray(v)) stack.push(...v);
    else if (typeof v === 'object' && v !== null) {
      for (const k of Object.keys(v)) {
        if (k === '__proto__') return 'args must not contain __proto__.';
        size += k.length;
      }
      stack.push(...Object.values(v));
    } else if (typeof v === 'function' || typeof v === 'symbol' || typeof v === 'bigint') {
      return 'args must be plain data.';
    }
    if (size > MAX_REQUEST_CHARS) return 'args are too large.';
  }
  return null;
}

export interface HostEnv {
  /** The origin the Activity is served from (its OAuth redirect base). */
  origin: string;
  /**
   * The origin string messages from the frame carry. Default `origin` (a frame that keeps its origin); the
   * sandboxed frame is `OPAQUE_ORIGIN`. Frames from any other origin are dropped.
   */
  senderOrigin?: string;
  /** Where `storageSet` and `storageRemove` keep the member's values; without it the capability is off. */
  storage?: ActivityStorage;
  /**
   * Mint the member's proxy token for the instance (D-399), sent to the frame in `ready` so its writes through
   * the app's URL mappings pass the proxy. Absent or failing: no token.
   */
  proxyToken?: (instanceId: string) => Promise<string | null>;
  /** Hand the person a file the Activity produced (after they agreed); the host page downloads it. */
  saveFile?: (name: string, mime: string, bytes: Uint8Array) => void;
  /**
   * D-389: mint the per-object URL of one shelf object with the member's own session
   * (`GET /shelves/{shelf_id}/objects/{object_hash}/url`, which checks the grant and every delisting). Absent: no
   * `shelfObjectUrl`.
   */
  shelfObjectUrl?: (shelfId: string, objectHash: string) => Promise<ShelfObjectUrl>;
  /**
   * D-460: fetch the bytes behind a per-object URL from this (the member's) page, without credentials and at most
   * `max` bytes; rejects past the cap. Default: `fetch`. Tests pass their own.
   */
  fetchShelfObject?: (url: string, max: number) => Promise<Uint8Array>;
  /**
   * D-466: open the app's own shelf picker (never inside the frame) and resolve the one object the member chose, or
   * `null` when they cancel. `signal` aborts when the frame goes away (the picker closes and resolves `null`).
   * Absent: no `pickShelfObject`.
   */
  pickShelfObject?: (
    req: { accept: string[]; maxSize: number | null },
    signal: AbortSignal,
  ) => Promise<ShelfObjectPick | null>;
  applicationId: string;
  instance: () => ActivityInstance;
  channel: () => { id: string; guild_id: string; name: string };
  /** People by id (the live user store), unknown ids omitted. */
  users: (ids: string[]) => Participant[];
  oauth: OAuthApi;
  /** Ask the person; resolves false when they decline. */
  confirm: (
    ask:
      | { kind: 'authorize'; scopes: string[] }
      | { kind: 'link'; url: string }
      | { kind: 'save'; name: string; mime: string; size: number }
      | { kind: 'shelf'; shelfId: string; objectHash: string },
  ) => Promise<boolean>;
  openLink: (url: string) => void;
  post: (frame: Frame) => void;
  /** Set (or with `null` clear) the person's Rich Presence activity through the gateway. */
  setPresence: (activity: PresenceActivity | null) => void;
  /** The application's Rich Presence assets (name and id), for resolving `assets.*_image` names. */
  assets: () => Promise<{ id: string; name: string }[]>;
  /** The signed-in member (sent to the Activity in `ready`). */
  selfId?: () => string | null;
  /**
   * The room's activity data channel (ACT-CH-001), present for an instance in a voice channel. The host page
   * seals every message with the room's key; the SFU relays opaque bytes to the instance's other members.
   */
  data?: ActivityDataBridge;
  /**
   * ARC-GST-002 (D-397): the guests of this instance's guest link, by lobby callsign only (no handle, no avatar).
   * They join the participant list the game sees, because they are in its session; they are in nothing else.
   */
  guests?: () => Participant[];
  /** Test seam. */
  now?: () => number;
}

/** What the host needs from the voice engine's data channel (see `voice/dataPort.ts`). */
export interface ActivityDataBridge {
  join(instance: string): void;
  leave(instance: string): void;
  send(instance: string, bytes: Uint8Array): Promise<boolean>;
  onMessage(cb: (instance: string, sender: string, bytes: Uint8Array) => void): () => void;
}

/** The capability a bundle declares in `sideroom.json` to use `sdk.state` and `sdk.input`. */
export const DATA_CAPABILITY = 'datachannel';

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
   * D-388: the instance's host may have moved (`instance().host_id`). Tells a subscribed Activity `HOST_CHANGED`
   * once per change; `participantsChanged` checks it too.
   */
  hostChanged(): void;
  /**
   * The voice channel's people changed (voice states or speaking): tell subscribers of `VOICE_STATE_UPDATE`,
   * `SPEAKING_START` and `SPEAKING_STOP`. The first call only records the baseline.
   */
  voiceChanged(people: VoicePerson[]): void;
  /** The frame is going away (closed, left, instance ended): clear the Activity's presence. */
  dispose(): void;
}

/** The object is over the cap (D-460); the game is told so, and told the cap. */
export class ShelfObjectTooBigError extends Error {
  constructor() {
    super('too big');
    this.name = 'ShelfObjectTooBigError';
  }
}

/**
 * D-460: GET a per-object URL without credentials, refusing more than `max` bytes. The body is read as a stream and
 * the fetch is aborted as soon as the running total passes the cap, so a body without a content-length never
 * downloads whole.
 */
export async function fetchCapped(url: string, max: number): Promise<Uint8Array> {
  const ctl = new AbortController();
  const res = await fetch(url, {
    credentials: 'omit',
    cache: 'no-store',
    referrerPolicy: 'no-referrer',
    signal: ctl.signal,
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  if (Number(res.headers.get('content-length') ?? 0) > max) {
    ctl.abort();
    throw new ShelfObjectTooBigError();
  }
  if (!res.body) {
    const bytes = new Uint8Array(await res.arrayBuffer());
    if (bytes.length > max) throw new ShelfObjectTooBigError();
    return bytes;
  }
  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.length;
    if (total > max) {
      ctl.abort();
      await reader.cancel().catch(() => undefined);
      throw new ShelfObjectTooBigError();
    }
    chunks.push(value);
  }
  const out = new Uint8Array(total);
  let at = 0;
  for (const c of chunks) {
    out.set(c, at);
    at += c.length;
  }
  return out;
}

const tooBig = () => new RpcError(RPC_ERRORS.failed, 'That file is over 1 MiB.');

export function createActivityHost(env: HostEnv): ActivityHost {
  let ready = false;
  let presenceSet = false;
  const subscribed = new Set<EventName>();
  const calls: number[] = [];
  const requests: number[] = [];
  const now = env.now ?? Date.now;
  let voiceBaseline: Map<string, VoicePerson> | null = null;
  let dataJoined: { instance: string; off: () => void } | null = null;
  /** The host the Activity was last told about (in `ready` or `HOST_CHANGED`). */
  let toldHost: string | null = null;

  const hasStorage = () =>
    !!env.storage && (env.instance().capabilities ?? []).includes(STORAGE_CAPABILITY);

  /** Shelf objects the member let this frame open (asked once per object, or picked: D-466). */
  const shelfAllowed = new Set<string>();
  /** The open picker, if any (one at a time, D-466). */
  let picking: AbortController | null = null;
  const hasByosRead = () =>
    !!env.shelfObjectUrl && (env.instance().capabilities ?? []).includes(BYOS_READ_CAPABILITY);

  const hasData = () => !!env.data && (env.instance().capabilities ?? []).includes(DATA_CAPABILITY);

  /** Start relaying the instance's data messages to the Activity (once the handshake is done). */
  const joinData = () => {
    const data = env.data;
    if (!data || !hasData() || dataJoined) return;
    const instance = env.instance().id;
    data.join(instance);
    const off = data.onMessage((inst, sender, bytes) => {
      if (inst === instance) emit('DATA_MESSAGE', { from: sender, b: b64(bytes) });
    });
    dataJoined = { instance, off };
  };

  const ok = (nonce: string, data: unknown) =>
    env.post({ sr: PROTOCOL_VERSION, kind: 'response', nonce, ok: true, data });
  const fail = (nonce: string, error: RpcErrorBody) =>
    env.post({ sr: PROTOCOL_VERSION, kind: 'response', nonce, ok: false, error });

  const participants = (): Participant[] => [
    ...env.users(env.instance().participants.map((p) => p.user_id)),
    ...(env.guests?.() ?? []),
  ];

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
    // OPS-CRASH-001: the command, as an opaque enum, goes into the crash ring buffer; its arguments never do.
    recordTick('activity', bridgeInput(cmd));
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
      case 'dataSend': {
        if (!env.data || !hasData()) {
          throw new RpcError(
            RPC_ERRORS.notAuthorized,
            'This Activity did not declare the datachannel capability.',
          );
        }
        const b = obj(args).b;
        if (typeof b !== 'string' || b.length > Math.ceil((MAX_DATA_BYTES * 4) / 3) + 8) {
          throw invalid('b must be base64 of at most 16 KiB.');
        }
        let bytes: Uint8Array;
        try {
          bytes = unb64(b);
        } catch {
          throw invalid('b must be base64.');
        }
        if (bytes.length > MAX_DATA_BYTES) throw invalid('b must be base64 of at most 16 KiB.');
        return { sent: await env.data.send(env.instance().id, bytes) };
      }
      case 'storageSet':
      case 'storageRemove': {
        const store = env.storage;
        if (!store || !hasStorage()) {
          throw new RpcError(
            RPC_ERRORS.notAuthorized,
            'This Activity did not declare the storage capability.',
          );
        }
        const a = obj(args);
        const key = a.key;
        if (
          typeof key !== 'string' ||
          key.length === 0 ||
          key.length > STORAGE_LIMITS.keyChars ||
          !/^[\w.:@-]+$/.test(key)
        ) {
          throw invalid('key must be 1 to 128 letters, digits or . _ : @ -.');
        }
        if (cmd === 'storageRemove') {
          store.remove(key);
          return {};
        }
        const value = a.value;
        if (typeof value !== 'string' || value.length > STORAGE_LIMITS.valueChars) {
          throw invalid('value must be a string of at most 32768 characters.');
        }
        const all = store.all();
        const keys = Object.keys(all);
        let total = value.length;
        for (const k of keys) if (k !== key) total += (all[k] ?? '').length;
        if (
          total > STORAGE_LIMITS.totalChars ||
          (!(key in all) && keys.length >= STORAGE_LIMITS.keys)
        ) {
          throw new RpcError(RPC_ERRORS.failed, 'The storage quota is used up.');
        }
        store.set(key, value);
        return {};
      }
      case 'shelfObjectUrl': {
        // D-389: one object of a shelf the member can open, after the member said yes (D-399: no server data the
        // member has not granted). The server checks the grant, the kind rule and every delisting when it mints.
        if (!hasByosRead() || !env.shelfObjectUrl) {
          throw new RpcError(
            RPC_ERRORS.notAuthorized,
            'This Activity did not declare the byos_read capability.',
          );
        }
        if (!isShelfObjectRef(args)) {
          throw invalid('shelf_id must be an id and object_hash a lowercase hex SHA-256.');
        }
        const key = `${args.shelf_id}/${args.object_hash}`;
        if (!shelfAllowed.has(key)) {
          const yes = await env.confirm({
            kind: 'shelf',
            shelfId: args.shelf_id,
            objectHash: args.object_hash,
          });
          if (!yes) throw new RpcError(RPC_ERRORS.denied, 'The person declined.');
          shelfAllowed.add(key);
        }
        try {
          const minted = await env.shelfObjectUrl(args.shelf_id, args.object_hash);
          return { url: minted.url, expires_at: minted.expires_at };
        } catch {
          throw new RpcError(RPC_ERRORS.failed, 'That file is not available.');
        }
      }
      case 'shelfObjectBytes': {
        // D-460: the same object, the same yes and the same per-object URL as shelfObjectUrl, but this page fetches
        // it and hands the frame the bytes, so the game's CSP never names a storage host.
        if (!hasByosRead() || !env.shelfObjectUrl) {
          throw new RpcError(
            RPC_ERRORS.notAuthorized,
            'This Activity did not declare the byos_read capability.',
          );
        }
        if (!isShelfObjectRef(args)) {
          throw invalid('shelf_id must be an id and object_hash a lowercase hex SHA-256.');
        }
        const key = `${args.shelf_id}/${args.object_hash}`;
        if (!shelfAllowed.has(key)) {
          const yes = await env.confirm({
            kind: 'shelf',
            shelfId: args.shelf_id,
            objectHash: args.object_hash,
          });
          if (!yes) throw new RpcError(RPC_ERRORS.denied, 'The person declined.');
          shelfAllowed.add(key);
        }
        let bytes: Uint8Array;
        try {
          const minted = await env.shelfObjectUrl(args.shelf_id, args.object_hash);
          bytes = await (env.fetchShelfObject ?? fetchCapped)(minted.url, SHELF_OBJECT_BYTES_MAX);
        } catch (e) {
          if (e instanceof ShelfObjectTooBigError) throw tooBig();
          throw new RpcError(RPC_ERRORS.failed, 'That file is not available.');
        }
        if (bytes.length > SHELF_OBJECT_BYTES_MAX) throw tooBig();
        return { b: b64(bytes), size: bytes.length };
      }
      case 'pickShelfObject': {
        // D-466: the member chooses one file in the app's own dialog; that choice is their yes for that object, so a
        // following shelfObjectBytes/shelfObjectUrl on it shows no second dialog. The game learns the pick only.
        if (!hasByosRead() || !env.pickShelfObject) {
          throw new RpcError(
            RPC_ERRORS.notAuthorized,
            'This Activity did not declare the byos_read capability.',
          );
        }
        if (!isPickShelfObjectArgs(args)) {
          throw invalid(
            'accept must list up to 16 extensions or media types and max_size be a whole number.',
          );
        }
        if (picking) throw new RpcError(RPC_ERRORS.failed, 'A file is already being chosen.');
        const req = {
          accept: [...(args?.accept ?? [])],
          maxSize: args?.max_size ?? null,
        };
        const ctl = new AbortController();
        picking = ctl;
        let pick: ShelfObjectPick | null;
        try {
          pick = await env.pickShelfObject(req, ctl.signal);
        } catch {
          throw new RpcError(RPC_ERRORS.failed, 'The file picker is not available.');
        } finally {
          if (picking === ctl) picking = null;
        }
        if (pick === null || ctl.signal.aborted) return null;
        // The picker filters too; this is the second check, so a swapped picker cannot widen what a game asked for.
        if (!isShelfObjectPick(pick) || !pickable(pick.name, pick.size, req)) {
          throw new RpcError(RPC_ERRORS.failed, 'That file is not available.');
        }
        shelfAllowed.add(`${pick.shelf_id}/${pick.object_hash}`);
        return {
          shelf_id: pick.shelf_id,
          object_hash: pick.object_hash,
          name: pick.name.slice(0, 255),
          size: pick.size,
          content_type: pick.content_type.slice(0, 127),
        } satisfies ShelfObjectPick;
      }
      case 'saveFile': {
        if (!env.saveFile) {
          throw new RpcError(RPC_ERRORS.notAuthorized, 'Saving files is not available.');
        }
        const a = obj(args);
        const name = a.name;
        const mime = a.mime;
        if (typeof name !== 'string' || !/^\w[\w. -]{0,99}$/.test(name) || name.includes('..')) {
          throw invalid('name must be a plain file name of at most 100 characters.');
        }
        if (typeof mime !== 'string' || !(SAVE_FILE_TYPES as readonly string[]).includes(mime)) {
          throw invalid(`mime must be one of: ${SAVE_FILE_TYPES.join(', ')}.`);
        }
        const b = a.b;
        if (typeof b !== 'string' || b.length > Math.ceil((SAVE_FILE_MAX_BYTES * 4) / 3) + 8) {
          throw invalid('b must be base64 of at most 8 MiB.');
        }
        let bytes: Uint8Array;
        try {
          bytes = unb64(b);
        } catch {
          throw invalid('b must be base64.');
        }
        if (bytes.length > SAVE_FILE_MAX_BYTES) throw invalid('b must be base64 of at most 8 MiB.');
        if (!(await env.confirm({ kind: 'save', name, mime, size: bytes.length }))) {
          return { saved: false };
        }
        env.saveFile(name, mime, bytes);
        return { saved: true };
      }
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

  const hostChanged = () => {
    if (!ready || toldHost === null) return;
    const now = hostOf(env.instance());
    if (now === toldHost) return;
    const data: HostChanged = { host_id: now, previous: toldHost };
    toldHost = now;
    emit('HOST_CHANGED', data);
  };

  const userOf = (id: string): Participant =>
    env.users([id])[0] ?? { id, username: null, global_name: null, avatar: null };

  return {
    async handle(data, origin) {
      if (origin !== (env.senderOrigin ?? env.origin)) return;
      const f = parseFrame(data);
      if (!f) return;
      if (f.kind === 'handshake') {
        if (f.application_id !== env.applicationId) return;
        // D-399: the instance's own credential for writes through the app's URL mappings (the frame's origin
        // is opaque). Without one the game still runs; only its mapped writes are refused.
        const proxyToken = env.proxyToken
          ? await env.proxyToken(env.instance().id).catch(() => null)
          : null;
        ready = true;
        joinData();
        const inst = env.instance();
        env.post({
          sr: PROTOCOL_VERSION,
          kind: 'ready',
          config: {
            application_id: env.applicationId,
            instance_id: inst.id,
            channel_id: inst.channel_id,
            guild_id: inst.guild_id,
            ...(env.selfId?.() ? { user_id: env.selfId() ?? undefined } : {}),
            host_id: (toldHost = hostOf(inst)),
            capabilities: inst.capabilities ?? [],
            ...(hasStorage() && env.storage ? { storage: env.storage.all() } : {}),
            ...(proxyToken ? { proxy_token: proxyToken } : {}),
          },
        });
        return;
      }
      if (f.kind !== 'request') return;
      const problem = requestProblem(f);
      if (problem) {
        fail(f.nonce, { code: RPC_ERRORS.invalidArguments, message: problem });
        return;
      }
      const at = now();
      while (requests.length && at - (requests[0] ?? 0) > REQUEST_WINDOW_MS) requests.shift();
      if (requests.length >= REQUEST_LIMIT) {
        fail(f.nonce, {
          code: RPC_ERRORS.rateLimited,
          message: 'Too many requests. Try again soon.',
        });
        return;
      }
      requests.push(at);
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
    hostChanged,
    participantsChanged() {
      hostChanged();
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
      picking?.abort();
      picking = null;
      if (dataJoined) {
        dataJoined.off();
        env.data?.leave(dataJoined.instance);
        dataJoined = null;
      }
      if (presenceSet) env.setPresence(null);
      presenceSet = false;
      subscribed.clear();
      shelfAllowed.clear();
      ready = false;
      toldHost = null;
    },
  };
}

export { hostOf } from './types';
