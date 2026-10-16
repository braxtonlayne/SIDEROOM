import {
  CallbackType,
  EPHEMERAL,
  type Message,
  type MessageOptions,
  type RawInteraction,
  type User,
} from './types';

/** The error body of the API: `{code, message, errors?}`. */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: number,
    message: string,
    readonly errors?: unknown,
  ) {
    super(`api error ${status} (code ${code}): ${message}`);
    this.name = 'ApiError';
  }
}

export class RateLimitError extends Error {
  constructor(readonly retryAfterMs: number) {
    super(`rate limited (retry after ${retryAfterMs} ms)`);
    this.name = 'RateLimitError';
  }
}

export type FetchLike = (
  url: string,
  init: { method: string; headers: Record<string, string>; body?: string },
) => Promise<{
  status: number;
  headers: { get(name: string): string | null };
  text(): Promise<string>;
}>;

export interface RestOptions {
  /** API root, for example `https://chat.example/api/v1`. */
  apiUrl: string;
  token: string;
  fetch?: FetchLike;
  /** Waits after a 429 (injectable for tests). */
  sleep?: (ms: number) => Promise<void>;
}

const MAX_RETRIES = 3;
const MAX_WAIT_MS = 60_000;

export function messageBody(o: MessageOptions | string): Record<string, unknown> {
  const m: MessageOptions = typeof o === 'string' ? { content: o } : o;
  const body: Record<string, unknown> = {};
  if (m.content !== undefined) body.content = m.content;
  if (m.embeds?.length) body.embeds = m.embeds;
  if (m.components?.length) body.components = m.components;
  if (m.ephemeral) body.flags = EPHEMERAL;
  return body;
}

export class Rest {
  private readonly base: string;
  private readonly token: string;
  private readonly doFetch: FetchLike;
  private readonly sleep: (ms: number) => Promise<void>;

  constructor(opts: RestOptions) {
    this.base = opts.apiUrl.replace(/\/+$/, '');
    this.token = opts.token.replace(/^Bot /, '');
    this.doFetch = opts.fetch ?? (globalThis.fetch as unknown as FetchLike);
    this.sleep = opts.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms)));
  }

  // The token is a credential and never printed.
  toJSON(): { base: string } {
    return { base: this.base };
  }

  async request<T = unknown>(
    method: string,
    path: string,
    body?: unknown,
    auth = true,
  ): Promise<T> {
    const headers: Record<string, string> = { 'User-Agent': 'sideroom-bot-sdk' };
    if (auth) headers.Authorization = `Bot ${this.token}`;
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    for (let attempt = 0; ; attempt++) {
      const res = await this.doFetch(`${this.base}${path}`, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      const text = await res.text();
      if (res.status === 429) {
        const wait = retryAfterMs(text, res.headers.get('retry-after'));
        if (attempt >= MAX_RETRIES || wait > MAX_WAIT_MS) throw new RateLimitError(wait);
        await this.sleep(wait);
        continue;
      }
      const json: unknown = text ? JSON.parse(text) : null;
      if (res.status >= 200 && res.status < 300) return json as T;
      const e = (json ?? {}) as { code?: number; message?: string; errors?: unknown };
      throw new ApiError(res.status, e.code ?? 0, e.message ?? 'request failed', e.errors);
    }
  }

  currentUser(): Promise<User> {
    return this.request('GET', '/users/@me');
  }

  async gatewayBot(): Promise<{ url: string; shards: number }> {
    const v = await this.request<{ url: string; shards?: number }>('GET', '/gateway/bot');
    return { url: v.url, shards: Math.max(1, v.shards ?? 1) };
  }

  createMessage(channelId: string, message: MessageOptions | string): Promise<Message> {
    return this.request('POST', `/channels/${channelId}/messages`, messageBody(message));
  }

  editMessage(
    channelId: string,
    messageId: string,
    message: MessageOptions | string,
  ): Promise<Message> {
    return this.request(
      'PATCH',
      `/channels/${channelId}/messages/${messageId}`,
      messageBody(message),
    );
  }

  async deleteMessage(channelId: string, messageId: string): Promise<void> {
    await this.request('DELETE', `/channels/${channelId}/messages/${messageId}`);
  }

  /** Bulk overwrite of the application's global commands. */
  setGlobalCommands(applicationId: string, commands: unknown[]): Promise<unknown[]> {
    return this.request('PUT', `/applications/${applicationId}/commands`, commands);
  }

  setGuildCommands(
    applicationId: string,
    guildId: string,
    commands: unknown[],
  ): Promise<unknown[]> {
    return this.request(
      'PUT',
      `/applications/${applicationId}/guilds/${guildId}/commands`,
      commands,
    );
  }

  async interactionCallback(
    i: Pick<RawInteraction, 'id' | 'token'>,
    type: number,
    data?: unknown,
  ): Promise<void> {
    const body: Record<string, unknown> = { type };
    if (data !== undefined) body.data = data;
    await this.request('POST', `/interactions/${i.id}/${i.token}/callback`, body, false);
  }

  interactionFollowUp(
    i: Pick<RawInteraction, 'application_id' | 'token'>,
    message: MessageOptions | string,
  ): Promise<Message> {
    return this.request(
      'POST',
      `/webhooks/${i.application_id}/${i.token}?wait=true`,
      messageBody(message),
      false,
    );
  }

  interactionEditOriginal(
    i: Pick<RawInteraction, 'application_id' | 'token'>,
    message: MessageOptions | string,
  ): Promise<unknown> {
    return this.request(
      'PATCH',
      `/webhooks/${i.application_id}/${i.token}/messages/@original`,
      messageBody(message),
      false,
    );
  }

  async interactionDeleteOriginal(
    i: Pick<RawInteraction, 'application_id' | 'token'>,
  ): Promise<void> {
    await this.request(
      'DELETE',
      `/webhooks/${i.application_id}/${i.token}/messages/@original`,
      undefined,
      false,
    );
  }
}

function retryAfterMs(body: string, header: string | null): number {
  let secs: number | undefined;
  try {
    const v = JSON.parse(body) as { retry_after?: number };
    if (typeof v.retry_after === 'number') secs = v.retry_after;
  } catch {
    /* not JSON */
  }
  if (secs === undefined && header !== null && Number.isFinite(Number(header))) {
    secs = Number(header);
  }
  return Math.round(Math.min(Math.max(secs ?? 1, 0), 3600) * 1000);
}

export { CallbackType };
