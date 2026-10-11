import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runCli } from './support/cli';

export const OWNER = {
  username: 'owner',
  email: 'owner@example.test',
  // Dev-only credential for the throwaway e2e database.
  password: process.env.SR_OWNER_PASSWORD ?? 'e2e-owner-Passw0rd!',
};

export const STATE_FILE = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '.state',
  'bootstrap.json',
);

export interface BootstrapState {
  owner: typeof OWNER;
  /** One-time login link printed by `sr-cli bootstrap`; null if the Owner already existed. */
  loginLink: string | null;
}

/**
 * Applies migrations and bootstraps the Owner on a fresh install (exit gate: "a fresh install can be
 * bootstrapped"). Re-running against an already-bootstrapped database is tolerated: the Owner then
 * exists with the same deterministic password, so only the one-time link is unavailable.
 */
export default function globalSetup() {
  runCli(['migrate']);

  let loginLink: string | null = null;
  try {
    const out = runCli(
      ['bootstrap', '--owner-username', OWNER.username, '--owner-email', OWNER.email],
      { SR_OWNER_PASSWORD: OWNER.password },
    );
    loginLink = out.match(/https?:\/\/\S+\/login\/link\?token=\S+/)?.[0] ?? null;
    if (!loginLink) throw new Error(`no login link in bootstrap output:\n${out}`);
  } catch (e) {
    if (!/already (in use|bootstrapped)/i.test(String(e))) throw e;
    console.warn('[e2e] Owner already exists; reusing it (login link not available).');
  }

  const state: BootstrapState = { owner: OWNER, loginLink };
  mkdirSync(dirname(STATE_FILE), { recursive: true });
  writeFileSync(STATE_FILE, JSON.stringify(state, null, 2));
}
