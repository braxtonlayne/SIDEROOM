// SDK compatibility run (M-16, D-306, DEV-SDK-003). Provisions the test server over REST, walks the tier-2
// list of docs/dev/sdk-compat.md with one library's sample bot and writes a JUnit report.
//
//   node run.mjs --lib discord.js|serenity [--out report/discord.js.xml]
//
// Environment: SDK_COMPAT_BASE (default http://127.0.0.1:8080; also the server's SR_SERVER__PUBLIC_URL),
// SDK_COMPAT_OWNER and SDK_COMPAT_PASSWORD (the bootstrapped Owner; same defaults as e2e/global-setup.ts),
// SDK_COMPAT_SERENITY_BIN (a built serenity_bot example; default: cargo run it).
// Exit code 0 when every check passed, 1 otherwise. A one-line summary also goes to $GITHUB_STEP_SUMMARY.
import { spawn } from 'node:child_process';
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { toJunit } from './lib/junit.mjs';
import { provision, waitReady } from './lib/provision.mjs';
import { BotRun, walk } from './lib/steps.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const args = process.argv.slice(2);
const opt = (n, d) => (args.includes(n) ? args[args.indexOf(n) + 1] : d);

const lib = opt('--lib', 'discord.js');
const base = (process.env.SDK_COMPAT_BASE ?? 'http://127.0.0.1:8080').replace(/\/$/, '');
const owner = process.env.SDK_COMPAT_OWNER ?? 'owner';
const password =
  process.env.SDK_COMPAT_PASSWORD ?? process.env.SR_OWNER_PASSWORD ?? 'e2e-owner-Passw0rd!';
const out = resolve(opt('--out', resolve(here, 'report', `${lib}.xml`)));

/** The exact version the run is pinned to, read from the manifest that pins it. */
function versionOf(l) {
  if (l === 'discord.js') {
    return JSON.parse(readFileSync(resolve(here, 'package.json'), 'utf8')).dependencies[
      'discord.js'
    ];
  }
  const m = readFileSync(resolve(repo, 'crates/sr-server/Cargo.toml'), 'utf8').match(
    /serenity\s*=\s*\{[^}]*version\s*=\s*"=?([^"]+)"/,
  );
  if (!m) throw new Error('serenity is not pinned in crates/sr-server/Cargo.toml');
  return m[1];
}

function spawnBot(l, env) {
  const full = { ...process.env, ...env };
  if (l === 'discord.js') {
    return spawn(process.execPath, [resolve(here, 'discordjs-bot.mjs')], { env: full, cwd: here });
  }
  if (l === 'serenity') {
    const bin = process.env.SDK_COMPAT_SERENITY_BIN;
    if (bin) return spawn(bin, [], { env: full, cwd: repo });
    return spawn(
      'cargo',
      [
        'run',
        '-q',
        '-j',
        '2',
        '-p',
        'sr-server',
        '--example',
        'serenity_bot',
        '--features',
        'sdk-compat',
      ],
      { env: full, cwd: repo },
    );
  }
  throw new Error(`unknown library ${l} (discord.js or serenity)`);
}

const version = versionOf(lib);
console.log(`SDK compat: ${lib} ${version} against ${base}`);

let cases;
try {
  await waitReady(base);
  const ctx = await provision({ base, owner, password });
  cases = await walk({
    ctx,
    log: (s) => console.log(s),
    start: (content) =>
      new BotRun(
        spawnBot(lib, {
          SDK_COMPAT_BASE: base,
          SDK_COMPAT_TOKEN: ctx.token,
          SDK_COMPAT_CONTENT: content ? '1' : '0',
        }),
      ),
  });
} catch (e) {
  // Provisioning failed: the server, not the library, is the suspect, but it still fails the run.
  cases = [{ name: 'provision', ok: false, message: String(e?.stack ?? e) }];
}

mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, toJunit({ library: lib, version, cases }));
const failed = cases.filter((c) => !c.ok);
for (const c of cases)
  console.log(`${c.ok ? 'ok  ' : 'FAIL'} ${c.name}${c.ok ? '' : `: ${c.message}`}`);
const summary = `${failed.length ? 'FAIL' : 'PASS'} ${lib} ${version}: ${cases.length - failed.length}/${cases.length} checks`;
console.log(summary);
if (process.env.GITHUB_STEP_SUMMARY)
  appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${summary}\n`);
process.exit(failed.length ? 1 : 0);
