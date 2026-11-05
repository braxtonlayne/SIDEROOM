// The Activity side of the Embedded App SDK (DEV-ACT-003). An Activity creates one `SideRoomSDK` with its
// application id, awaits `ready()`, then calls `commands.*`. The API mirrors Discord's Embedded App SDK
// where the feature exists here; unknown commands fail with `UNKNOWN_COMMAND`.

import {
  PROTOCOL_VERSION,
  parseFrame,
  type AuthenticateResult,
  type ActivityInput,
  type ChannelInfo,
  type EventName,
  type Frame,
  type Participant,
  type ReadyConfig,
  type RpcErrorBody,
  type SubscribeArgs,
  type ShelfObjectRef,
  type ShelfObjectBytes,
  type ShelfObjectUrl,
  STORAGE_LIMITS,
  STORAGE_CAPABILITY,
  PROXY_TOKEN_HEADER,
} from './protocol';
import {
  ActivityData,
  b64,
  unb64,
  type DataEventMap,
  type DataEventName,
  type DataTransport,
} from './data';

/** The slice of `window` the SDK needs, injectable for tests. */
export interface SdkEnv {
  /** The window that hosts this Activity. */
  parent: { postMessage(message: unknown, targetOrigin: string): void } | null;
  /** Where the host lives; the SDK posts only to this origin. */
  hostOrigin: string;
  addEventListener(type: 'message', listener: (e: MessageEvent) => void): void;
  removeEventListener(type: 'message', listener: (e: MessageEvent) => void): void;
  /** The host window object a valid message must come from. */
  hostWindow: unknown;
  /** `fetch` for `mappedFetch`; defaults to the window's. */
  fetch?: (input: string, init?: RequestInit) => Promise<Response>;
}

export class SdkError extends Error {
  readonly code: string;
  constructor(body: RpcErrorBody) {
    super(body.message);
    this.name = 'SdkError';
    this.code = body.code;
  }
}

type Pending = { resolve: (v: unknown) => void; reject: (e: SdkError) => void };
type Listener = (data: unknown) => void;

export interface SdkOptions {
  /** Milliseconds a command may take before it fails. Default 60 000 (an authorize waits for a person). */
  timeoutMs?: number;
  /** Test seam; defaults to the browser's `window`. */
  env?: SdkEnv;
  /**
   * The activity data channel (ACT-CH-001). Tests pass a `LoopbackHub` transport; in the client the host
   * page provides the SFU-backed one. Without it `sdk.state` and `sdk.input` throw `NO_TRANSPORT`.
   */
  transport?: DataTransport;
}

function browserEnv(): SdkEnv {
  const w = window;
  // The host's origin is the origin of the page that framed us. `ancestorOrigins` is the reliable source;
  // `document.referrer` is a fallback (no-referrer policies drop it, which is why the host posts `ready`
  // with the origin check below rather than trusting it).
  const ancestor = w.location.ancestorOrigins?.[0];
  let origin = ancestor ?? '';
  if (!origin && document.referrer) {
    try {
      origin = new URL(document.referrer).origin;
    } catch {
      origin = '';
    }
  }
  return {
    parent: w.parent === w ? null : w.parent,
    hostOrigin: origin,
    addEventListener: (t, l) => w.addEventListener(t, l),
    removeEventListener: (t, l) => w.removeEventListener(t, l),
    hostWindow: w.parent,
  };
}

export class SideRoomSDK {
  readonly applicationId: string;
  /** Set once `ready()` resolved. */
  config: ReadyConfig | null = null;
  private readonly env: SdkEnv;
  private readonly timeoutMs: number;
  private readonly pending = new Map<string, Pending>();
  private readonly listeners = new Map<string, Set<Listener>>();
  private seq = 0;
  private readyWaiter: { resolve: (c: ReadyConfig) => void } | null = null;
  private readonly kv = new Map<string, string>();
  private readonly onMessage = (e: MessageEvent) => this.receive(e);
  /** The data layer behind `state`, `input` and `on` (public for activities that need `version`, `snapshot`). */
  readonly data = new ActivityData();

  constructor(applicationId: string, opts: SdkOptions = {}) {
    this.applicationId = applicationId;
    this.env = opts.env ?? browserEnv();
    this.timeoutMs = opts.timeoutMs ?? 60_000;
    this.env.addEventListener('message', this.onMessage);
    if (opts.transport) this.data.attach(opts.transport);
  }

  /** Attach (or replace) the data channel transport. */
  attachTransport(transport: DataTransport): void {
    this.data.attach(transport);
  }

  /** The instance's shared JSON document: last writer wins per key; `host:*` keys are the host's alone. */
  get state() {
    const d = this.data;
    return {
      get: <T = unknown>(key: string): T | undefined => d.get<T>(key),
      /** A copy of every live key. */
      all: (): Record<string, unknown> => d.snapshot(),
      /** The document version (the highest key version seen). */
      get version(): number {
        return d.version;
      },
      set: (patch: Record<string, unknown>): void => d.set(patch),
      delete: (...keys: string[]): void => d.delete(...keys),
    };
  }

  /** Inputs to the host and the host's replies to one member. */
  get input() {
    const d = this.data;
    return {
      /** Send bytes to the instance's host. */
      send: (bytes: Uint8Array): void => d.sendInput(bytes),
      /** Host only: send bytes to one member (e.g. hole cards); other members ignore them. */
      reply: (member: string, bytes: Uint8Array): void => d.reply(member, bytes),
    };
  }

  /** Listen to `'state'` (the document changed) or `'input'` (an input for the host, or a host reply). */
  on<K extends DataEventName>(event: K, cb: (e: DataEventMap[K]) => void): () => void {
    return this.data.on(event, cb);
  }

  /** This participant's own member id (from the data channel), `null` before a transport is attached. */
  get self(): string | null {
    return this.data.self;
  }

  /**
   * The instance's host: the only writer of `host:*` keys. The launcher, until the platform moves the host
   * (D-388, `HOST_CHANGED`, `host_handoff` games); listen with `sdk.on('host', ...)`.
   */
  get hostId(): string | null {
    return this.data.host;
  }

  /** True when this participant is the host now. */
  get isHost(): boolean {
    return this.data.isHost;
  }

  /**
   * A small per-member key-value store that survives reloads (ARC-SBX-001). The sandbox has no `localStorage`
   * (its origin is opaque), so the host keeps the values and sends them in `ready`; reads are local and
   * writes go to the host. Needs the `storage` capability in `sideroom.json`; without it writes throw.
   * `install()` makes `window.localStorage` this store, for code that already uses `localStorage`.
   */
  get storage() {
    const kv = this.kv;
    const can = () => !!this.config?.capabilities?.includes(STORAGE_CAPABILITY);
    const api = {
      get length(): number {
        return kv.size;
      },
      key: (i: number): string | null => [...kv.keys()][i] ?? null,
      getItem: (key: string): string | null => kv.get(String(key)) ?? null,
      setItem: (key: string, value: string): void => {
        const k = String(key);
        const v = String(value);
        if (!can())
          throw new SdkError({
            code: 'NOT_AUTHORIZED',
            message: 'Declare the storage capability.',
          });
        let total = v.length;
        for (const [ek, ev] of kv) if (ek !== k) total += ev.length;
        if (
          k.length === 0 ||
          k.length > STORAGE_LIMITS.keyChars ||
          v.length > STORAGE_LIMITS.valueChars ||
          total > STORAGE_LIMITS.totalChars ||
          (!kv.has(k) && kv.size >= STORAGE_LIMITS.keys)
        ) {
          throw new SdkError({ code: 'QUOTA_EXCEEDED', message: 'The storage quota is used up.' });
        }
        kv.set(k, v);
        void this.call('storageSet', { key: k, value: v }).catch(() => undefined);
      },
      removeItem: (key: string): void => {
        const k = String(key);
        if (!kv.delete(k)) return;
        void this.call('storageRemove', { key: k }).catch(() => undefined);
      },
      clear: (): void => {
        for (const k of [...kv.keys()]) api.removeItem(k);
      },
      /** Replace `target.localStorage` (default: this window) with this store. */
      install: (target: object = globalThis): void => {
        Object.defineProperty(target, 'localStorage', { configurable: true, value: api });
      },
    };
    return api;
  }

  /**
   * `fetch` through the application's own URL mappings (D-399). `path` is a path on the activity's host under a
   * mapping prefix of the application's settings (e.g. `/api/scores`); the proxy relays it to the mapped
   * server. The sandboxed frame's origin is opaque, so writes carry the instance's proxy token from `ready`,
   * without which the proxy refuses them. Call after `ready()`.
   */
  mappedFetch(path: string, init: RequestInit = {}): Promise<Response> {
    if (typeof path !== 'string' || !path.startsWith('/') || path.startsWith('//')) {
      return Promise.reject(
        new SdkError({
          code: 'INVALID_ARGUMENTS',
          message: 'mappedFetch takes a path such as /api/scores.',
        }),
      );
    }
    const headers = new Headers(init.headers);
    const token = this.config?.proxy_token;
    if (token) headers.set(PROXY_TOKEN_HEADER, token);
    const f = this.env.fetch ?? ((i: string, o?: RequestInit) => fetch(i, o));
    return f(path, { ...init, headers, mode: 'cors', credentials: 'omit' });
  }

  /** Stop listening. Pending commands fail. */
  close(): void {
    this.env.removeEventListener('message', this.onMessage);
    this.data.detach();
    for (const [, p] of this.pending)
      p.reject(new SdkError({ code: 'CLOSED', message: 'The SDK was closed.' }));
    this.pending.clear();
  }

  /** Handshake with the host. Resolves with the instance the Activity runs in. */
  ready(): Promise<ReadyConfig> {
    if (this.config) return Promise.resolve(this.config);
    if (!this.env.parent) {
      return Promise.reject(
        new SdkError({ code: 'NO_HOST', message: 'This page is not running inside SIDE ROOM.' }),
      );
    }
    return new Promise<ReadyConfig>((resolve, reject) => {
      this.readyWaiter = { resolve };
      this.post({ sr: PROTOCOL_VERSION, kind: 'handshake', application_id: this.applicationId });
      setTimeout(() => {
        if (this.readyWaiter) {
          this.readyWaiter = null;
          reject(
            new SdkError({ code: 'TIMEOUT', message: 'The client did not answer the handshake.' }),
          );
        }
      }, this.timeoutMs);
    });
  }

  readonly commands = {
    /** Ask the person to let the application identify them. Resolves with an OAuth2 `code` for the app's server. */
    authorize: (args: { scope: string[] }) => this.call<{ code: string }>('authorize', args),
    /** Check an access token the application's server obtained from the code. */
    authenticate: (args: { access_token: string }) =>
      this.call<AuthenticateResult>('authenticate', args),
    getChannel: () => this.call<ChannelInfo>('getChannel'),
    getInstanceConnectedParticipants: () =>
      this.call<{ participants: Participant[] }>('getInstanceConnectedParticipants'),
    /** Open a link in a new tab after the person confirms. Resolves `{ opened }`. */
    openExternalLink: (args: { url: string }) =>
      this.call<{ opened: boolean }>('openExternalLink', args),
    /**
     * Set what the person is shown doing (their Rich Presence) while the Activity runs. Resolves with the
     * activity as the host stored it; `null` clears it. The host clears it when the Activity closes.
     */
    setActivity: (args: { activity: ActivityInput | null }) =>
      this.call<ActivityInput | null>('setActivity', args),
    /**
     * D-389: a short-lived URL for one object of a shelf the member can open (needs `byos_read`; the member is asked
     * first). Fetching it needs the shelf's storage host in the manifest's `connect` list. Members only.
     */
    shelfObjectUrl: (args: ShelfObjectRef) => this.call<ShelfObjectUrl>('shelfObjectUrl', args),
    /**
     * D-460: the bytes of one shelf object (needs `byos_read`; the member is asked first). The member's page fetches
     * the per-object URL and hands the bytes over, so no storage host has to be in the game's CSP. At most
     * SHELF_OBJECT_BYTES_MAX. Members only.
     */
    shelfObjectBytes: async (args: ShelfObjectRef): Promise<Uint8Array> =>
      unb64((await this.call<ShelfObjectBytes>('shelfObjectBytes', args)).b),
    /** Offer the person a file to save (png, jpeg, webp, json or text, at most 8 MiB). Resolves `{ saved }`. */
    saveFile: (args: { name: string; mime: string; bytes: Uint8Array }) =>
      this.call<{ saved: boolean }>('saveFile', {
        name: args.name,
        mime: args.mime,
        b: b64(args.bytes),
      }),
  };

  /**
   * Listen to an event. Returns the unsubscribe function. `args.channel_id`, when given, must be the channel
   * of the instance (voice events are only ever about that channel).
   */
  subscribe(event: EventName, listener: Listener, args?: SubscribeArgs): () => void {
    let set = this.listeners.get(event);
    if (!set) {
      set = new Set();
      this.listeners.set(event, set);
      void this.call('subscribe', { event, args }).catch(() => undefined);
    }
    set.add(listener);
    return () => {
      const s = this.listeners.get(event);
      if (!s) return;
      s.delete(listener);
      if (s.size === 0) {
        this.listeners.delete(event);
        void this.call('unsubscribe', { event }).catch(() => undefined);
      }
    };
  }

  /** With the `datachannel` capability the host page relays our data envelopes through the SFU. */
  private attachHostTransport(cfg: ReadyConfig): void {
    if (this.data.self || !cfg.capabilities?.includes('datachannel')) return;
    if (!cfg.user_id || !cfg.host_id) return;
    const subs = new Set<(from: string, bytes: Uint8Array) => void>();
    this.subscribe('DATA_MESSAGE', (d) => {
      const m = d as { from?: unknown; b?: unknown };
      if (typeof m.from !== 'string' || typeof m.b !== 'string') return;
      let bytes: Uint8Array;
      try {
        bytes = unb64(m.b);
      } catch {
        return;
      }
      for (const s of subs) s(m.from, bytes);
    });
    // D-388: the platform moved the host (a `host_handoff` game whose host left). The data layer follows it.
    this.subscribe('HOST_CHANGED', (d) => {
      const m = d as { host_id?: unknown };
      if (typeof m.host_id !== 'string') return;
      if (this.config) this.config = { ...this.config, host_id: m.host_id };
      this.data.changeHost(m.host_id);
    });
    this.data.attach({
      self: cfg.user_id,
      host: cfg.host_id,
      send: (bytes) => void this.call('dataSend', { b: b64(bytes) }).catch(() => undefined),
      onMessage: (cb) => {
        subs.add(cb);
        return () => subs.delete(cb);
      },
    });
  }

  private post(frame: Frame): void {
    this.env.parent?.postMessage(frame, this.env.hostOrigin || '*');
  }

  private call<T>(cmd: string, args?: unknown): Promise<T> {
    if (!this.env.parent) {
      return Promise.reject(
        new SdkError({ code: 'NO_HOST', message: 'Not running inside SIDE ROOM.' }),
      );
    }
    const nonce = `${Date.now().toString(36)}-${(this.seq++).toString(36)}`;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(nonce, { resolve: resolve as (v: unknown) => void, reject });
      this.post({ sr: PROTOCOL_VERSION, kind: 'request', nonce, cmd, args });
      setTimeout(() => {
        const p = this.pending.get(nonce);
        if (!p) return;
        this.pending.delete(nonce);
        p.reject(new SdkError({ code: 'TIMEOUT', message: `${cmd} timed out.` }));
      }, this.timeoutMs);
    });
  }

  private receive(e: MessageEvent): void {
    // Only the window that framed us may speak, and only from the origin it announced.
    if (e.source !== this.env.hostWindow) return;
    if (this.env.hostOrigin && e.origin !== this.env.hostOrigin) return;
    const f = parseFrame(e.data);
    if (!f) return;
    if (f.kind === 'ready' && this.readyWaiter) {
      this.config = f.config;
      this.kv.clear();
      for (const [k, v] of Object.entries(f.config.storage ?? {})) {
        if (typeof v === 'string') this.kv.set(k, v);
      }
      this.attachHostTransport(f.config);
      this.readyWaiter.resolve(f.config);
      this.readyWaiter = null;
    } else if (f.kind === 'response') {
      const p = this.pending.get(f.nonce);
      if (!p) return;
      this.pending.delete(f.nonce);
      if (f.ok) p.resolve(f.data);
      else p.reject(new SdkError(f.error));
    } else if (f.kind === 'event') {
      this.listeners.get(f.event)?.forEach((l) => l(f.data));
    }
  }
}
