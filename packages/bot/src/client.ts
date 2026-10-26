import { Cache } from './cache';
import {
  CommandRouter,
  wrapInteraction,
  type AnyInteraction,
  type SlashCommandBuilder,
} from './commands';
import { GatewayOp, Shard, urlHost, type SocketFactory } from './gateway';
import { Rest, type FetchLike } from './rest';
import {
  DEFAULT_INTENTS,
  EPHEMERAL,
  type MembersChunk,
  type Message,
  type PresenceUpdate,
  type RawInteraction,
  type Ready,
  type RequestGuildMembers,
} from './types';

export interface ClientEvents {
  ready: [Ready];
  messageCreate: [Message];
  interactionCreate: [AnyInteraction];
  guildMembersChunk: [MembersChunk];
  /** Every dispatch, undecoded. */
  raw: [name: string, data: unknown, shard: number];
  /** A command handler or the command registration failed. */
  error: [Error];
  /** A shard stopped for good (bad token, bad intents...). */
  fatal: [code: number, reason: string];
}

type Listener<A extends unknown[]> = (...args: A) => void | Promise<void>;

export interface ClientOptions {
  token: string;
  /** API root, for example `https://chat.example/api/v1`. */
  apiUrl: string;
  intents?: number;
  /** `'auto'` uses the count `GET /gateway/bot` recommends. */
  shards?: 'auto' | number | { id: number; count: number };
  /** Skip `GET /gateway/bot` for the URL. */
  gatewayUrl?: string;
  /**
   * Also follow gateway and resume URLs on these hosts (D-234). By default only the host of
   * `apiUrl` (and of an explicit `gatewayUrl`) is followed; any other host falls back to
   * `wss://<api host>/gateway`.
   */
  allowGatewayHosts?: string[];
  /** Register the router's commands globally at the first READY (default true). */
  registerCommands?: boolean;
  fetch?: FetchLike;
  createSocket?: SocketFactory;
  /** Gap between shard IDENTIFYs; the server allows one at a time. */
  identifyGapMs?: number;
}

/** `wss://host/gateway` for an API root such as `https://host/api/v1`. */
export function defaultGatewayUrl(apiUrl: string): string {
  const u = new URL(apiUrl);
  return `${u.protocol === 'http:' ? 'ws' : 'wss'}://${u.host}/gateway`;
}

export function requestGuildMembersPayload(req: RequestGuildMembers, nonce: string) {
  const d: Record<string, unknown> = { guild_id: req.guildId };
  if (req.userIds?.length) d.user_ids = req.userIds;
  else {
    d.query = req.query ?? '';
    d.limit = req.limit ?? 0;
  }
  if (req.presences) d.presences = true;
  d.nonce = nonce;
  return d;
}

export class Client {
  readonly rest: Rest;
  readonly cache = new Cache();
  readonly commands = new CommandRouter();
  private readonly listeners: { [K in keyof ClientEvents]?: Listener<ClientEvents[K]>[] } = {};
  private shards: Shard[] = [];
  private timers: ReturnType<typeof setTimeout>[] = [];
  private registered = false;
  private shardCount = 1;
  private readonly pending = new Map<string, (c: MembersChunk) => void>();
  private nonce = 0;

  constructor(private readonly opts: ClientOptions) {
    this.rest = new Rest({ apiUrl: opts.apiUrl, token: opts.token, fetch: opts.fetch });
  }

  on<K extends keyof ClientEvents>(event: K, fn: Listener<ClientEvents[K]>): this {
    (this.listeners[event] ??= [] as never).push(fn as never);
    return this;
  }

  private emit<K extends keyof ClientEvents>(event: K, ...args: ClientEvents[K]): void {
    for (const fn of this.listeners[event] ?? []) {
      try {
        const r = (fn as Listener<ClientEvents[K]>)(...args);
        if (r instanceof Promise) r.catch((e: unknown) => this.fail(e));
      } catch (e) {
        this.fail(e);
      }
    }
  }

  private fail(e: unknown): void {
    const err = e instanceof Error ? e : new Error(String(e));
    if (this.listeners.error?.length) this.emit('error', err);
  }

  /** Register a slash command and its handler. */
  command(
    def: SlashCommandBuilder,
    handler: Parameters<CommandRouter['add']>[1],
    autocomplete?: Parameters<CommandRouter['add']>[2],
  ): this {
    this.commands.add(def, handler, autocomplete);
    return this;
  }

  /** Connect every shard. Resolves once they are started; events arrive through `on`. */
  async login(): Promise<void> {
    let url = this.opts.gatewayUrl;
    // D-234: the API host and the operator's allow-list are the only hosts followed.
    const allowed = [urlHost(this.opts.apiUrl), ...(this.opts.allowGatewayHosts ?? [])]
      .filter((h): h is string => h !== undefined)
      .map((h) => h.toLowerCase());
    let recommended = 1;
    if (!url) {
      const gw = await this.rest.gatewayBot();
      recommended = gw.shards;
      url = this.hostAllowed(gw.url, allowed) ? gw.url : defaultGatewayUrl(this.opts.apiUrl);
    }
    const gwHost = urlHost(url);
    if (gwHost !== undefined) allowed.push(gwHost);
    const s = this.opts.shards ?? 'auto';
    const plan: [number, number][] =
      s === 'auto'
        ? Array.from({ length: recommended }, (_, i) => [i, recommended])
        : typeof s === 'number'
          ? Array.from({ length: Math.max(1, s) }, (_, i) => [i, Math.max(1, s)])
          : [[s.id, s.count]];
    this.shardCount = Math.max(1, ...plan.map((p) => p[1]));
    const gap = this.opts.identifyGapMs ?? 5000;
    plan.forEach((shard, n) => {
      const sh = new Shard({
        token: this.opts.token.replace(/^Bot /, ''),
        intents: this.opts.intents ?? DEFAULT_INTENTS,
        shard,
        url: url as string,
        createSocket: this.opts.createSocket,
        allowedHosts: allowed,
        onDispatch: (id, name, data) => this.dispatch(id, name, data),
        onFatal: (code, reason) => {
          this.destroy();
          this.emit('fatal', code, reason);
        },
      });
      this.shards.push(sh);
      if (n === 0 || gap === 0) sh.connect();
      else this.timers.push(setTimeout(() => sh.connect(), gap * n));
    });
  }

  private hostAllowed(url: string, allowed: string[]): boolean {
    const h = urlHost(url);
    return h !== undefined && allowed.includes(h);
  }

  private shardFor(guildId: string): Shard | undefined {
    let id: bigint;
    try {
      id = BigInt(guildId);
    } catch {
      return undefined;
    }
    const n = Number((id >> 22n) % BigInt(this.shardCount));
    return this.shards.find((s) => s.id === n);
  }

  /**
   * REQUEST_GUILD_MEMBERS (op 8). Yields each GUILD_MEMBERS_CHUNK of this request and ends
   * after the last one. Needs the GuildMembers intent.
   */
  async *requestGuildMembers(req: RequestGuildMembers): AsyncGenerator<MembersChunk> {
    const shard = this.shardFor(req.guildId);
    if (!shard) throw new Error('the shard for this guild is not running in this process');
    const nonce = req.nonce ?? `sr-bot-${++this.nonce}`;
    const queue: MembersChunk[] = [];
    let wake: (() => void) | undefined;
    let done = false;
    this.pending.set(nonce, (c) => {
      queue.push(c);
      if (c.chunk_index + 1 >= c.chunk_count) done = true;
      wake?.();
    });
    const sent = shard.send(
      GatewayOp.REQUEST_GUILD_MEMBERS,
      requestGuildMembersPayload(req, nonce),
    );
    if (!sent) {
      this.pending.delete(nonce);
      throw new Error('the shard is not connected');
    }
    try {
      while (true) {
        const next = queue.shift();
        if (next) {
          yield next;
          continue;
        }
        if (done) return;
        await new Promise<void>((r) => (wake = r));
        wake = undefined;
      }
    } finally {
      this.pending.delete(nonce);
    }
  }

  /** Presence update (op 3) on every shard running here. False when none is connected. */
  setPresence(p: PresenceUpdate): boolean {
    const d = {
      status: p.status,
      activities: p.activities ?? [],
      since: null,
      afk: p.afk ?? false,
    };
    let any = false;
    for (const s of this.shards) any = s.send(GatewayOp.PRESENCE_UPDATE, d) || any;
    return any;
  }

  /**
   * Gateway op 4 (voice state update) on the shard that owns `guildId`: join, or with a null `channel_id` in
   * `body` leave, a voice channel. See `voiceStateBody` and `E2eeCall.joinState` of `@sideroom/bot/voice`. The
   * answer arrives as a `VOICE_SERVER_UPDATE` in the `raw` event. False when the shard is not connected.
   */
  voiceStateUpdate(guildId: string, body: Record<string, unknown>): boolean {
    return this.shardFor(guildId)?.send(GatewayOp.VOICE_STATE_UPDATE, body) ?? false;
  }

  /**
   * Gateway op 101 (`E2EE_MLS`) on the shard that owns `guildId`: the `d` objects an `E2eeCall` produces. The
   * answers arrive as `E2EE_MLS` in the `raw` event. False when the shard is not connected.
   */
  e2eeMls(guildId: string, body: Record<string, unknown>): boolean {
    return this.shardFor(guildId)?.send(GatewayOp.E2EE_MLS, body) ?? false;
  }

  destroy(): void {
    for (const t of this.timers) clearTimeout(t);
    this.timers = [];
    for (const sh of this.shards) sh.destroy();
    this.shards = [];
  }

  private dispatch(shard: number, name: string, data: unknown): void {
    this.cache.apply(name, data);
    this.emit('raw', name, data, shard);
    switch (name) {
      case 'READY':
        this.emit('ready', data as Ready);
        void this.register();
        break;
      case 'MESSAGE_CREATE':
        this.emit('messageCreate', data as Message);
        break;
      case 'GUILD_MEMBERS_CHUNK': {
        const c = data as MembersChunk;
        this.emit('guildMembersChunk', c);
        if (c.nonce) this.pending.get(c.nonce)?.(c);
        break;
      }
      case 'INTERACTION_CREATE':
        void this.interaction(data as RawInteraction);
        break;
    }
  }

  private async register(): Promise<void> {
    if (this.registered || this.opts.registerCommands === false || this.commands.size === 0) return;
    const app = this.cache.applicationId;
    if (!app) return;
    this.registered = true;
    try {
      await this.rest.setGlobalCommands(app, this.commands.definitions());
    } catch (e) {
      this.fail(e);
    }
  }

  private async interaction(raw: RawInteraction): Promise<void> {
    const i = wrapInteraction(raw, this.rest);
    this.emit('interactionCreate', i);
    try {
      await this.commands.dispatch(i);
    } catch (e) {
      // Tell the invoking user; a reply that fails (already answered, window passed) is not
      // worth more noise.
      await this.rest
        .interactionCallback(raw, 4, {
          content: 'Something went wrong with that command.',
          flags: EPHEMERAL,
        })
        .catch(() => undefined);
      this.fail(e);
    }
  }
}
