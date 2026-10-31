// The fake Discord of the bridge's end-to-end suite (BRG-COR-001). Plain Node, no dependencies.
//
//   node e2e/support/fake-discord.mjs            # listens on 127.0.0.1:8090 (SR_FAKE_DISCORD_PORT overrides)
//   SR_BRIDGE_DISCORD_API=http://127.0.0.1:8090/api/v10 sr-server ...
//
// It serves the REST routes the bridge uses, with Discord's shapes, rate-limit headers and status codes:
//
//   GET    /api/v10/users/@me                                  the bot (`bot: true`), or a user (`bot: false`)
//   GET    /api/v10/users/@me/guilds
//   GET    /api/v10/guilds/{id}/channels
//   POST   /api/v10/channels/{id}/webhooks                     -> { id, token }
//   POST   /api/v10/webhooks/{id}/{token}?wait=true            -> { id }   (X-RateLimit-* headers)
//   PATCH  /api/v10/webhooks/{id}/{token}/messages/{mid}
//   DELETE /api/v10/webhooks/{id}/{token}/messages/{mid}
//   DELETE /api/v10/webhooks/{id}/{token}
//
// and the control routes the end-to-end specs use (e2e/support/bridgeFake.ts):
//
//   GET  /__fake/state    -> { webhooks, posts, deleted_webhooks, deleted_messages, injected }
//   POST /__fake/inject   <- { channel_id, content, author: { id, username, avatar? } }
//                            Recorded in `injected`. The gateway (MESSAGE_CREATE dispatch) belongs to BRG-GW-001
//                            and its own fake on 8091; this route only records, so a spec can assert the call.
//   POST /__fake/control  <- { rate_limit_next?: n, fail_next?: n, exhaust_bucket_secs?: s|null, undeletable?: [ids] }
//   POST /__fake/reset
//
// Real Discord is never contacted, and no real token is understood: FAKE_BOT_TOKEN only means something here.

import http from 'node:http';

const PORT = Number(process.env.SR_FAKE_DISCORD_PORT ?? 8090);
export const FAKE_BOT_TOKEN = process.env.SR_FAKE_DISCORD_TOKEN ?? 'fake-bot-token-for-tests-only';
export const FAKE_USER_TOKEN = 'fake-user-token-for-tests-only';
const GUILD_ID = '9100000000000000000';
const CHANNELS = [
  { id: '9100000000000000001', name: 'bridge-test', type: 0 },
  { id: '9100000000000000002', name: 'other-test', type: 0 },
  { id: '9100000000000000003', name: 'voice-test', type: 2 },
];
const BOT_ID = '9100000000000000099';

let state;
const fresh = () => ({
  next: 9100000000001000n,
  webhooks: new Map(), // id -> { token, channel_id }
  messages: new Map(), // `${webhook}:${mid}` -> body
  posts: [],
  deletedWebhooks: [],
  deletedMessages: [],
  injected: [],
  control: { rate_limit_next: 0, fail_next: 0, exhaust_bucket_secs: null, undeletable: new Set() },
});
state = fresh();
const nextId = () => (state.next++).toString();

const json = (res, status, body, headers = {}) => {
  const text = body === undefined ? '' : JSON.stringify(body);
  res.writeHead(status, { 'content-type': 'application/json', ...headers });
  res.end(text);
};

const readBody = (req) =>
  new Promise((resolve) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      try {
        resolve(chunks.length ? JSON.parse(Buffer.concat(chunks).toString('utf8')) : {});
      } catch {
        resolve({});
      }
    });
  });

const botOf = (req) => {
  const auth = req.headers.authorization ?? '';
  if (auth === `Bot ${FAKE_BOT_TOKEN}`) return { id: BOT_ID, bot: true };
  if (auth === `Bot ${FAKE_USER_TOKEN}`) return { id: '9100000000000098', bot: false };
  return null;
};

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://fake');
  const path = url.pathname;
  const method = req.method ?? 'GET';

  // ---- control routes
  if (path === '/__fake/state' && method === 'GET') {
    return json(res, 200, {
      webhooks: [...state.webhooks].map(([id, w]) => ({ id, channel_id: w.channel_id })),
      posts: state.posts,
      deleted_webhooks: state.deletedWebhooks,
      deleted_messages: state.deletedMessages,
      injected: state.injected,
    });
  }
  if (path === '/__fake/reset' && method === 'POST') {
    state = fresh();
    return json(res, 200, { ok: true });
  }
  if (path === '/__fake/inject' && method === 'POST') {
    state.injected.push(await readBody(req));
    return json(res, 200, { ok: true });
  }
  if (path === '/__fake/control' && method === 'POST') {
    const b = await readBody(req);
    const c = state.control;
    if (typeof b.rate_limit_next === 'number') c.rate_limit_next = b.rate_limit_next;
    if (typeof b.fail_next === 'number') c.fail_next = b.fail_next;
    if ('exhaust_bucket_secs' in b) c.exhaust_bucket_secs = b.exhaust_bucket_secs;
    if (Array.isArray(b.undeletable)) c.undeletable = new Set(b.undeletable.map(String));
    return json(res, 200, { ok: true });
  }

  // ---- Discord
  const segs = path.replace(/^\/api\/v\d+\//, '').split('/').filter(Boolean);
  if (!path.startsWith('/api/')) return json(res, 404, { message: '404: Not Found', code: 0 });
  const bot = botOf(req);
  const unauthorized = () => json(res, 401, { message: '401: Unauthorized', code: 0 });

  if (method === 'GET' && segs.join('/') === 'users/@me') {
    return bot ? json(res, 200, { id: bot.id, username: 'sideroom-bridge-bot', bot: bot.bot }) : unauthorized();
  }
  if (method === 'GET' && segs.join('/') === 'users/@me/guilds') {
    return bot ? json(res, 200, [{ id: GUILD_ID, name: 'Fake Discord' }]) : unauthorized();
  }
  if (method === 'GET' && segs[0] === 'guilds' && segs[2] === 'channels' && segs.length === 3) {
    if (!bot) return unauthorized();
    return json(res, 200, segs[1] === GUILD_ID ? CHANNELS : []);
  }
  if (method === 'POST' && segs[0] === 'channels' && segs[2] === 'webhooks') {
    if (!bot) return unauthorized();
    if (!CHANNELS.some((c) => c.id === segs[1])) return json(res, 404, { message: 'Unknown Channel', code: 10003 });
    const id = nextId();
    const token = `fakewhtoken${id}`;
    state.webhooks.set(id, { token, channel_id: segs[1] });
    return json(res, 200, { id, token, channel_id: segs[1], name: 'SIDE ROOM bridge' });
  }
  if (segs[0] === 'webhooks' && segs.length >= 3) {
    const [, id, token, sub, mid] = segs;
    const hook = state.webhooks.get(id);
    const known = hook && hook.token === token;
    const c = state.control;
    if (method === 'POST' && segs.length === 3) {
      if (!known) return json(res, 404, { message: 'Unknown Webhook', code: 10015 });
      if (c.rate_limit_next > 0) {
        c.rate_limit_next -= 1;
        return json(res, 429, { message: 'You are being rate limited.', retry_after: 0.2, global: false }, { 'retry-after': '1' });
      }
      if (c.fail_next > 0) {
        c.fail_next -= 1;
        return json(res, 500, { message: 'internal error' });
      }
      const body = await readBody(req);
      const messageId = nextId();
      state.messages.set(`${id}:${messageId}`, body);
      state.posts.push({
        webhook_id: id,
        channel_id: hook.channel_id,
        message_id: messageId,
        content: body.content ?? '',
        username: body.username,
        avatar_url: body.avatar_url,
        allowed_mentions: body.allowed_mentions,
      });
      const headers = { 'x-ratelimit-remaining': '4', 'x-ratelimit-limit': '5' };
      if (c.exhaust_bucket_secs != null) {
        headers['x-ratelimit-remaining'] = '0';
        headers['x-ratelimit-reset-after'] = String(c.exhaust_bucket_secs);
      }
      return json(res, 200, { id: messageId, content: body.content ?? '', webhook_id: id }, headers);
    }
    if (sub === 'messages' && mid) {
      const key = `${id}:${mid}`;
      if (!known || !state.messages.has(key)) return json(res, 404, { message: 'Unknown Message', code: 10008 });
      if (method === 'PATCH') {
        const body = await readBody(req);
        state.messages.set(key, { ...state.messages.get(key), content: body.content });
        const post = state.posts.find((p) => p.message_id === mid);
        if (post) post.content = body.content ?? post.content;
        return json(res, 200, { id: mid });
      }
      if (method === 'DELETE') {
        state.messages.delete(key);
        state.deletedMessages.push(mid);
        state.posts = state.posts.filter((p) => p.message_id !== mid);
        res.writeHead(204);
        return res.end();
      }
    }
    if (method === 'DELETE' && segs.length === 3) {
      if (c.undeletable.has(id)) return json(res, 500, { message: 'internal error' });
      if (!known) return json(res, 404, { message: 'Unknown Webhook', code: 10015 });
      state.webhooks.delete(id);
      state.deletedWebhooks.push(id);
      res.writeHead(204);
      return res.end();
    }
  }
  return json(res, 404, { message: '404: Not Found', code: 0 });
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`fake Discord listening on http://127.0.0.1:${PORT} (bot token: ${FAKE_BOT_TOKEN})`);
});

for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => server.close(() => process.exit(0)));
