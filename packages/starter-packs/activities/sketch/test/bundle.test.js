// @ts-check
// The bundle (tools/bundle.mjs): it passes every rule of crates/sr-activities/src/bundle.rs and the Activity CSP,
// declares what the game needs, and its game code names no app route (D-385, D-396, D-397).
import assert from 'node:assert/strict';
import { ACTIVITY_MANIFEST, ROOT, checkBundle, collectFiles } from '../tools/bundle.mjs';

const files = collectFiles(ROOT);
const byPath = new Map(files.map((f) => [f.path, f.data.toString('utf8')]));

test('the bundle passes the server rules and the CSP checks', () => {
  assert.deepEqual(checkBundle(files), []);
  for (const p of ['index.html', 'main.js', 'style.css', 'host/engine.js', 'shared/rules.js', 'sdk/index.js', 'sideroom.json']) {
    assert.ok(byPath.has(p), p);
  }
});

test('the manifest declares the capabilities the game uses and nothing that widens its CSP', () => {
  assert.equal(ACTIVITY_MANIFEST.id, 'side-room-sketch');
  assert.deepEqual([...ACTIVITY_MANIFEST.capabilities].sort(), ['byos_read', 'datachannel', 'guest_play', 'host_handoff', 'lobby_channel', 'storage']);
  assert.equal(ACTIVITY_MANIFEST.connect, undefined, 'no connect list: the pack bytes come through the bridge (D-460)');
  assert.ok(!ACTIVITY_MANIFEST.capabilities.includes('ingame_voice'));
});

test('the vendored SDK carries shelfObjectBytes', () => {
  assert.match(byPath.get('sdk/sdk.js') ?? '', /shelfObjectBytes/);
});

test('game code names no app route, and member-only commands only on the member path of the platform', () => {
  const routes = /\/api\/|\/channels\/|\/users\/|\/shelves\/|\/scrapbook|\/profile|\/messages|\/guilds\//i;
  const memberOnly = /\bcommands\.(?:getInstanceConnectedParticipants|setActivity|shelfObjectBytes|shelfObjectUrl|authorize|authenticate|getChannel)\b/;
  for (const [path, text] of byPath) {
    if (path.startsWith('sdk/') || !path.endsWith('.js')) continue;
    assert.ok(!routes.test(text), `${path} names an app route`);
    if (path !== 'platform/sideroom-platform.js' && path !== 'main.js') assert.ok(!memberOnly.test(text.replace(/\/\/.*$/gm, '')), `${path} calls a member-only command`);
  }
  // main.js only reaches packs through the platform, which refuses guests.
  assert.ok(!/sdk\.commands/.test(byPath.get('main.js') ?? ''));
});
