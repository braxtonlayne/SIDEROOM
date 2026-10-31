import type { APIRequestContext } from '@playwright/test';

/**
 * A thin client for the fake Discord (`e2e/support/fake-discord.mjs`: REST and, since the wave-35 integration, the
 * gateway WebSocket on the same port). These helpers read what it received and make it emit a message through its
 * control routes:
 *
 *   GET  {base}/__fake/state    -> { gateways, webhooks: Webhook[], posts: Post[], deleted_webhooks: string[] }
 *   POST {base}/__fake/inject   <- { channel_id, content, author: { id, username, avatar? } }  (a MESSAGE_CREATE on
 *                                  every connected gateway client)
 *   POST {base}/__fake/reset
 *
 * The server runs with SR_BRIDGE_DISCORD_API={base}/api/v10 and SR_BRIDGE_DISCORD_GATEWAY=ws://127.0.0.1:8090/gateway.
 * Never a real token: `FAKE_BOT_TOKEN` is only understood by the fake.
 */
export const FAKE_DISCORD_URL = process.env.SR_FAKE_DISCORD_URL ?? 'http://127.0.0.1:8090';
/** The token the fake accepts as a bot token (`GET /users/@me` answers `bot: true`). */
export const FAKE_BOT_TOKEN = process.env.SR_FAKE_DISCORD_TOKEN ?? 'fake-bot-token-for-tests-only';
/** The Discord channel the fake's guild offers (the name the mapping table shows). */
export const FAKE_CHANNEL_NAME = process.env.SR_FAKE_DISCORD_CHANNEL ?? 'bridge-test';
/** Its id on the fake. */
export const FAKE_CHANNEL_ID = process.env.SR_FAKE_DISCORD_CHANNEL_ID ?? '9100000000000000001';

export interface FakeWebhook {
  id: string;
  channel_id: string;
}
export interface FakePost {
  webhook_id: string;
  channel_id: string;
  content: string;
  username?: string;
  avatar_url?: string;
}
export interface FakeState {
  /** Gateway clients that identified (the bridge's shard once it connected). */
  gateways: number;
  webhooks: FakeWebhook[];
  posts: FakePost[];
  /** Ids of webhooks a `DELETE /webhooks/{id}/{token}` reached. */
  deleted_webhooks: string[];
}

export const fakeState = async (request: APIRequestContext): Promise<FakeState> => {
  const res = await request.get(`${FAKE_DISCORD_URL}/__fake/state`);
  if (!res.ok()) throw new Error(`fake Discord state: ${res.status()}`);
  return (await res.json()) as FakeState;
};

export const fakeInject = async (
  request: APIRequestContext,
  body: {
    channel_id: string;
    content: string;
    author: { id: string; username: string; avatar?: string };
  },
): Promise<void> => {
  const res = await request.post(`${FAKE_DISCORD_URL}/__fake/inject`, { data: body });
  if (!res.ok()) throw new Error(`fake Discord inject: ${res.status()}`);
};

export const fakeReset = async (request: APIRequestContext): Promise<void> => {
  const res = await request.post(`${FAKE_DISCORD_URL}/__fake/reset`);
  if (!res.ok()) throw new Error(`fake Discord reset: ${res.status()}`);
};

// ---- BRG-SYN-003 (wave 35b): the structural half of the fake (roles, channels, overwrites, threads). Run the fake on
// 8094 for the sync-gateway specs: SR_FAKE_DISCORD_PORT=8094 SR_FAKE_DISCORD_URL=http://127.0.0.1:8094.

export interface FakeAuditEntry {
  action: number;
  target_id: string;
  /** The reason the writer sent; `sr-bridge:<op id>` for the bridge, null for a Discord admin. */
  reason: string | null;
  user_id: string;
}
export interface FakeStructure {
  roles: Array<Record<string, unknown>>;
  channels: Array<Record<string, unknown>>;
  audit: FakeAuditEntry[];
  requests: Array<{ method: string; path: string; body: unknown; reason: string | null }>;
}

export const fakeStructure = async (request: APIRequestContext): Promise<FakeStructure> => {
  const res = await request.get(`${FAKE_DISCORD_URL}/__fake/structure`);
  if (!res.ok()) throw new Error(`fake Discord structure: ${res.status()}`);
  return (await res.json()) as FakeStructure;
};

/** A Discord admin's edit (same dispatches as the bot's, but no audit reason). */
export const fakeAdminEdit = async (
  request: APIRequestContext,
  method: 'POST' | 'PATCH' | 'PUT' | 'DELETE',
  path: string,
  body?: unknown,
): Promise<void> => {
  const res = await request.post(`${FAKE_DISCORD_URL}/__fake/structure/admin`, {
    data: { method, path, body },
  });
  if (!res.ok()) throw new Error(`fake Discord admin edit: ${res.status()}`);
};

export const fakeStructureControl = async (
  request: APIRequestContext,
  body: {
    community?: boolean;
    premium_tier?: number;
    reset_rename_window?: boolean;
    rate_limit_next?: number;
  },
): Promise<void> => {
  const res = await request.post(`${FAKE_DISCORD_URL}/__fake/structure/control`, { data: body });
  if (!res.ok()) throw new Error(`fake Discord structure control: ${res.status()}`);
};

/** Objects that existed on Discord before the bridge (no events, no audit). */
export const fakeSeed = async (
  request: APIRequestContext,
  body: { roles?: Array<Record<string, unknown>>; channels?: Array<Record<string, unknown>> },
): Promise<void> => {
  const res = await request.post(`${FAKE_DISCORD_URL}/__fake/structure/seed`, { data: body });
  if (!res.ok()) throw new Error(`fake Discord seed: ${res.status()}`);
};
