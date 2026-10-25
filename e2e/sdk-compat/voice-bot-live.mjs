// RDY-DEV-002: the sr-sdk voice bot (crates/sr-sdk/examples/voice_bot.rs) against a live sr-server with the SFU on,
// for one minute. The driver makes a server with a voice channel, installs a bot with CONNECT and SPEAK, runs
// the example with SR_RUN_SECONDS=60 and SR_DROP_AFTER_SECONDS=20 (the bot drops its own SFU WebSocket once and
// must resume) and checks what it printed: the call was joined, audio was connected, the socket resumed, and
// the process exited 0.
//
//   SDK_COMPAT_BASE=http://127.0.0.1:8080 SDK_COMPAT_OWNER=owner SDK_COMPAT_PASSWORD=... \
//     node e2e/sdk-compat/voice-bot-live.mjs [--seconds 60]
//
// SR_VOICE_BOT_BIN names a built example binary (default: cargo run -p sr-sdk --example voice_bot).

import { spawn } from 'node:child_process';
import { provision, waitReady } from './lib/provision.mjs';

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : fallback;
};
const base = process.env.SDK_COMPAT_BASE ?? 'http://127.0.0.1:8080';
const owner = process.env.SDK_COMPAT_OWNER ?? 'owner';
const password = process.env.SDK_COMPAT_PASSWORD ?? 'e2e-owner-Passw0rd!';
const seconds = Number(arg('seconds', '60'));
const dropAfter = Math.max(5, Math.floor(seconds / 3));

/** Pure: what the example's output must contain for the run to count. */
export function verdict(output, code) {
  const problems = [];
  if (code !== 0) problems.push(`the bot exited with ${code}`);
  if (!/^joining ws/m.test(output)) problems.push('it never got a VOICE_SERVER_UPDATE');
  if (!/^connected$/m.test(output)) problems.push('the media connection never came up');
  if (!/^dropping the SFU WebSocket$/m.test(output)) problems.push('it did not drop its socket');
  if (!/^resumed \(/m.test(output)) problems.push('the dropped socket was not resumed');
  if (/^closed \(/m.test(output)) problems.push('the call was closed under the bot');
  // An encrypted call (the default, D-249) must have been joined with the MLS handshake and sent under a key.
  if (/^joining .*\(e2ee: true\)$/m.test(output)) {
    if (!/^encrypting under epoch \d+$/m.test(output)) {
      problems.push('no encryption epoch was executed');
    }
    if (!/encrypted_epochs=[1-9]/.test(output) || !/frames=[1-9]/.test(output)) {
      problems.push('no frame was sent under an epoch key');
    }
  }
  return problems;
}

async function main() {
  await waitReady(base);
  // VIEW_CHANNEL, CONNECT, SPEAK.
  const permissions = String((1n << 10n) | (1n << 20n) | (1n << 21n));
  const run = await provision({ base, owner, password, permissions });
  const voice = await run.person.post(`/guilds/${run.guildId}/channels`, {
    name: 'voice-bot',
    type: 2,
  });

  const env = {
    ...process.env,
    SR_BOT_TOKEN: run.token,
    SR_API_URL: `${base}/api/v1`,
    SR_GATEWAY_URL: base.replace(/^http/, 'ws') + '/gateway',
    SR_GUILD_ID: run.guildId,
    SR_VOICE_CHANNEL_ID: voice.id,
    SR_BIND_IP: '127.0.0.1',
    SR_RUN_SECONDS: String(seconds),
    SR_DROP_AFTER_SECONDS: String(dropAfter),
  };
  const [cmd, args] = process.env.SR_VOICE_BOT_BIN
    ? [process.env.SR_VOICE_BOT_BIN, []]
    : ['cargo', ['run', '-q', '-j', '2', '-p', 'sr-sdk', '--example', 'voice_bot']];
  console.log(`running the voice bot for ${seconds} s (socket drop at ${dropAfter} s)`);
  const child = spawn(cmd, args, { env, stdio: ['ignore', 'pipe', 'inherit'] });
  let output = '';
  child.stdout.on('data', (d) => {
    output += d;
    process.stdout.write(d);
  });
  const killer = setTimeout(() => child.kill(), (seconds + 300) * 1000);
  const code = await new Promise((resolve) => child.on('exit', (c) => resolve(c ?? -1)));
  clearTimeout(killer);

  const problems = verdict(output, code);
  if (problems.length) {
    console.error(`voice bot run FAILED:\n- ${problems.join('\n- ')}`);
    process.exit(1);
  }
  console.log('voice bot run ok');
}

if (process.argv[1]?.endsWith('voice-bot-live.mjs')) {
  await main();
}
