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
//   POST /__fake/inject   <- { channel_id, content, author: { id, username, avatar? }, kind?: 'message'|'typing' }
//                            Recorded in `injected` and sent to every connected gateway client as a
//                            MESSAGE_CREATE (or TYPING_START) of the fake Discord server; answers { ok, id, gateways }.
//   POST /__fake/control  <- { rate_limit_next?: n, fail_next?: n, exhaust_bucket_secs?: s|null, undeletable?: [ids],
//                              deny_intents?: mask }   (an IDENTIFY asking for any intent in the mask is closed 4014)
//   POST /__fake/reset
//
// and the gateway (wave 35 integration, BRG-GW-001), a WebSocket on the same port written with node built-ins only:
//
//   ws://127.0.0.1:8090/gateway   (SR_BRIDGE_DISCORD_GATEWAY=ws://127.0.0.1:8090/gateway)
//
// HELLO, IDENTIFY (the fake bot token, else close 4004), READY, GUILD_CREATE of the fake server (its channels and
// the members injected so far), heartbeat ACKs, op 8 answered with one GUILD_MEMBERS_CHUNK, RESUME answered with an
// invalid session (the client identifies again).
//
// Real Discord is never contacted, and no real token is understood: FAKE_BOT_TOKEN only means something here.

import crypto from 'node:crypto';
import http from 'node:http';
import { createStructure } from './fake-discord-structure.mjs';

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
  members: new Map(), // discord user id -> member object (from injected authors)
  control: {
    rate_limit_next: 0,
    fail_next: 0,
    exhaust_bucket_secs: null,
    undeletable: new Set(),
    deny_intents: 0,
  },
});
state = fresh();
const nextId = () => (state.next++).toString();
// BRG-SYN-003: roles, channels, overwrites, threads and tags (e2e/support/fake-discord-structure.mjs).
const structure = createStructure({
  guildId: GUILD_ID,
  botId: BOT_ID,
  channels: CHANNELS,
  nextId: () => nextId(),
  dispatch: (name, d) => dispatch(name, d),
});

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
      gateways: [...gateways].filter((g) => g.ready).length,
      webhooks: [...state.webhooks].map(([id, w]) => ({ id, channel_id: w.channel_id })),
      posts: state.posts,
      deleted_webhooks: state.deletedWebhooks,
      deleted_messages: state.deletedMessages,
      injected: state.injected,
    });
  }
  if (path === '/__fake/reset' && method === 'POST') {
    state = fresh();
    structure.reset();
    return json(res, 200, { ok: true });
  }
  if (path === '/__fake/inject' && method === 'POST') {
    const b = await readBody(req);
    state.injected.push(b);
    const author = {
      id: String(b.author?.id ?? '9100000000000000050'),
      username: b.author?.username ?? 'discord-user',
      global_name: b.author?.global_name ?? b.author?.username ?? 'Discord user',
      avatar: b.author?.avatar ?? null,
    };
    const member = { user: author, nick: null, roles: [], joined_at: new Date().toISOString() };
    const known = state.members.has(author.id);
    state.members.set(author.id, member);
    if (!known) dispatch('GUILD_MEMBER_ADD', { ...member, guild_id: GUILD_ID });
    if (b.kind === 'typing') {
      const n = dispatch('TYPING_START', {
        channel_id: String(b.channel_id),
        guild_id: GUILD_ID,
        user_id: author.id,
        timestamp: Math.floor(Date.now() / 1000),
      });
      return json(res, 200, { ok: true, gateways: n });
    }
    const id = nextId();
    const n = dispatch('MESSAGE_CREATE', {
      id,
      type: 0,
      channel_id: String(b.channel_id),
      guild_id: GUILD_ID,
      content: b.content ?? '',
      author,
      member: { nick: null, roles: [] },
      attachments: b.attachments ?? [],
      embeds: [],
      mentions: [],
      mention_roles: [],
      mention_everyone: false,
      pinned: false,
      tts: false,
      timestamp: new Date().toISOString(),
    });
    return json(res, 200, { ok: true, id, gateways: n });
  }
  if (path === '/__fake/control' && method === 'POST') {
    const b = await readBody(req);
    const c = state.control;
    if (typeof b.rate_limit_next === 'number') c.rate_limit_next = b.rate_limit_next;
    if (typeof b.fail_next === 'number') c.fail_next = b.fail_next;
    if ('exhaust_bucket_secs' in b) c.exhaust_bucket_secs = b.exhaust_bucket_secs;
    if (Array.isArray(b.undeletable)) c.undeletable = new Set(b.undeletable.map(String));
    if (typeof b.deny_intents === 'number') c.deny_intents = b.deny_intents;
    return json(res, 200, { ok: true });
  }

  if (path.startsWith('/__fake/structure')) {
    const b = method === 'POST' ? await readBody(req) : {};
    const r = structure.handleControl(method, path, b);
    if (r) return json(res, r.status, r.body);
  }

  // ---- Discord
  const segs = path
    .replace(/^\/api\/v\d+\//, '')
    .split('/')
    .filter(Boolean);
  if (!path.startsWith('/api/')) return json(res, 404, { message: '404: Not Found', code: 0 });
  const bot = botOf(req);
  const unauthorized = () => json(res, 401, { message: '401: Unauthorized', code: 0 });

  if (method === 'GET' && segs.join('/') === 'users/@me') {
    return bot
      ? json(res, 200, { id: bot.id, username: 'sideroom-bridge-bot', bot: bot.bot })
      : unauthorized();
  }
  if (method === 'GET' && segs.join('/') === 'users/@me/guilds') {
    return bot ? json(res, 200, [{ id: GUILD_ID, name: 'Fake Discord' }]) : unauthorized();
  }
  if (segs[0] === 'guilds' || (segs[0] === 'channels' && segs[2] !== 'webhooks')) {
    const body = ['POST', 'PUT', 'PATCH'].includes(method) ? await readBody(req) : {};
    const r = structure.handleRest(method, segs, body, req.headers, bot);
    if (r) {
      if (r.status === 204) {
        res.writeHead(204);
        return res.end();
      }
      return json(res, r.status, r.body, r.headers);
    }
  }
  if (method === 'POST' && segs[0] === 'channels' && segs[2] === 'webhooks') {
    if (!bot) return unauthorized();
    if (!structure.channelExists(segs[1]))
      return json(res, 404, { message: 'Unknown Channel', code: 10003 });
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
        return json(
          res,
          429,
          { message: 'You are being rate limited.', retry_after: 0.2, global: false },
          { 'retry-after': '1' },
        );
      }
      if (c.fail_next > 0) {
        c.fail_next -= 1;
        return json(res, 500, { message: 'internal error' });
      }
      const body = await readBody(req);
      const messageId = nextId();
      state.messages.set(`${id}:${messageId}`, body);
      const target = structure.webhookTarget(hook.channel_id, url.searchParams, body, id);
      state.posts.push({
        webhook_id: id,
        channel_id: target,
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
      return json(
        res,
        200,
        { id: messageId, content: body.content ?? '', webhook_id: id, channel_id: target },
        headers,
      );
    }
    if (sub === 'messages' && mid) {
      const key = `${id}:${mid}`;
      if (!known || !state.messages.has(key))
        return json(res, 404, { message: 'Unknown Message', code: 10008 });
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

// ---- the gateway: RFC 6455 over node:http's upgrade, text frames only (what the bridge's shard speaks)

const gateways = new Set();

const frame = (opcode, payload) => {
  const len = payload.length;
  let head;
  if (len < 126) head = Buffer.from([0x80 | opcode, len]);
  else if (len < 65536) {
    head = Buffer.alloc(4);
    head[0] = 0x80 | opcode;
    head[1] = 126;
    head.writeUInt16BE(len, 2);
  } else {
    head = Buffer.alloc(10);
    head[0] = 0x80 | opcode;
    head[1] = 127;
    head.writeBigUInt64BE(BigInt(len), 2);
  }
  return Buffer.concat([head, payload]);
};

const sendText = (g, value) => {
  if (!g.open) return;
  g.socket.write(frame(0x1, Buffer.from(JSON.stringify(value), 'utf8')));
};

const closeWith = (g, code) => {
  if (!g.open) return;
  const p = Buffer.alloc(2);
  p.writeUInt16BE(code, 0);
  g.socket.write(frame(0x8, p));
  g.open = false;
  gateways.delete(g);
  setTimeout(() => g.socket.destroy(), 50);
};

/** Send a dispatch to every identified client; how many got it. */
function dispatch(name, d) {
  let n = 0;
  for (const g of gateways) {
    if (!g.ready) continue;
    g.seq += 1;
    sendText(g, { op: 0, t: name, s: g.seq, d });
    n += 1;
  }
  return n;
}

const guildCreate = () => ({
  id: GUILD_ID,
  name: 'Fake Discord',
  unavailable: false,
  member_count: state.members.size + 1,
  roles: structure.snapshot().roles,
  channels: structure.snapshot().channels,
  members: [...state.members.values()],
  presences: [],
});

function onGatewayMessage(g, text) {
  let msg;
  try {
    msg = JSON.parse(text);
  } catch {
    return closeWith(g, 4002);
  }
  switch (msg.op) {
    case 1: // heartbeat
      return sendText(g, { op: 11 });
    case 2: {
      // identify
      const token = String(msg.d?.token ?? '').replace(/^Bot /, '');
      if (token !== FAKE_BOT_TOKEN) return closeWith(g, 4004);
      const intents = Number(msg.d?.intents ?? 0);
      if (intents & state.control.deny_intents) return closeWith(g, 4014);
      g.ready = true;
      console.log(new Date().toISOString(), 'gateway identified');
      g.seq = 0;
      g.seq += 1;
      sendText(g, {
        op: 0,
        t: 'READY',
        s: g.seq,
        d: {
          v: 10,
          user: { id: BOT_ID, username: 'sideroom-bridge-bot', bot: true },
          guilds: [{ id: GUILD_ID, unavailable: true }],
          session_id: crypto.randomUUID(),
          resume_gateway_url: `ws://127.0.0.1:${PORT}/gateway`,
          application: { id: BOT_ID, flags: 0 },
        },
      });
      g.seq += 1;
      sendText(g, { op: 0, t: 'GUILD_CREATE', s: g.seq, d: guildCreate() });
      return;
    }
    case 6: // resume: this fake keeps no sessions, so the client identifies again
      return sendText(g, { op: 9, d: false });
    case 8: {
      // request guild members: one chunk with everyone injected so far
      g.seq += 1;
      return sendText(g, {
        op: 0,
        t: 'GUILD_MEMBERS_CHUNK',
        s: g.seq,
        d: {
          guild_id: GUILD_ID,
          members: [...state.members.values()],
          chunk_index: 0,
          chunk_count: 1,
          presences: [],
        },
      });
    }
    default:
      return undefined;
  }
}

server.on('upgrade', (req, socket) => {
  const url = new URL(req.url, 'http://fake');
  const key = req.headers['sec-websocket-key'];
  if (url.pathname !== '/gateway' || !key) {
    socket.end('HTTP/1.1 404 Not Found\r\n\r\n');
    return;
  }
  const accept = crypto
    .createHash('sha1')
    .update(key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11')
    .digest('base64');
  socket.write(
    'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n' +
      `Sec-WebSocket-Accept: ${accept}\r\n\r\n`,
  );
  socket.setNoDelay(true);
  const g = { socket, open: true, ready: false, seq: 0, buf: Buffer.alloc(0), parts: [] };
  gateways.add(g);
  sendText(g, { op: 10, d: { heartbeat_interval: 41250 } });
  socket.on('data', (chunk) => {
    g.buf = Buffer.concat([g.buf, chunk]);
    for (;;) {
      if (g.buf.length < 2) return;
      const fin = (g.buf[0] & 0x80) !== 0;
      const opcode = g.buf[0] & 0x0f;
      const masked = (g.buf[1] & 0x80) !== 0;
      let len = g.buf[1] & 0x7f;
      let off = 2;
      if (len === 126) {
        if (g.buf.length < 4) return;
        len = g.buf.readUInt16BE(2);
        off = 4;
      } else if (len === 127) {
        if (g.buf.length < 10) return;
        len = Number(g.buf.readBigUInt64BE(2));
        off = 10;
      }
      const maskOff = off;
      if (masked) off += 4;
      if (g.buf.length < off + len) return;
      const payload = Buffer.from(g.buf.subarray(off, off + len));
      if (masked) for (let i = 0; i < len; i++) payload[i] ^= g.buf[maskOff + (i % 4)];
      g.buf = g.buf.subarray(off + len);
      if (opcode === 0x8) {
        if (g.open) socket.write(frame(0x8, payload.subarray(0, 2)));
        g.open = false;
        gateways.delete(g);
        socket.end();
        return;
      }
      if (opcode === 0x9) {
        socket.write(frame(0xa, payload));
        continue;
      }
      if (opcode === 0x1 || opcode === 0x0) {
        g.parts.push(payload);
        if (fin) {
          const text = Buffer.concat(g.parts).toString('utf8');
          g.parts = [];
          onGatewayMessage(g, text);
        }
      }
    }
  });
  const gone = () => {
    if (gateways.has(g)) console.log(new Date().toISOString(), 'gateway closed');
    g.open = false;
    gateways.delete(g);
  };
  // The upgraded socket comes from node:http, which allows half-open sockets: a client that closes without a
  // close frame only sends FIN, so end our side too.
  socket.on('end', () => {
    gone();
    socket.end();
  });
  socket.on('close', gone);
  socket.on('error', gone);
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`fake Discord listening on http://127.0.0.1:${PORT} (bot token: ${FAKE_BOT_TOKEN})`);
});

for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => server.close(() => process.exit(0)));
