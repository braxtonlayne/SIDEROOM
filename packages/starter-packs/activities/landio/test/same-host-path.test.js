// @ts-check
// OPS-TDEP-001: a test deployment serves the Activity from /_activity/<application id>/ on the main host; the
// platform finds its application id there as well as on <id>.activities.<host>.
import assert from 'node:assert/strict';
import { applicationIdOf, isSideRoom } from '../client/platform/sideroom-platform.js';

const at = (/** @type {string} */ hostname, /** @type {string} */ pathname, framed = true) => {
  /** @type {any} */
  const win = { location: { hostname, pathname } };
  win.parent = framed ? {} : win;
  return /** @type {Window} */ (win);
};

test('the application id comes from the activity host or the same-host path', () => {
  assert.equal(applicationIdOf(at('123.activities.chat.example.org', '/')), '123');
  assert.equal(applicationIdOf(at('abc.trycloudflare.com', '/_activity/123/')), '123');
  assert.equal(applicationIdOf(at('abc.trycloudflare.com', '/_activity/77/index.html')), '77');
});

test('only a framed activity page counts as running in SIDE ROOM', () => {
  assert.equal(isSideRoom(at('123.activities.chat.example.org', '/')), true);
  assert.equal(isSideRoom(at('abc.trycloudflare.com', '/_activity/123/')), true);
  assert.equal(isSideRoom(at('abc.trycloudflare.com', '/_activity/123/', false)), false);
  assert.equal(isSideRoom(at('abc.trycloudflare.com', '/play/x')), false);
  assert.equal(isSideRoom(at('localhost', '/_activity/abc/')), false);
});
