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
} from './protocol';

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
  private readonly onMessage = (e: MessageEvent) => this.receive(e);

  constructor(applicationId: string, opts: SdkOptions = {}) {
    this.applicationId = applicationId;
    this.env = opts.env ?? browserEnv();
    this.timeoutMs = opts.timeoutMs ?? 60_000;
    this.env.addEventListener('message', this.onMessage);
  }

  /** Stop listening. Pending commands fail. */
  close(): void {
    this.env.removeEventListener('message', this.onMessage);
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
