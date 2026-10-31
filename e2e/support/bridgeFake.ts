import type { APIRequestContext } from '@playwright/test';

/**
 * A thin client for the fake Discord (`e2e/support/fake-discord.mjs`, owned by bridge-core, started on 8090 by the
 * BRG-COR-001 setup). The fake is a second `sr-server` that serves Discord's webhook and gateway shapes; these helpers
 * read what it received and make it emit a message, through its control routes.
 *
 * The control routes below are the CONTRACT THIS FILE ASSUMES (BRG-UI-001 was written before bridge-core's fake
 * existed); reconcile the paths and fields here, in this one file, if the fake names them differently:
 *
 *   GET  {base}/__fake/state    -> { webhooks: Webhook[], posts: Post[], deleted_webhooks: string[] }
 *   POST {base}/__fake/inject   <- { channel_id, content, author: { id, username, avatar? } }  (emits MESSAGE_CREATE)
 *   POST {base}/__fake/reset
 *
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
