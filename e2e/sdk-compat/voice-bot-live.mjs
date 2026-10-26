// RDY-DEV-002 / RDY-DEV-001: the first-party voice bots against a live sr-server with the SFU on, for one minute,
// in an end-to-end encrypted call (voice E2EE stays on, D-249).
//
//   node e2e/sdk-compat/voice-bot-live.mjs [--seconds 60]        the Rust bot (crates/sr-sdk/examples/voice_bot.rs)
//   node e2e/sdk-compat/voice-bot-live.mjs --ts [--seconds 60]    the TypeScript bot (packages/bot, werift) with
//                                                                 the Rust bot as a partner in the same call
//
// The driver makes a server with a voice channel and installs a bot with CONNECT and SPEAK.
//
// Rust mode: the example runs with SR_RUN_SECONDS and SR_DROP_AFTER_SECONDS (it drops its own SFU WebSocket once
// and must resume); the check is on what it printed: joined, the MLS handshake ran and frames went out under an
// epoch key, audio connected, the socket resumed, exit 0.
//
// TypeScript mode: the Rust bot is started first and waits in the call; then packages/bot/src/voice.live.test.ts
// runs as a vitest test (SR_VOICE_LIVE=1). It must join the MLS group of the call, send under the epoch key,
// resume after its own drop and decrypt the Rust bot's frames; and the Rust bot must have decrypted the
// TypeScript bot's frames (a line "<id> spoke <n> bytes (<level> dBov)" with a level above -40: a tone, not silence): each implementation opens the other's audio.
//
//   SDK_COMPAT_BASE=http://127.0.0.1:8080 SDK_COMPAT_OWNER=owner SDK_COMPAT_PASSWORD=... \
//     node e2e/sdk-compat/voice-bot-live.mjs [--ts] [--seconds 60]
//
// SR_VOICE_BOT_BIN names a built example binary (default: cargo run -p sr-sdk --example voice_bot); SR_PNPM names
// the pnpm launcher for the TypeScript run (default: pnpm).

import { spawn } from 'node:child_process';
import { installBot, provision, waitReady } from './lib/provision.mjs';

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : fallback;
};
const base = process.env.SDK_COMPAT_BASE ?? 'http://127.0.0.1:8080';
const owner = process.env.SDK_COMPAT_OWNER ?? 'owner';
const password = process.env.SDK_COMPAT_PASSWORD ?? 'e2e-owner-Passw0rd!';
const seconds = Number(arg('seconds', '60'));
const dropAfter = Math.max(5, Math.floor(seconds / 3));
const typescript = process.argv.includes('--ts');

/**
 * Pure: what the Rust example's output must contain for the run to count.
 * @param {string} output
 * @param {number} code
 * @param {{ drop?: boolean, heardFrom?: string | null }} [opts] `drop`: the bot dropped its socket and must have
 *   resumed (default true); `heardFrom`: a user id whose encoded tone the bot must have decrypted and decoded to sound.
 */
export function verdict(output, code, { drop = true, heardFrom = null } = {}) {
  const problems = [];
  if (code !== 0) problems.push(`the bot exited with ${code}`);
  if (!/^joining ws/m.test(output)) problems.push('it never got a VOICE_SERVER_UPDATE');
  if (!/^connected$/m.test(output)) problems.push('the media connection never came up');
  if (drop) {
    if (!/^dropping the SFU WebSocket$/m.test(output)) problems.push('it did not drop its socket');
    if (!/^resumed \(/m.test(output)) problems.push('the dropped socket was not resumed');
  }
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
  const heard = new RegExp(
    String.raw`^${heardFrom} spoke \d+ bytes \(-(?:\d|[1-3]\d) dBov\)$`,
    'm',
  );
  if (heardFrom !== null && !heard.test(output)) {
    problems.push(`it never decrypted a frame from ${heardFrom}`);
  }
  return problems;
}

/** Starts a process and collects its stdout; `done` resolves with the exit code. */
function start(cmd, args, env, options = {}) {
  const child = spawn(cmd, args, { env, stdio: ['ignore', 'pipe', 'inherit'], ...options });
  const run = { child, output: '' };
  child.stdout.on('data', (d) => {
    run.output += d;
    process.stdout.write(d);
  });
  run.done = new Promise((resolve) => child.on('exit', (c) => resolve(c ?? -1)));
  return run;
}

const rustBot = () =>
  process.env.SR_VOICE_BOT_BIN
    ? [process.env.SR_VOICE_BOT_BIN, []]
    : ['cargo', ['run', '-q', '-j', '2', '-p', 'sr-sdk', '--example', 'voice_bot']];

async function botUserId(token) {
  const res = await fetch(`${base}/api/v1/users/@me`, {
    headers: { Authorization: `Bot ${token}` },
  });
  if (!res.ok) throw new Error(`GET /users/@me -> ${res.status}`);
  return String((await res.json()).id);
}

async function until(f, ms, what) {
  const end = Date.now() + ms;
  while (!f()) {
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 200));
  }
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

  const common = {
    ...process.env,
    SR_API_URL: `${base}/api/v1`,
    SR_GATEWAY_URL: base.replace(/^http/, 'ws') + '/gateway',
    SR_GUILD_ID: run.guildId,
    SR_VOICE_CHANNEL_ID: voice.id,
    SR_BIND_IP: '127.0.0.1',
  };
  const killers = [];
  const failed = [];

  if (!typescript) {
    const env = {
      ...common,
      SR_BOT_TOKEN: run.token,
      SR_RUN_SECONDS: String(seconds),
      SR_DROP_AFTER_SECONDS: String(dropAfter),
    };
    const [cmd, args] = rustBot();
    console.log(`running the Rust voice bot for ${seconds} s (socket drop at ${dropAfter} s)`);
    const rust = start(cmd, args, env);
    killers.push(setTimeout(() => rust.child.kill(), (seconds + 300) * 1000));
    const code = await rust.done;
    failed.push(...verdict(rust.output, code));
  } else {
    // The Rust bot is the partner: it stays in the call from before the TypeScript bot joins until after it left.
    const partner = await installBot(run.person, run.guildId, 'Voice partner', permissions);
    const partnerEnv = {
      ...common,
      SR_BOT_TOKEN: partner.token,
      SR_RUN_SECONDS: String(seconds + 60),
    };
    const [cmd, args] = rustBot();
    console.log(`starting the Rust partner bot (${seconds + 60} s)`);
    const rust = start(cmd, args, partnerEnv);
    killers.push(setTimeout(() => rust.child.kill(), (seconds + 400) * 1000));
    await until(
      () => /^encrypting under epoch \d+$/m.test(rust.output),
      120_000,
      'the partner to be in the call',
    );

    const tsUser = await botUserId(run.token);
    const partnerUser = await botUserId(partner.token);
    console.log(
      `running the TypeScript voice bot for ${seconds} s (socket drop at ${dropAfter} s)`,
    );
    const pnpm = process.env.SR_PNPM ?? 'pnpm';
    const vitest = start(
      pnpm,
      ['--filter', '@sideroom/bot', 'exec', 'vitest', 'run', 'src/voice.live.test.ts'],
      {
        ...common,
        SR_VOICE_LIVE: '1',
        SR_BOT_TOKEN: run.token,
        SR_RUN_SECONDS: String(seconds),
        SR_DROP_AFTER_SECONDS: String(dropAfter),
        SR_PARTNER_USER: partnerUser,
      },
      { shell: true },
    );
    killers.push(setTimeout(() => vitest.child.kill(), (seconds + 300) * 1000));
    const tsCode = await vitest.done;
    if (tsCode !== 0) failed.push(`the TypeScript bot's test exited with ${tsCode}`);
    // The partner leaves when its time is up; the exit code and what it decrypted decide.
    const partnerCode = await rust.done;
    failed.push(
      ...verdict(rust.output, partnerCode, { drop: false, heardFrom: tsUser }).map(
        (p) => `partner: ${p}`,
      ),
    );
  }
  killers.forEach(clearTimeout);

  if (failed.length) {
    console.error(`voice bot run FAILED:\n- ${failed.join('\n- ')}`);
    process.exit(1);
  }
  console.log('voice bot run ok');
}

if (process.argv[1]?.endsWith('voice-bot-live.mjs')) {
  await main();
}
