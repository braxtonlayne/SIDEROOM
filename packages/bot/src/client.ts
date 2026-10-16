import { Cache } from './cache';
import {
  CommandRouter,
  wrapInteraction,
  type AnyInteraction,
  type SlashCommandBuilder,
} from './commands';
import { Shard, type SocketFactory } from './gateway';
import { Rest, type FetchLike } from './rest';
import { DEFAULT_INTENTS, EPHEMERAL, type Message, type RawInteraction, type Ready } from './types';

export interface ClientEvents {
  ready: [Ready];
  messageCreate: [Message];
  interactionCreate: [AnyInteraction];
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
  /** Register the router's commands globally at the first READY (default true). */
  registerCommands?: boolean;
  fetch?: FetchLike;
  createSocket?: SocketFactory;
  /** Gap between shard IDENTIFYs; the server allows one at a time. */
  identifyGapMs?: number;
}

export class Client {
  readonly rest: Rest;
  readonly cache = new Cache();
  readonly commands = new CommandRouter();
  private readonly listeners: { [K in keyof ClientEvents]?: Listener<ClientEvents[K]>[] } = {};
  private shards: Shard[] = [];
  private timers: ReturnType<typeof setTimeout>[] = [];
  private registered = false;

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
    let recommended = 1;
    if (!url) {
      const gw = await this.rest.gatewayBot();
      url = gw.url;
      recommended = gw.shards;
    }
    const s = this.opts.shards ?? 'auto';
    const plan: [number, number][] =
      s === 'auto'
        ? Array.from({ length: recommended }, (_, i) => [i, recommended])
        : typeof s === 'number'
          ? Array.from({ length: Math.max(1, s) }, (_, i) => [i, Math.max(1, s)])
          : [[s.id, s.count]];
    const gap = this.opts.identifyGapMs ?? 5000;
    plan.forEach((shard, n) => {
      const sh = new Shard({
        token: this.opts.token.replace(/^Bot /, ''),
        intents: this.opts.intents ?? DEFAULT_INTENTS,
        shard,
        url: url as string,
        createSocket: this.opts.createSocket,
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
