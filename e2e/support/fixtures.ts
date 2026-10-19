import { readFileSync } from 'node:fs';
import {
  test as base,
  expect,
  type APIRequestContext,
  type Browser,
  type BrowserContext,
  type Page,
} from '@playwright/test';
import { STATE_FILE, type BootstrapState } from '../global-setup';
import { createInvite, PUBLIC_URL } from './cli';

/**
 * Shared Playwright fixtures (XC-TEST-004).
 *
 *  - `owner`        the bootstrapped Owner credentials (global-setup).
 *  - `seedUser`     registers a fresh user through the API (platform invite from `sr-cli`), no UI clicks.
 *  - `signedIn`     opens an isolated browser context already logged in as a user (cookie session).
 *  - `scenario`     multi-user setups: `ownerAndMember()` builds a server with its Owner and a second
 *                   user who has joined it, each in their own signed-in context and page.
 *
 * Everything created through these fixtures is closed with the test; seeded accounts persist in the
 * throwaway e2e database, so usernames carry a random stamp.
 *
 * Usage:
 *   import { test, expect } from '../support/fixtures';
 *   test('two users chat', async ({ scenario }) => {
 *     const { a, b, channelId } = await scenario.ownerAndMember();
 *     ...
 *   });
 */

const API = '/api/v1';

export interface SeededUser {
  id: string;
  username: string;
  email: string;
  password: string;
}

export interface SignedIn {
  user: SeededUser;
  context: BrowserContext;
  page: Page;
}

export interface Scenario {
  /** The server's Owner (the bootstrapped Owner account) and the joined member. */
  a: SignedIn;
  b: SignedIn;
  guildId: string;
  /** `#general` of the new server. */
  channelId: string;
  guildName: string;
}

export const bootstrapState = (): BootstrapState =>
  JSON.parse(readFileSync(STATE_FILE, 'utf8')) as BootstrapState;

const stamp = () => `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;

const headers = (token?: string): Record<string, string> => ({
  Origin: new URL(PUBLIC_URL).origin,
  ...(token ? { 'X-SR-CSRF': token } : {}),
});

/** CSRF token for the cookie session, or undefined when anonymous (register/login need none). */
async function csrf(request: APIRequestContext): Promise<string | undefined> {
  const res = await request.get(`${API}/sr/csrf`, { headers: headers() });
  if (!res.ok()) return undefined;
  return ((await res.json()) as { token: string }).token;
}

/** POST with session cookie + CSRF header; throws with the body on a non-2xx. */
export async function apiPost<T = unknown>(
  request: APIRequestContext,
  path: string,
  data?: unknown,
): Promise<T> {
  const res = await request.post(`${API}${path}`, {
    headers: headers(await csrf(request)),
    data,
  });
  if (!res.ok()) throw new Error(`POST ${path} -> ${res.status()}: ${await res.text()}`);
  return (res.status() === 204 ? undefined : await res.json()) as T;
}

/** PATCH with session cookie + CSRF header; throws with the body on a non-2xx. */
export async function apiPatch<T = unknown>(
  request: APIRequestContext,
  path: string,
  data?: unknown,
): Promise<T> {
  const res = await request.patch(`${API}${path}`, {
    headers: headers(await csrf(request)),
    data,
  });
  if (!res.ok()) throw new Error(`PATCH ${path} -> ${res.status()}: ${await res.text()}`);
  return (res.status() === 204 ? undefined : await res.json()) as T;
}

/** Register a new account over the API using a fresh `sr-cli` platform invite. */
export async function registerUser(
  request: APIRequestContext,
  label = 'user',
): Promise<SeededUser> {
  const s = stamp();
  const user = {
    username: `${label}_${s}`.slice(0, 32),
    email: `${label}_${s}@example.test`,
    password: 'Seeded-Passw0rd-9!',
  };
  const body = { code: createInvite(`e2e ${label} ${s}`), ...user, accepted_house_notes: true, confirmed_min_age: 13 };
  // Registration allows 5 per minute per address: wait out a short window instead of failing.
  for (let attempt = 0; ; attempt++) {
    const res = await request.post(`${API}/sr/register`, { headers: headers(), data: body });
    if (res.status() === 429 && attempt < 3) {
      const retry = Number(((await res.json()) as { retry_after?: number }).retry_after ?? 60);
      if (retry <= 70) {
        await new Promise((r) => setTimeout(r, (retry + 1) * 1000));
        continue;
      }
    }
    if (!res.ok()) throw new Error(`POST /sr/register -> ${res.status()}: ${await res.text()}`);
    return { id: ((await res.json()) as { id: string }).id, ...user };
  }
}

/** Log `request`'s cookie jar in as `user` (accounts without 2FA only). */
export async function loginApi(
  request: APIRequestContext,
  user: Pick<SeededUser, 'username' | 'password'>,
): Promise<void> {
  const res = await apiPost<{ status: string }>(request, '/sr/auth/login', {
    login: user.username,
    password: user.password,
  });
  expect(res.status, 'login result').toBe('ok');
}

/** A new isolated browser context + page, logged in as `user` without touching the login form. */
export async function signIn(browser: Browser, user: SeededUser): Promise<SignedIn> {
  const context = await browser.newContext({ baseURL: PUBLIC_URL });
  await loginApi(context.request, user);
  const page = await context.newPage();
  return { user, context, page };
}

export interface Fixtures {
  owner: SeededUser;
  seedUser: (label?: string) => Promise<SeededUser>;
  signedIn: (user: SeededUser) => Promise<SignedIn>;
  scenario: { ownerAndMember: () => Promise<Scenario> };
}

export const test = base.extend<Fixtures>({
  // eslint-disable-next-line no-empty-pattern
  owner: async ({}, use) => {
    const { owner } = bootstrapState();
    await use({ id: '', username: owner.username, email: owner.email, password: owner.password });
  },

  seedUser: async ({ playwright }, use) => {
    await use(async (label) => {
      const ctx = await playwright.request.newContext({ baseURL: PUBLIC_URL });
      try {
        return await registerUser(ctx, label);
      } finally {
        await ctx.dispose();
      }
    });
  },

  signedIn: async ({ browser }, use) => {
    const opened: SignedIn[] = [];
    await use(async (user) => {
      const s = await signIn(browser, user);
      opened.push(s);
      return s;
    });
    for (const s of opened) await s.context.close();
  },

  scenario: async ({ owner, seedUser, signedIn }, use) => {
    await use({
      async ownerAndMember() {
        const a = await signedIn(owner);
        const guildName = `Scn ${stamp()}`;
        const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
          a.context.request,
          '/guilds',
          { name: guildName },
        );
        const channel = guild.channels.find((c) => c.type === 0);
        if (!channel) throw new Error('new server has no text channel');
        const invite = await apiPost<{ code: string }>(
          a.context.request,
          `/channels/${channel.id}/invites`,
          {},
        );
        const b = await signedIn(await seedUser('member'));
        await apiPost(b.context.request, `/invites/${invite.code}`);
        return { a, b, guildId: guild.id, channelId: channel.id, guildName };
      },
    });
  },
});

export { expect };
