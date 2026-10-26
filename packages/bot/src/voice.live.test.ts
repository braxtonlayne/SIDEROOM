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
// optionally SR_RUN_SECONDS (60), SR_DROP_AFTER_SECONDS (20), SR_PARTNER_USER and SR_FRESH_SESSION_AFTER_SECONDS.
//
// The bot speaks a 440 Hz tone through `OpusEncoder` (DEV-FU-002) and decodes the partner's frames, which must
// be a tone too, not silence. With SR_FRESH_SESSION_AFTER_SECONDS the gateway session is thrown away that many
// seconds in (`Shard.reidentify`): the new session's IDENTIFY and READY carry no voice state, so the call is only
// joined again if the shard sends its remembered op 4 after READY (DEV-FU-001); the test then needs a second
// VOICE_SERVER_UPDATE, rejoins the SFU with it and does the MLS handshake again.
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadE2ee } from '@sideroom/wasm';
import { Client, Intents } from './index';
import { FRAME_SAMPLES, OpusDecoder, OpusEncoder, audioLevelDbov } from './audio';
import type { Shard } from './gateway';
import { E2eeCall, E2eeMedia, Identity, type Action } from './voice';
import { VoiceLink, VoiceServerUpdate, voiceStateBody, type VoiceEvent } from './voiceTransport';

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
const freshAfter = process.env.SR_FRESH_SESSION_AFTER_SECONDS
  ? Number(process.env.SR_FRESH_SESSION_AFTER_SECONDS)
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
      const laterUpdates: VoiceServerUpdate[] = [];
      let readies = 0;
      const mls: Record<string, unknown>[] = [];
      const gotReady = new Promise<void>((resolve) => {
        client.on('raw', (name, data) => {
          if (name === 'READY') {
            me = String((data as { user: { id: string } }).user.id);
            readies += 1;
            resolve();
          } else if (name === 'VOICE_SERVER_UPDATE') {
            const u = VoiceServerUpdate.fromDispatch(data as Record<string, unknown>);
            if (update === null) update = u;
            else if (u) laterUpdates.push(u);
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

      expect(
        client.voiceStateUpdate(guild, voiceStateBody(guild, channel, false, false, true)),
      ).toBe(true);
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
      // 20 ms of the tone per frame; the phase continues across frames.
      const encoder = link.session().adopt(new OpusEncoder(1, 'voip'));
      const decoder = link.session().adopt(new OpusDecoder(1));
      let clock = 0;
      const toneFrame = (): Int16Array => {
        const pcm = new Int16Array(FRAME_SAMPLES);
        for (let i = 0; i < FRAME_SAMPLES; i++) {
          pcm[i] = Math.round(
            Math.sin((2 * Math.PI * 440 * (clock * FRAME_SAMPLES + i)) / 48000) * 8000,
          );
        }
        clock += 1;
        return pcm;
      };
      let freshDone = false;
      let rejoins = 0;
      let bestHeard = -127;

      while (Date.now() - started < seconds * 1000 && closed === null) {
        for (const d of mls.splice(0)) apply(call.handle(d, roster, Date.now()));
        const due = call.deadline();
        if (due !== null && Date.now() >= due) apply(call.poll(roster, Date.now()));
        if (!dropped && dropAfter !== null && Date.now() - started >= dropAfter * 1000) {
          console.log('dropping the SFU WebSocket');
          link.session().abandonSignalling();
          dropped = true;
        }
        if (!freshDone && freshAfter !== null && Date.now() - started >= freshAfter * 1000) {
          console.log('throwing the gateway session away (a fresh IDENTIFY follows)');
          const shards = (client as unknown as { shards: Shard[] }).shards;
          shards[0]?.reidentify();
          freshDone = true;
        }
        // The shard sent its remembered join after the fresh READY, so the server says where the call is again.
        for (const u of laterUpdates.splice(0)) {
          console.log(`rejoining ${u.endpoint}`);
          rejoins += 1;
          closed = null;
          await link.rejoin(u);
          apply([call.hello()]);
        }
        if (link.session().isConnected() && media.sendingEpoch() >= 0 && Date.now() >= nextFrame) {
          nextFrame += 20;
          const pcm = toneFrame();
          await link.session().sendOpus(encoder.encodeFrame(pcm));
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
              if (e.user === partner) {
                // The partner's encoded tone, opened with our keys and decoded: sound, not silence.
                const level = audioLevelDbov(decoder.decodeFrame(e.frame));
                bestHeard = Math.max(bestHeard, level);
                if (level > -40) heard += 1;
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
              // After a thrown-away session the server removes the old media leg; the rejoin follows.
              if (freshDone && rejoins === 0) {
                console.log(`old media connection closed (${e.code}), waiting for the new one`);
                break;
              }
              closed = e.code;
              console.log(`closed (${e.code})`);
              break;
          }
        }
        if (changed) apply(call.rosterChanged(roster, Date.now()));
      }
      console.log(
        `done: connected=${connected} resumed=${resumed} encrypted_epochs=${epochs.length} frames=${frames} heard=${heard} loudest=${bestHeard}dBov undecryptable=${undecryptable} readies=${readies} rejoins=${rejoins}`,
      );
      await link.session().leave();
      client.voiceStateUpdate(guild, voiceStateBody(guild, null, false, false, true));
      client.destroy();

      expect(closed, 'the call was not closed under the bot').toBeNull();
      expect(connected, 'the media connection came up').toBe(true);
      expect(epochs.length, 'an epoch key was executed').toBeGreaterThan(0);
      expect(frames, 'frames went out under the epoch key').toBeGreaterThan(0);
      if (dropAfter !== null) expect(resumed, 'the dropped socket was resumed').toBe(true);
      if (freshAfter !== null) {
        expect(readies, 'a second, fresh READY arrived').toBeGreaterThan(1);
        expect(rejoins, 'the shard joined the call again after the fresh READY').toBeGreaterThan(0);
      }
      if (partner !== null) {
        expect(heard, "the partner's encrypted tone was decrypted and decoded").toBeGreaterThan(20);
      }
    },
    (seconds + 90) * 1000,
  );
});
