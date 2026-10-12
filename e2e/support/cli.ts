import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

export const DATABASE_URL =
  process.env.DATABASE_URL ?? 'postgres://sideroom:sideroom_dev@127.0.0.1:5432/sideroom';
export const PUBLIC_URL = process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173';

/** Runs `sr-cli` (prebuilt binary if present, else `cargo run`) and returns stdout. Throws on failure. */
export function runCli(args: string[], extraEnv: Record<string, string> = {}): string {
  const exe = process.platform === 'win32' ? 'sr-cli.exe' : 'sr-cli';
  const bin = process.env.SR_CLI_BIN ?? resolve(repoRoot, 'target', 'debug', exe);
  const [cmd, cmdArgs]: [string, string[]] = existsSync(bin)
    ? [bin, args]
    : ['cargo', ['run', '-q', '-p', 'sr-cli', '--', ...args]];
  const r = spawnSync(cmd, cmdArgs, {
    cwd: repoRoot,
    encoding: 'utf8',
    env: {
      ...process.env,
      SR_DATABASE__URL: DATABASE_URL,
      SR_SERVER__PUBLIC_URL: PUBLIC_URL,
      ...extraEnv,
    },
  });
  if (r.status !== 0) {
    throw new Error(`sr-cli ${args.join(' ')} failed (${r.status}):\n${r.stdout}\n${r.stderr}`);
  }
  return r.stdout;
}

const CODE = /^[0-9A-HJKMNP-TV-Z]{5}-[0-9A-HJKMNP-TV-Z]{5}$/;

/** `sr-cli invite create` prints the formatted code on its first output line (issuer: the Owner). */
export function createInvite(note = 'e2e'): string {
  const out = runCli(['invite', 'create', '--note', note]);
  const code = out
    .split(/\r?\n/)
    .map((l) => l.trim())
    .find((l) => CODE.test(l));
  if (!code) throw new Error(`could not find an invite code in sr-cli output:\n${out}`);
  return code;
}
