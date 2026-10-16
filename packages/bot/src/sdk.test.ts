import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ApiError,
  Client,
  Intents,
  RateLimitError,
  Rest,
  Shard,
  SlashCommandBuilder,
} from './index';
import { Cache } from './cache';
import { Options } from './commands';
import { fakeFetch, socketFactory } from './helpers.test-util';

const API = 'https://x.test/api/v1/';

describe('Rest', () => {
  it('sends the bot credential and a JSON body, and decodes the reply', async () => {
    const { fetch, seen } = fakeFetch([
      { status: 200, body: { id: '1', channel_id: '2', content: 'hi', author: { id: '9' } } },
    ]);
    const rest = new Rest({ apiUrl: API, token: 'Bot tok', fetch });
    const m = await rest.createMessage('2', 'hi');
    expect(m.id).toBe('1');
    expect(seen[0]).toMatchObject({
      url: 'https://x.test/api/v1/channels/2/messages',
      method: 'POST',
      body: { content: 'hi' },
    });
    expect(seen[0]?.headers.Authorization).toBe('Bot tok');
  });

  it('maps error bodies to ApiError', async () => {
    const { fetch } = fakeFetch([
      { status: 403, body: { code: 50013, message: 'Missing Permissions' } },
    ]);
    const rest = new Rest({ apiUrl: API, token: 't', fetch });
    const err = await rest.deleteMessage('1', '2').catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err).toMatchObject({ status: 403, code: 50013 });
  });

  it('waits and retries on 429, then gives up after the budget', async () => {
    const sleeps: number[] = [];
    const sleep = async (ms: number) => void sleeps.push(ms);
    const a = fakeFetch([
      { status: 429, body: { retry_after: 0.5 } },
      { status: 200, body: { id: '7' } },
    ]);
    const ok = await new Rest({ apiUrl: API, token: 't', fetch: a.fetch, sleep }).currentUser();
    expect(ok.id).toBe('7');
    expect(sleeps).toEqual([500]);

    const b = fakeFetch(
      Array.from({ length: 5 }, () => ({ status: 429, body: { retry_after: 1 } })),
    );
    await expect(
      new Rest({ apiUrl: API, token: 't', fetch: b.fetch, sleep }).currentUser(),
    ).rejects.toBeInstanceOf(RateLimitError);
  });

  it('keeps the credential off interaction-token routes and out of JSON', async () => {
    const { fetch, seen } = fakeFetch([{ status: 204 }]);
    const rest = new Rest({ apiUrl: API, token: 'secret', fetch });
    await rest.interactionCallback({ id: '11', token: 'itok' }, 4, { content: 'ok', flags: 64 });
    expect(seen[0]?.url).toBe('https://x.test/api/v1/interactions/11/itok/callback');
    expect(seen[0]?.headers.Authorization).toBeUndefined();
    expect(seen[0]?.body).toEqual({ type: 4, data: { content: 'ok', flags: 64 } });
    expect(JSON.stringify(rest)).not.toContain('secret');
  });

  it('reads the recommended shard count', async () => {
    const { fetch } = fakeFetch([
      { status: 200, body: { url: 'wss://x.test/gateway', shards: 3 } },
    ]);
    expect(await new Rest({ apiUrl: API, token: 't', fetch }).gatewayBot()).toEqual({
      url: 'wss://x.test/gateway',
      shards: 3,
    });
  });
});

describe('Shard', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const opts = (
    f: ReturnType<typeof socketFactory>,
    extra: Partial<ConstructorParameters<typeof Shard>[0]> = {},
  ) => ({
    token: 'tok',
    intents: 513,
    shard: [1, 4] as [number, number],
    url: 'wss://gw.test/gateway',
    createSocket: f.createSocket,
    onDispatch: () => undefined,
    onFatal: () => undefined,
    minBackoffMs: 10,
    ...extra,
  });
  const ready = (s: number) => ({
    op: 0,
    s,
    t: 'READY',
    d: { session_id: 'sess1', resume_gateway_url: 'wss://gw.test/resume', user: { id: '9' } },
  });

  it('identifies with token, intents and shard, then forwards dispatches', () => {
    const f = socketFactory();
    const got: string[] = [];
    const sh = new Shard(opts(f, { onDispatch: (id, n) => void got.push(`${id}:${n}`) }));
    sh.connect();
    f.sockets[0]?.serverSends({ op: 10, d: { heartbeat_interval: 40000 } });
    expect(f.sockets[0]?.sent[0]).toMatchObject({
      op: 2,
      d: { token: 'tok', intents: 513, shard: [1, 4] },
    });
    f.sockets[0]?.serverSends(ready(1));
    expect(got).toEqual(['1:READY']);
    sh.destroy();
  });

  it('heartbeats with the last sequence; a missing ack resumes on the resume url', () => {
    const f = socketFactory();
    const sh = new Shard(opts(f));
    sh.connect();
    const s0 = f.sockets[0]!;
    s0.serverSends({ op: 10, d: { heartbeat_interval: 1000 } });
    s0.serverSends(ready(7));
    vi.advanceTimersByTime(1000);
    expect(s0.sent[1]).toEqual({ op: 1, d: 7 });
    vi.advanceTimersByTime(1000); // no ack: zombied
    expect(s0.closedWith).toBe(4000);
    vi.advanceTimersByTime(20);
    const s1 = f.sockets[1]!;
    expect(s1.url).toBe('wss://gw.test/resume');
    s1.serverSends({ op: 10, d: { heartbeat_interval: 40000 } });
    expect(s1.sent[0]).toEqual({ op: 6, d: { token: 'tok', session_id: 'sess1', seq: 7 } });
    sh.destroy();
  });

  it('invalid session (false) forgets the session and identifies on the base url', () => {
    const f = socketFactory();
    const sh = new Shard(opts(f));
    sh.connect();
    const s0 = f.sockets[0]!;
    s0.serverSends({ op: 10, d: { heartbeat_interval: 40000 } });
    s0.serverSends(ready(1));
    s0.serverSends({ op: 9, d: false });
    vi.advanceTimersByTime(20);
    const s1 = f.sockets[1]!;
    expect(s1.url).toBe('wss://gw.test/gateway');
    s1.serverSends({ op: 10, d: { heartbeat_interval: 40000 } });
    expect(s1.sent[0]).toMatchObject({ op: 2 });
    sh.destroy();
  });

  it('a fatal close code stops the shard and reports it; others reconnect', () => {
    const f = socketFactory();
    const fatal: [number, string][] = [];
    const sh = new Shard(opts(f, { onFatal: (c, r) => void fatal.push([c, r]) }));
    sh.connect();
    f.sockets[0]?.serverCloses(1006);
    vi.advanceTimersByTime(20);
    expect(f.sockets).toHaveLength(2);
    f.sockets[1]?.serverCloses(4014);
    vi.advanceTimersByTime(1000);
    expect(fatal).toEqual([[4014, 'disallowed intents']]);
    expect(f.sockets).toHaveLength(2);
  });
});

describe('commands', () => {
  it('builders serialize like the API expects', () => {
    const def = new SlashCommandBuilder()
      .setName('roll')
      .setDescription('Roll dice')
      .setDefaultMemberPermissions('0')
      .addIntegerOption((o) =>
        o
          .setName('sides')
          .setDescription('Faces')
          .setRequired()
          .addChoices({ name: 'd6', value: 6 }),
      )
      .addAttachmentOption((o) => o.setName('image').setDescription('A picture'))
      .addSubcommand((s) =>
        s
          .setName('hard')
          .setDescription('Hard mode')
          .addBooleanOption((o) => o.setName('x').setDescription('X')),
      );
    expect(def.toJSON()).toEqual({
      type: 1,
      name: 'roll',
      description: 'Roll dice',
      default_member_permissions: '0',
      options: [
        {
          type: 4,
          name: 'sides',
          description: 'Faces',
          required: true,
          choices: [{ name: 'd6', value: 6 }],
        },
        { type: 11, name: 'image', description: 'A picture' },
        {
          type: 1,
          name: 'hard',
          description: 'Hard mode',
          options: [{ type: 5, name: 'x', description: 'X' }],
        },
      ],
    });
  });

  it('options read through sub-commands and report the focused option', () => {
    const o = new Options([
      {
        type: 2,
        name: 'grp',
        options: [
          {
            type: 1,
            name: 'add',
            options: [
              { type: 3, name: 'name', value: 'fun', focused: true },
              { type: 4, name: 'n', value: 3 },
              { type: 5, name: 'loud', value: true },
              { type: 6, name: 'who', value: '77' },
            ],
          },
        ],
      },
    ]);
    expect([o.subcommandGroup, o.subcommand]).toEqual(['grp', 'add']);
    expect([o.getString('name'), o.getInteger('n'), o.getBoolean('loud'), o.getId('who')]).toEqual([
      'fun',
      3,
      true,
      '77',
    ]);
    expect(o.focused).toEqual({ name: 'name', value: 'fun' });
    expect(o.getString('n')).toBeNull();
  });
});

describe('Client', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('registers commands at READY, routes interactions and emits events', async () => {
    const { fetch, seen } = fakeFetch([
      { status: 200, body: { url: 'wss://gw.test/gateway', shards: 1 } },
      { status: 200, body: [] },
      { status: 204 },
    ]);
    const f = socketFactory();
    const client = new Client({
      token: 'tok',
      apiUrl: API,
      intents: Intents.Guilds | Intents.MessageContent,
      fetch,
      createSocket: f.createSocket,
    });
    client.command(new SlashCommandBuilder().setName('hi').setDescription('Say hi'), (i) =>
      i.reply('hello'),
    );
    const msgs: string[] = [];
    client.on('messageCreate', (m) => void msgs.push(m.content));
    await client.login();
    const s = f.sockets[0]!;
    s.serverSends({ op: 10, d: { heartbeat_interval: 40000 } });
    s.serverSends({
      op: 0,
      s: 1,
      t: 'READY',
      d: { session_id: 's', user: { id: '9', bot: true }, application: { id: '5' }, guilds: [] },
    });
    s.serverSends({
      op: 0,
      s: 2,
      t: 'MESSAGE_CREATE',
      d: { id: '1', channel_id: '2', content: 'yo', author: { id: '3' } },
    });
    s.serverSends({
      op: 0,
      s: 3,
      t: 'INTERACTION_CREATE',
      d: { id: '11', application_id: '5', type: 2, token: 'itok', data: { name: 'hi' } },
    });
    await vi.advanceTimersByTimeAsync(10);
    expect(msgs).toEqual(['yo']);
    expect(client.cache.applicationId).toBe('5');
    expect(seen[1]).toMatchObject({
      url: 'https://x.test/api/v1/applications/5/commands',
      method: 'PUT',
    });
    expect((seen[1]?.body as { name: string }[])[0]?.name).toBe('hi');
    expect(seen[2]?.url).toBe('https://x.test/api/v1/interactions/11/itok/callback');
    client.destroy();
  });

  it('a failing handler tells the user and surfaces the error', async () => {
    const { fetch, seen } = fakeFetch([{ status: 204 }]);
    const f = socketFactory();
    const client = new Client({
      token: 't',
      apiUrl: API,
      gatewayUrl: 'wss://gw.test/gateway',
      fetch,
      createSocket: f.createSocket,
      registerCommands: false,
    });
    client.command(new SlashCommandBuilder().setName('boom').setDescription('Boom'), () => {
      throw new Error('kaput');
    });
    const errors: string[] = [];
    client.on('error', (e) => void errors.push(e.message));
    await client.login();
    const s = f.sockets[0]!;
    s.serverSends({ op: 10, d: { heartbeat_interval: 40000 } });
    s.serverSends({
      op: 0,
      s: 1,
      t: 'INTERACTION_CREATE',
      d: { id: '1', application_id: '5', type: 2, token: 'tk', data: { name: 'boom' } },
    });
    await vi.advanceTimersByTimeAsync(10);
    expect(errors).toEqual(['kaput']);
    expect(seen[0]?.body).toMatchObject({ type: 4, data: { flags: 64 } });
    client.destroy();
  });

  it('a fatal close ends the client and emits fatal', async () => {
    const f = socketFactory();
    const client = new Client({
      token: 't',
      apiUrl: API,
      gatewayUrl: 'wss://gw.test/gateway',
      createSocket: f.createSocket,
    });
    const fatal: number[] = [];
    client.on('fatal', (c) => void fatal.push(c));
    await client.login();
    f.sockets[0]?.serverCloses(4004);
    expect(fatal).toEqual([4004]);
  });
});

describe('Cache', () => {
  it('follows guild, channel and role events', () => {
    const c = new Cache();
    c.apply('READY', {
      user: { id: '9' },
      application: { id: '5' },
      guilds: [{ id: '1', unavailable: true }],
    });
    c.apply('GUILD_CREATE', {
      id: '1',
      name: 'Home',
      channels: [{ id: '10', name: 'general', guild_id: '1' }],
      roles: [{ id: '20', name: 'Mod' }],
    });
    expect(c.channel('10')?.name).toBe('general');
    c.apply('CHANNEL_CREATE', { id: '11', name: 'new', guild_id: '1' });
    c.apply('CHANNEL_DELETE', { id: '10', guild_id: '1' });
    expect(c.channel('10')).toBeUndefined();
    expect(c.channel('11')?.name).toBe('new');
    c.apply('GUILD_ROLE_UPDATE', { guild_id: '1', role: { id: '20', name: 'Admin' } });
    expect(c.role('1', '20')?.name).toBe('Admin');
    c.apply('GUILD_DELETE', { id: '1' });
    expect(c.guilds.size).toBe(0);
  });
});

describe('address rule (D-234) and gateway ops', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const shardOpts = (f: ReturnType<typeof socketFactory>, allowedHosts?: string[]) => ({
    token: 'tok',
    intents: 513,
    shard: [0, 1] as [number, number],
    url: 'wss://gw.test/gateway',
    createSocket: f.createSocket,
    onDispatch: () => undefined,
    onFatal: () => undefined,
    minBackoffMs: 10,
    allowedHosts,
  });
  const readyAt = (resume: string) => ({
    op: 0,
    s: 1,
    t: 'READY',
    d: { session_id: 's1', resume_gateway_url: resume, user: { id: '9' } },
  });

  it('a resume url on another host falls back; an allow-listed host is followed', () => {
    for (const [allow, resume, expected] of [
      [undefined, 'wss://evil.test/r', 'wss://gw.test/gateway'],
      [['resume.test'], 'wss://RESUME.test/r', 'wss://RESUME.test/r'],
    ] as const) {
      const f = socketFactory();
      const sh = new Shard(shardOpts(f, allow ? [...allow] : undefined));
      sh.connect();
      f.sockets[0]?.serverSends({ op: 10, d: { heartbeat_interval: 40000 } });
      f.sockets[0]?.serverSends(readyAt(resume));
      f.sockets[0]?.serverCloses(1006);
      vi.advanceTimersByTime(20);
      expect(f.sockets[1]?.url).toBe(expected);
      sh.destroy();
    }
  });

  it('a gateway url from /gateway/bot on another host falls back to the api host', async () => {
    const { fetch } = fakeFetch([
      { status: 200, body: { url: 'wss://evil.test/gateway', shards: 1 } },
    ]);
    const f = socketFactory();
    const client = new Client({ token: 'tok', apiUrl: API, fetch, createSocket: f.createSocket });
    await client.login();
    expect(f.sockets[0]?.url).toBe('wss://x.test/gateway');
    client.destroy();

    const ok = fakeFetch([{ status: 200, body: { url: 'wss://gw.test/gateway', shards: 1 } }]);
    const f2 = socketFactory();
    const c2 = new Client({
      token: 'tok',
      apiUrl: API,
      fetch: ok.fetch,
      createSocket: f2.createSocket,
      allowGatewayHosts: ['GW.test'],
    });
    await c2.login();
    expect(f2.sockets[0]?.url).toBe('wss://gw.test/gateway');
    c2.destroy();
  });

  it('requestGuildMembers sends op 8 and streams the chunks of its nonce; setPresence sends op 3', async () => {
    const { fetch } = fakeFetch([
      { status: 200, body: { url: 'wss://x.test/gateway', shards: 1 } },
    ]);
    const f = socketFactory();
    const client = new Client({ token: 'tok', apiUrl: API, fetch, createSocket: f.createSocket });
    await client.login();
    const s = f.sockets[0]!;
    s.serverSends({ op: 10, d: { heartbeat_interval: 40000 } });
    s.serverSends(readyAt('wss://x.test/r'));

    const got: number[] = [];
    const run = (async () => {
      for await (const c of client.requestGuildMembers({ guildId: '42', limit: 0 })) {
        got.push(c.chunk_index);
      }
    })();
    const frame = s.sent.find((x) => x.op === 8) as { d: Record<string, unknown> };
    expect(frame.d).toMatchObject({ guild_id: '42', query: '', limit: 0 });
    const nonce = frame.d.nonce as string;
    for (const i of [0, 1]) {
      s.serverSends({
        op: 0,
        s: 2 + i,
        t: 'GUILD_MEMBERS_CHUNK',
        d: { guild_id: '42', members: [], chunk_index: i, chunk_count: 2, nonce },
      });
    }
    await run;
    expect(got).toEqual([0, 1]);

    expect(client.setPresence({ status: 'idle', activities: [{ name: 'chess', type: 0 }] })).toBe(
      true,
    );
    expect(s.sent.at(-1)).toEqual({
      op: 3,
      d: { status: 'idle', activities: [{ name: 'chess', type: 0 }], since: null, afk: false },
    });
    client.destroy();
  });
});
