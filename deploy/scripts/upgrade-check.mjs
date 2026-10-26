#!/usr/bin/env node
// The data half of the upgrade drill (RDY-OPS-007, .github/workflows/prod-smoke.yml job `upgrade`): writes a few
// things through the public HTTP API of the previous version, then proves they are still there after the upgrade
// and after the rollback. It uses only the API the web app uses, signed in as the Owner and as the seeded member
// `alice` (sr-cli seed), through Caddy and its internal CA.
//
//   node deploy/scripts/upgrade-check.mjs seed
//        on the previous version, after `sr-cli seed`: create a draft Cinema title as the Owner, post a marker
//        message in the seeded guild's #general as alice, and record what the Library holds. Writes the state file.
//   node deploy/scripts/upgrade-check.mjs verify [--write-marker] [--expect-marker-gone]
//        sign in as both, then check the state file's facts against the API: the member and her guild, the marker
//        message and the seeded history, the Library series, a chapter and its pages, the Cinema title. With
//        --write-marker it then posts a second message (written on the new version, after the backup); with
//        --expect-marker-gone it checks that second message is NOT there (the rollback restored the backup).
//
//   SMOKE_URL                  public URL, for example https://smoke.localhost
//   SMOKE_OWNER_USERNAME       default owner
//   SMOKE_OWNER_PASSWORD_FILE  file holding the Owner password
//   SMOKE_SEED_PASSWORD        password of the seeded accounts (sr-cli seed default)
//   SMOKE_UPGRADE_STATE        the state file (JSON), default $SMOKE_WORK/upgrade-state.json
//   NODE_EXTRA_CA_CERTS        the Caddy root certificate
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { parseArgs } from 'node:util';

const { positionals, values } = parseArgs({
  allowPositionals: true,
  options: {
    'write-marker': { type: 'boolean', default: false },
    'expect-marker-gone': { type: 'boolean', default: false },
  },
});
const mode = positionals[0];
if (!['seed', 'verify'].includes(mode ?? '')) {
  console.error('usage: upgrade-check.mjs seed | verify [--write-marker] [--expect-marker-gone]');
  process.exit(2);
}
const base = (process.env.SMOKE_URL ?? '').replace(/\/$/, '');
if (!base) throw new Error('set SMOKE_URL');
const statePath =
  process.env.SMOKE_UPGRADE_STATE ??
  `${process.env.SMOKE_WORK ?? process.env.RUNNER_TEMP ?? '/tmp'}/upgrade-state.json`;
const ownerName = process.env.SMOKE_OWNER_USERNAME ?? 'owner';
const ownerPassword = readFileSync(
  process.env.SMOKE_OWNER_PASSWORD_FILE ??
    (() => {
      throw new Error('set SMOKE_OWNER_PASSWORD_FILE');
    })(),
  'utf8',
).trim();
const seedPassword = process.env.SMOKE_SEED_PASSWORD ?? 'sideroom-dev-password';
const DEV_GUILD = 'Side Room Dev';

/** One signed-in client with its own cookie jar. `expect` lets a call accept a status other than 2xx. */
async function signIn(login, password) {
  const jar = new Map();
  const client = {
    async call(method, path, body, { expect = null } = {}) {
      const cookie = [...jar].map(([k, v]) => `${k}=${v}`).join('; ');
      const csrf = method === 'GET' || !client.csrf ? {} : { 'X-SR-CSRF': client.csrf };
      const res = await fetch(`${base}/api/v1${path}`, {
        method,
        headers: {
          Origin: base,
          ...(cookie ? { Cookie: cookie } : {}),
          ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
          ...csrf,
        },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      for (const line of res.headers.getSetCookie()) {
        const [pair] = line.split(';');
        const i = pair.indexOf('=');
        const name = pair.slice(0, i).trim();
        const value = pair.slice(i + 1).trim();
        if (value === '' || /;\s*max-age=0/i.test(line)) jar.delete(name);
        else jar.set(name, value);
      }
      const text = await res.text();
      if (expect !== null) return { status: res.status, text };
      if (!res.ok)
        throw new Error(`${login}: ${method} ${path} -> ${res.status}: ${text.slice(0, 300)}`);
      return text ? JSON.parse(text) : undefined;
    },
  };
  const out = await client.call('POST', '/sr/auth/login', { login, password });
  if (out?.status !== 'ok')
    throw new Error(`${login}: sign-in did not complete: ${JSON.stringify(out)}`);
  client.csrf = (await client.call('GET', '/sr/csrf')).token;
  if (!client.csrf) throw new Error(`${login}: no CSRF token`);
  console.log(`signed in as ${login}`);
  return client;
}

function must(cond, what) {
  if (!cond) throw new Error(`upgrade check failed: ${what}`);
  console.log(`  ok: ${what}`);
}

async function generalChannel(alice) {
  const guilds = await alice.call('GET', '/users/@me/guilds');
  const guild = guilds.find((g) => g.name === DEV_GUILD);
  must(guild, `alice is a member of "${DEV_GUILD}"`);
  const channels = await alice.call('GET', `/guilds/${guild.id}/channels`);
  const general = channels.find((c) => c.name === 'general');
  must(general, '#general is there');
  return { guildId: guild.id, channelId: general.id };
}

async function library(alice) {
  const page = await alice.call('GET', '/library/series?limit=50');
  must(page.items.length > 0, `the Library lists ${page.items.length} series`);
  // The first series with a chapter, so a series that has none cannot hide a lost chapter.
  for (const series of page.items) {
    const list = await alice.call('GET', `/library/series/${series.id}/chapters`);
    const chapter = list.chapters[0];
    if (!chapter) continue;
    const manifest = await alice.call('GET', `/library/chapters/${chapter.id}/pages`);
    return {
      seriesTotal: page.total,
      seriesId: series.id,
      seriesTitle: series.title,
      chapterId: chapter.id,
      pages: manifest.pages.length,
    };
  }
  throw new Error('upgrade check failed: no Library series has a chapter');
}

const messageIn = async (alice, channelId, id) =>
  alice.call('GET', `/channels/${channelId}/messages/${id}`, undefined, { expect: 'any' });

if (mode === 'seed') {
  const owner = await signIn(ownerName, ownerPassword);
  const alice = await signIn('alice', seedPassword);
  const title = await owner.call('POST', '/admin/cinema/titles', {
    kind: 'movie',
    title: `Upgrade Drill Film ${Date.now().toString(36)}`,
    year: 2001,
    synopsis:
      'Written on the previous version, read back after the upgrade and after the rollback.',
    genres: ['Drama'],
  });
  console.log(`created the draft Cinema title ${title.id}`);
  const { guildId, channelId } = await generalChannel(alice);
  const marker = await alice.call('POST', `/channels/${channelId}/messages`, {
    content: `upgrade-drill marker, written before the backup (${Date.now()})`,
  });
  const lib = await library(alice);
  const history = await alice.call('GET', `/channels/${channelId}/messages?limit=100`);
  must(
    history.length >= 100,
    `the seeded history has ${history.length} messages in the first page`,
  );
  const state = {
    titleId: title.id,
    titleName: title.title,
    guildId,
    channelId,
    markerId: marker.id,
    markerText: marker.content,
    library: lib,
  };
  writeFileSync(statePath, `${JSON.stringify(state, null, 2)}\n`);
  console.log(`state written to ${statePath}`);
} else {
  if (!existsSync(statePath)) throw new Error(`no state file ${statePath}: run "seed" first`);
  const s = JSON.parse(readFileSync(statePath, 'utf8'));
  const owner = await signIn(ownerName, ownerPassword);
  const alice = await signIn('alice', seedPassword);

  const me = await alice.call('GET', '/users/@me');
  must(me.username === 'alice', 'the seeded member alice signs in and is herself');
  const where = await generalChannel(alice);
  must(where.channelId === s.channelId, '#general kept its id');

  const marker = await alice.call('GET', `/channels/${s.channelId}/messages/${s.markerId}`);
  must(marker.content === s.markerText, 'the marker message is intact');
  const history = await alice.call('GET', `/channels/${s.channelId}/messages?limit=100`);
  must(
    history.length >= 100,
    `the seeded history is there (${history.length} messages in the first page)`,
  );

  const lib = await library(alice);
  must(
    lib.seriesTotal === s.library.seriesTotal,
    `the Library still holds ${s.library.seriesTotal} series`,
  );
  const chapters = await alice.call('GET', `/library/series/${s.library.seriesId}/chapters`);
  must(
    chapters.chapters.some((c) => c.id === s.library.chapterId),
    `the Library chapter ${s.library.chapterId} is listed`,
  );
  const manifest = await alice.call('GET', `/library/chapters/${s.library.chapterId}/pages`);
  must(manifest.pages.length === s.library.pages, `the chapter still has ${s.library.pages} pages`);

  const title = await owner.call('GET', `/admin/cinema/titles/${s.titleId}`);
  must(title.title.title === s.titleName, `the Cinema title "${s.titleName}" is there`);

  if (values['write-marker']) {
    const after = await alice.call('POST', `/channels/${s.channelId}/messages`, {
      content: `upgrade-drill marker, written after the upgrade (${Date.now()})`,
    });
    s.afterId = after.id;
    writeFileSync(statePath, `${JSON.stringify(s, null, 2)}\n`);
    console.log(`wrote the post-upgrade marker ${after.id}`);
  }
  if (values['expect-marker-gone']) {
    if (!s.afterId)
      throw new Error('no post-upgrade marker in the state file: run verify --write-marker first');
    const gone = await messageIn(alice, s.channelId, s.afterId);
    must(
      gone.status === 404,
      `the message written after the backup is gone after the rollback (status ${gone.status})`,
    );
  }
}
console.log('upgrade-check: ok');
