import assert from 'node:assert/strict';
import { test } from 'node:test';
import { verdict } from '../voice-bot-live.mjs';

const good = [
  'joining ws://127.0.0.1:8080/voice/ws (e2ee: true)',
  'connected',
  'dropping the SFU WebSocket',
  'resumed (0 in the channel)',
  'done after 60.0s: connected=true resumed=true',
].join('\n');

test('a run that joined, dropped its socket and resumed passes', () => {
  assert.deepEqual(verdict(good, 0), []);
});

test('every missing step is named', () => {
  assert.match(verdict(good.replace('connected\n', ''), 0).join(), /never came up/);
  assert.match(verdict(good.replace(/resumed \(.*\n/, ''), 0).join(), /not resumed/);
  assert.match(verdict(`${good}\nclosed (4014)`, 0).join(), /closed under/);
  assert.match(verdict(good, 1).join(), /exited with 1/);
  assert.match(verdict('', 0).join(), /VOICE_SERVER_UPDATE/);
});
