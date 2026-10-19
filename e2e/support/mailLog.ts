import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { repoRoot } from './cli';

/**
 * The e2e stack has no mail relay: without `[smtp] host` sr-server uses the log-only mailer, which writes
 * every message (recipient and body, single-use links included) to the server log. The log file is the dev
 * mail sink: `server.log` in the repo root (what `deploy/compose/e2e-run.sh` writes), or the file named by
 * `SR_E2E_SERVER_LOG`. The mail body never reaches the browser, so this is the only way to read the link.
 */
export const SERVER_LOG = process.env.SR_E2E_SERVER_LOG ?? resolve(repoRoot, 'server.log');

// eslint-disable-next-line no-control-regex
const ANSI = /\u001b\[[0-9;]*m/g;

/** Polls the server log for the first `/<path>/<token>` link logged after a mail to `to`. */
export async function mailedToken(
  to: string,
  path: 'verify-email' | 'reset',
  timeoutMs = 20_000,
): Promise<string> {
  if (!existsSync(SERVER_LOG)) {
    throw new Error(`server log not found at ${SERVER_LOG}; set SR_E2E_SERVER_LOG`);
  }
  const link = new RegExp(`/${path}/([A-Za-z0-9_-]+)`);
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const log = readFileSync(SERVER_LOG, 'utf8').replace(ANSI, '');
    const at = log.lastIndexOf(to);
    if (at >= 0) {
      const m = link.exec(log.slice(at));
      if (m?.[1]) return m[1];
    }
    if (Date.now() > deadline) throw new Error(`no ${path} mail for ${to} in ${SERVER_LOG}`);
    await new Promise((r) => setTimeout(r, 300));
  }
}
