import assert from 'node:assert/strict';
import { test } from 'node:test';
import { verdict } from '../voice-bot-live.mjs';

const good = [
  'joining ws://127.0.0.1:8080/voice/ws (e2ee: true)',
  'encrypting under epoch 1',
  'connected',
  'dropping the SFU WebSocket',
  'resumed (0 in the channel)',
  'done after 60.0s: connected=true resumed=true encrypted_epochs=1 frames=900',
].join('\n');

test('a run that joined, dropped its socket and resumed passes', () => {
  assert.deepEqual(verdict(good, 0), []);
});

test('every missing step is named', () => {
  assert.match(verdict(good.replace('connected\n', ''), 0).join(), /never came up/);
  assert.match(verdict(good.replace(/resumed \(.*\n/, ''), 0).join(), /not resumed/);
  assert.match(verdict(`${good}\nclosed (4014)`, 0).join(), /closed under/);
  assert.match(verdict(good, 1).join(), /exited with 1/);
  assert.match(
    verdict(good.replace('encrypting under epoch 1\n', ''), 0).join(),
    /no encryption epoch/,
  );
  assert.match(verdict(good.replace('frames=900', 'frames=0'), 0).join(), /no frame was sent/);
  assert.match(verdict('', 0).join(), /VOICE_SERVER_UPDATE/);
});

test('the partner must have decoded a tone from the other bot, not silence (DEV-FU-002)', () => {
  const heard = (line) => verdict(`${good}\n${line}`, 0, { heardFrom: '42' });
  assert.deepEqual(heard('42 spoke 57 bytes (-12 dBov)'), []);
  assert.match(heard('42 spoke 57 bytes (-127 dBov)').join(), /never decrypted a frame from 42/);
  assert.match(heard('42 spoke 60 bytes').join(), /never decrypted a frame from 42/);
  assert.match(heard('43 spoke 57 bytes (-12 dBov)').join(), /never decrypted a frame from 42/);
});
