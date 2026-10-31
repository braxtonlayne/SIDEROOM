// Entry point. Inside SIDE ROOM the Embedded App SDK supplies the shared state; with `?loopback` (solo host)
// or `?loopback=2` (a host and a guest side by side) the page runs on an in-memory room, to work on the UI
// and to try the table without a call.

import { startApp, h } from './app.js';
import { t } from './strings.js';
import { handleInput } from './table.js';
import { createLoopbackRoom, identity, runHost, sdkTransport } from './transport.js';

const params = new URLSearchParams(location.search);
const page = document.getElementById('app');

function fail(message) {
  page.replaceChildren(h('p', { class: 'fatal', role: 'alert', dataset: { testid: 'fatal' } }, message));
}

async function loopback(count) {
  const room = createLoopbackRoom('host');
  const host = room.join({ id: 'host', name: 'Host' });
  const hooks = {};
  runHost(host, handleInput, { hooks });
  const mounts = [[host, hooks]];
  if (count >= 2) mounts.push([room.join({ id: 'guest', name: 'Guest' }), null]);
  page.replaceChildren(...mounts.map(() => h('div', { class: 'seat' })));
  mounts.forEach(([tr, hk], i) => startApp(page.children[i], tr, { hooks: hk ?? undefined }));
  page.classList.add('multi');
}

async function embedded() {
  // The SDK modules are vendored beside this file by build.mjs; the variable keeps dev servers from resolving it.
  const sdkUrl = './sdk/index.js';
  const { SideRoomSDK } = await import(/* @vite-ignore */ sdkUrl);
  const appId = location.hostname.split('.')[0];
  const sdk = new SideRoomSDK(appId, { timeoutMs: 90_000 });
  page.replaceChildren(h('p', { class: 'loading' }, t('loading')));
  try {
    await sdk.ready();
  } catch (e) {
    return fail(e?.code === 'NO_HOST' ? t('noHost') : String(e?.message ?? e));
  }
  // The data channel hands the SDK who we are and who hosts; give it a moment to attach.
  let who = identity(sdk);
  for (let i = 0; i < 50 && (!who.self || !who.host); i++) {
    await new Promise((r) => setTimeout(r, 100));
    who = identity(sdk);
  }
  if (!who.self || !who.host) return fail(t('noHost'));

  let people = [];
  const toPerson = (p) => ({ id: p.id, name: p.global_name || p.username || p.id, avatarUrl: p.avatar_url ?? null });
  try {
    people = (await sdk.commands.getInstanceConnectedParticipants()).participants.map(toPerson);
    sdk.subscribe('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE', (d) => {
      people = (d?.participants ?? []).map(toPerson);
    });
  } catch {
    /* names fall back to ids */
  }
  const me = people.find((p) => p.id === who.self) ?? { id: who.self, name: who.self, avatarUrl: null };
  const transport = sdkTransport(sdk, { me, hostId: who.host, people: () => people });
  const hooks = {};
  if (transport.isHost) runHost(transport, handleInput, { hooks });
  page.replaceChildren(h('div', { class: 'seat' }));
  startApp(page.firstChild, transport, { hooks });
}

if (params.has('loopback')) {
  loopback(Number(params.get('loopback')) >= 2 ? 2 : 1);
} else {
  embedded().catch((e) => fail(String(e?.message ?? e)));
}
