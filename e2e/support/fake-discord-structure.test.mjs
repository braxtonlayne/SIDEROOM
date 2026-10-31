// Smoke test of the structural fake Discord (BRG-SYN-003). Runs the fake on 8094 and drives it over HTTP.
//   node --test e2e/support/fake-discord-structure.test.mjs
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { after, before, test } from 'node:test';

const PORT = 8094;
const BASE = `http://127.0.0.1:${PORT}`;
const TOKEN = 'fake-bot-token-for-tests-only';
const GUILD = '9100000000000000000';
let child;

const call = async (method, path, body, headers = {}) => {
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers: { authorization: `Bot ${TOKEN}`, 'content-type': 'application/json', ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  return { status: res.status, body: text ? JSON.parse(text) : null };
};
const api = (m, p, b, h) => call(m, `/api/v10${p}`, b, h);
const control = (p, b) => call('POST', `/__fake/structure${p}`, b);

before(async () => {
  child = spawn(process.execPath, ['e2e/support/fake-discord.mjs'], {
    env: { ...process.env, SR_FAKE_DISCORD_PORT: String(PORT) },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  await new Promise((resolve) => child.stdout.once('data', resolve));
});
after(() => child.kill());

test('roles carry the audit reason and a Discord admin edit does not', async () => {
  await call('POST', '/__fake/reset');
  const r = await api(
    'POST',
    `/guilds/${GUILD}/roles`,
    { name: 'Mods', permissions: '8192' },
    { 'x-audit-log-reason': 'sr-bridge:7' },
  );
  assert.equal(r.status, 200);
  assert.equal(r.body.position, 1);
  await control('/admin', {
    method: 'POST',
    path: `/guilds/${GUILD}/roles`,
    body: { name: 'ByHand' },
  });
  const st = (await call('GET', '/__fake/structure')).body;
  assert.deepEqual(
    st.audit.map((a) => a.reason),
    ['sr-bridge:7', null],
  );
  assert.equal(st.roles.length, 4);
});

test('community-only types need COMMUNITY, forum tags get ids, threads open through the webhook', async () => {
  await call('POST', '/__fake/reset');
  const refused = await api('POST', `/guilds/${GUILD}/channels`, { name: 'ideas', type: 15 });
  assert.equal(refused.status, 400);
  await control('/control', { community: true });
  const forum = await api('POST', `/guilds/${GUILD}/channels`, {
    name: 'ideas',
    type: 15,
    available_tags: [{ name: 'bug' }],
  });
  assert.equal(forum.status, 200);
  assert.ok(forum.body.available_tags[0].id);
  const wh = await api('POST', `/channels/${forum.body.id}/webhooks`, {});
  const post = await call('POST', `/api/v10/webhooks/${wh.body.id}/${wh.body.token}?wait=true`, {
    content: 'hi',
    thread_name: 'A post',
  });
  assert.equal(post.status, 200);
  const thread = await api('GET', `/channels/${post.body.channel_id}`);
  assert.equal(thread.body.parent_id, forum.body.id);
  const active = await api('GET', `/guilds/${GUILD}/threads/active`);
  assert.equal(active.body.threads.length, 1);
});

test('overwrites, positions, the rename limit and the 401', async () => {
  await call('POST', '/__fake/reset');
  const ch = await api('POST', `/guilds/${GUILD}/channels`, {
    name: 'a',
    type: 0,
    permission_overwrites: [{ id: GUILD, type: 0, allow: '0', deny: String(1n << 47n) }],
  });
  assert.equal(ch.body.permission_overwrites[0].deny, String(1n << 47n));
  assert.equal(
    (await api('PUT', `/channels/${ch.body.id}/permissions/5`, { type: 0, allow: '1', deny: '2' }))
      .status,
    204,
  );
  assert.equal(
    (
      await api('PATCH', `/guilds/${GUILD}/channels`, [
        { id: ch.body.id, position: 4, parent_id: null },
      ])
    ).status,
    204,
  );
  assert.equal((await api('PATCH', `/channels/${ch.body.id}`, { name: 'b' })).status, 200);
  assert.equal((await api('PATCH', `/channels/${ch.body.id}`, { name: 'c' })).status, 200);
  assert.equal((await api('PATCH', `/channels/${ch.body.id}`, { name: 'd' })).status, 429);
  await control('/control', { reset_rename_window: true });
  assert.equal((await api('PATCH', `/channels/${ch.body.id}`, { name: 'd' })).status, 200);
  const anon = await call('GET', `/api/v10/guilds/${GUILD}/roles`, undefined, {
    authorization: 'Bot nope',
  });
  assert.equal(anon.status, 401);
  const above = await control('/seed', { roles: [{ id: '777', name: 'Above', position: 5 }] });
  assert.equal(above.status, 200);
  assert.equal((await api('PATCH', `/guilds/${GUILD}/roles/777`, { name: 'x' })).status, 403);
});
