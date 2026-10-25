// RDY-DEV-001: the TypeScript voice bot against a live sr-server and its SFU, in an end-to-end encrypted call
// (voice E2EE stays on, D-249). Runs only with SR_VOICE_LIVE=1; the `live` job of e2e.yml starts it through
// e2e/sdk-compat/voice-bot-live.mjs --ts, which makes the server, the channel and the bot and, as a partner in the
// same call, the Rust bot of crates/sr-sdk/examples/voice_bot.rs. The test:
//
//  - joins the call with op 4 and answers the MLS handshake (hello, key package, welcome, commit, ready) over
//    op 101 with `E2eeCall`, so the bot is a real member of the call's MLS group (D-263);
//  - sends 20 ms frames under the epoch key once the server executes it;
//  - drops its own SFU WebSocket once and must be resumed by `VoiceLink`;
//  - with SR_PARTNER_USER, must hear that user's frames decrypted (plaintext 60 bytes of 0x5a): the Rust
//    implementation's encrypted audio, opened by this implementation's keys.
//
// Environment: SR_BOT_TOKEN, SR_API_URL, SR_GATEWAY_URL, SR_GUILD_ID, SR_VOICE_CHANNEL_ID, SR_BIND_IP, and
// optionally SR_RUN_SECONDS (60), SR_DROP_AFTER_SECONDS (20), SR_PARTNER_USER.
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadE2ee } from '@sideroom/wasm';
import { Client, Intents } from './index';
import { E2eeCall, E2eeMedia, Identity, type Action } from './voice';
import {
  VoiceLink,
  VoiceServerUpdate,
  voiceStateBody,
  type VoiceEvent,
} from './voiceTransport';

const live = process.env.SR_VOICE_LIVE === '1';
const env = (k: string): string => {
  const v = process.env[k];
  if (!v) throw new Error(`set ${k}`);
  return v;
};
const here = dirname(fileURLToPath(import.meta.url));
const seconds = Number(process.env.SR_RUN_SECONDS ?? '60');
const dropAfter = process.env.SR_DROP_AFTER_SECONDS
  ? Number(process.env.SR_DROP_AFTER_SECONDS)
  : null;

describe.runIf(live)('voice bot against a live server and SFU (RDY-DEV-001)', () => {
  it(
    'joins an encrypted call, takes part in its MLS group, resumes after a drop and hears the partner',
    async () => {
      const guild = env('SR_GUILD_ID');
      const channel = env('SR_VOICE_CHANNEL_ID');
      const bindIp = env('SR_BIND_IP');
      const partner = process.env.SR_PARTNER_USER ?? null;
      const api = await loadE2ee(readFileSync(join(here, '../../wasm/dist/e2ee/e2ee_bg.wasm')));

      const client = new Client({
        token: env('SR_BOT_TOKEN'),
        apiUrl: env('SR_API_URL'),
        gatewayUrl: env('SR_GATEWAY_URL'),
        intents: Intents.Guilds,
        identifyGapMs: 0,
      });
      let me = '';
      let update: VoiceServerUpdate | null = null;
      const mls: Record<string, unknown>[] = [];
      const gotReady = new Promise<void>((resolve) => {
        client.on('raw', (name, data) => {
          if (name === 'READY') {
            me = String((data as { user: { id: string } }).user.id);
            resolve();
          } else if (name === 'VOICE_SERVER_UPDATE') {
            update = VoiceServerUpdate.fromDispatch(data as Record<string, unknown>);
          } else if (name === 'E2EE_MLS') {
            mls.push(data as Record<string, unknown>);
          }
        });
      });
      await client.login();
      await gotReady;

      const call = E2eeCall.create(api, me, channel, Identity.generate());
      const media = new E2eeMedia(api, me);
      const epochs: number[] = [];
      let roster = new Set<string>([me]);
      const apply = (actions: Action[]) => {
        for (const a of actions) {
          if (a.kind === 'send') client.e2eeMls(guild, a.d);
          else if (a.kind === 'keys') media.prepare(a.keys);
          else if (a.kind === 'execute') {
            if (media.execute(a.epoch)) {
              epochs.push(a.epoch);
              console.log(`encrypting under epoch ${a.epoch}`);
            }
          } else if (a.kind === 'failed') console.log(`key agreement failed: ${a.reason}`);
        }
      };

      expect(client.voiceStateUpdate(guild, voiceStateBody(guild, channel, false, false, true))).toBe(
        true,
      );
      const waitFor = async <T>(f: () => T | null, ms: number, what: string): Promise<T> => {
        const end = Date.now() + ms;
        for (;;) {
          const v = f();
          if (v !== null) return v;
          if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
          await new Promise((r) => setTimeout(r, 20));
        }
      };
      const first = await waitFor(() => update, 15_000, 'VOICE_SERVER_UPDATE');
      expect(first.e2ee, 'voice E2EE is on by default').toBe(true);
      // Within ten seconds of VOICE_SERVER_UPDATE or the server removes the bot (D-263).
      apply([call.hello()]);
      console.log(`joining ${first.endpoint} (e2ee: ${first.e2ee})`);
      const link = await VoiceLink.connect(first, { bindIp });
      link.session().setTransform(media);
      for (const u of link.session().initialParticipants) roster.add(u);

      const started = Date.now();
      let connected = false;
      let resumed = false;
      let dropped = false;
      let frames = 0;
      let heard = 0;
      let undecryptable = 0;
      let closed: number | null = null;
      let nextFrame = Date.now();
      const tone = new Uint8Array(60).fill(0x5a);

      while (Date.now() - started < seconds * 1000 && closed === null) {
        for (const d of mls.splice(0)) apply(call.handle(d, roster, Date.now()));
        const due = call.deadline();
        if (due !== null && Date.now() >= due) apply(call.poll(roster, Date.now()));
        if (!dropped && dropAfter !== null && Date.now() - started >= dropAfter * 1000) {
          console.log('dropping the SFU WebSocket');
          link.session().abandonSignalling();
          dropped = true;
        }
        if (link.session().isConnected() && media.sendingEpoch() >= 0 && Date.now() >= nextFrame) {
          nextFrame += 20;
          await link.session().sendOpus(tone);
          frames += 1;
        }
        const events: VoiceEvent[] = await link.pump(20);
        let changed = false;
        for (const e of events) {
          switch (e.kind) {
            case 'connected':
              connected = true;
              console.log('connected');
              break;
            case 'audio':
              if (e.user === partner && e.frame.length === 60 && e.frame.every((b) => b === 0x5a)) {
                heard += 1;
              }
              break;
            case 'undecryptable':
              if (e.user === partner) undecryptable += 1;
              break;
            case 'joined':
              roster.add(e.user);
              changed = true;
              break;
            case 'left':
              changed = roster.delete(e.user) || changed;
              break;
            case 'resumed':
              resumed = true;
              console.log(`resumed (${e.participants.length} in the channel)`);
              roster = new Set([...e.participants, me]);
              changed = true;
              break;
            case 'reconnected':
              console.log(`joined again (${e.participants.length} in the channel)`);
              roster = new Set([...e.participants, me]);
              changed = true;
              break;
            case 'closed':
              closed = e.code;
              console.log(`closed (${e.code})`);
              break;
          }
        }
        if (changed) apply(call.rosterChanged(roster, Date.now()));
      }
      console.log(
        `done: connected=${connected} resumed=${resumed} encrypted_epochs=${epochs.length} frames=${frames} heard=${heard} undecryptable=${undecryptable}`,
      );
      await link.session().leave();
      client.voiceStateUpdate(guild, voiceStateBody(guild, null, false, false, true));
      client.destroy();

      expect(closed, 'the call was not closed under the bot').toBeNull();
      expect(connected, 'the media connection came up').toBe(true);
      expect(epochs.length, 'an epoch key was executed').toBeGreaterThan(0);
      expect(frames, 'frames went out under the epoch key').toBeGreaterThan(0);
      if (dropAfter !== null) expect(resumed, 'the dropped socket was resumed').toBe(true);
      if (partner !== null) {
        expect(heard, "the partner's encrypted audio was decrypted").toBeGreaterThan(20);
      }
    },
    (seconds + 90) * 1000,
  );
});
