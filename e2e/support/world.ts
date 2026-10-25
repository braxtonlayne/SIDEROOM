import type { APIRequestContext, Browser } from '@playwright/test';
import { PUBLIC_URL } from './cli';
import {
  apiPost,
  bootstrapState,
  signIn,
  test as base,
  type SeededUser,
  type SignedIn,
} from './fixtures';

/** DELETE with the session cookie and CSRF token; the Owner's servers are removed again after each spec. */
export async function apiDelete(request: APIRequestContext, path: string): Promise<void> {
  const origin = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
  const csrf = await request.get('/api/v1/sr/csrf', { headers: { Origin: origin } });
  const token = ((await csrf.json()) as { token: string }).token;
  await request.delete(`/api/v1/${path.replace(/^\//, '')}`, {
    headers: { Origin: origin, 'X-SR-CSRF': token },
  });
}

/**
 * Every spec that creates a server as the (shared) Owner removes it again, and the solo fixture makes only one per worker. The web app asks for one settings
 * document per server on every page load, and the API rate-limits that: a few dozen leftover servers made later
 * specs fail with 429 on a long local run.
 */
export async function dropServer(w: { a: SignedIn; guildId: string }): Promise<void> {
  await apiDelete(w.a.context.request, `guilds/${w.guildId}`).catch(() => undefined);
}

let ownerState: Awaited<ReturnType<SignedIn['context']['storageState']>> | null = null;

/**
 * The Owner's session, logged in once per worker and then reused through its cookies: the API allows 50 logins per
 * account and hour, and the Owner is the account every spec in this suite uses.
 */
export async function ownerSession(browser: Browser, owner: SeededUser): Promise<SignedIn> {
  if (!ownerState) {
    const s = await signIn(browser, owner);
    ownerState = await s.context.storageState();
    return s;
  }
  const context = await browser.newContext({ baseURL: PUBLIC_URL, storageState: ownerState });
  return { user: owner, context, page: await context.newPage() };
}

interface SoloGuild {
  guildId: string;
  channelId: string;
  guildName: string;
}

/**
 * `solo`: a one-person server of the Owner and a signed-in page in it. The server is made once per worker (guild
 * creation is rate-limited, and every server the Owner has costs a settings request on each page load) and removed
 * when the worker ends; each test gets its own browser context.
 */
export const test = base.extend<{ solo: SoloWorld }, { soloGuild: SoloGuild }>({
  soloGuild: [
    async ({ browser }, use) => {
      const { owner } = bootstrapState();
      const user: SeededUser = { id: '', ...owner };
      const w = await soloServer((u) => ownerSession(browser, u), user);
      await use({ guildId: w.guildId, channelId: w.channelId, guildName: w.guildName });
      await dropServer(w);
      await w.a.context.close();
    },
    { scope: 'worker' },
  ],
  solo: async ({ soloGuild, owner, browser }, use) => {
    const a = await ownerSession(browser, owner);
    await use({ a, ...soloGuild });
    await a.context.close();
  },
});
export { expect } from './fixtures';

/** What a spec needs from a one-person server (the Owner, who is already registered: no new account). */
export interface SoloWorld {
  a: SignedIn;
  guildId: string;
  channelId: string;
  guildName: string;
}

/**
 * The Owner signs in and creates a server through the API. Registration allows 20 accounts per address and day, and
 * the keyboard specs do not need a second person, so they spend none.
 */
export async function soloServer(
  signedIn: (u: SeededUser) => Promise<SignedIn>,
  owner: SeededUser,
  label = 'Kbd',
): Promise<SoloWorld> {
  const a = await signedIn(owner);
  const guildName = `${label} ${Date.now().toString(36)}`;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a.context.request,
    '/guilds',
    { name: guildName },
  );
  const channel = guild.channels.find((c) => c.type === 0);
  if (!channel) throw new Error('new server has no text channel');
  return { a, guildId: guild.id, channelId: channel.id, guildName };
}

/**
 * The fixture world of the a11y specs (RDY-A11Y-001): a server with a few messages and a thread, a published
 * Cinema title and a watch party, all created through the API as the Owner. Library series come from
 * `sr-cli seed` where object storage is reachable; the specs say so when they are missing.
 */
export interface World {
  guildId: string;
  channelId: string;
  messageId: string;
  threadId: string;
  titleId: string;
  partyId: string | null;
}

export async function buildWorld(s: SoloWorld): Promise<World> {
  const r = s.a.context.request;
  let messageId = '';
  for (const text of [
    'Welcome to the lounge. **Bold** and `code` render here.',
    'Second message with a link https://example.com/page',
    'Third message, the thread starts here.',
  ]) {
    const m = await apiPost<{ id: string }>(r, `/channels/${s.channelId}/messages`, {
      content: text,
    });
    messageId = m.id;
  }
  const thread = await apiPost<{ id: string }>(
    r,
    `/channels/${s.channelId}/messages/${messageId}/threads`,
    { name: 'Thread for the audit' },
  );
  await apiPost(r, `/channels/${thread.id}/messages`, { content: 'First thread reply.' });

  const name = `A11y Film ${Date.now().toString(36)}`;
  const title = await apiPost<{ id: string }>(r, '/admin/cinema/titles', {
    kind: 'movie',
    title: name,
    year: 2001,
    synopsis: 'A fixture film for the accessibility audit.',
    genres: ['Drama'],
  });
  await r.put(`/api/v1/admin/cinema/titles/${title.id}/asset`, {
    headers: await csrfHeaders(r),
    data: { asset_id: '4242' },
  });
  await apiPost(r, `/admin/cinema/titles/${title.id}/publish`);

  let partyId: string | null = null;
  try {
    const party = await apiPost<{ id: string }>(r, '/watch-parties', { title_id: title.id });
    partyId = party.id;
  } catch {
    partyId = null;
  }
  return {
    guildId: s.guildId,
    channelId: s.channelId,
    messageId,
    threadId: thread.id,
    titleId: title.id,
    partyId,
  };
}

async function csrfHeaders(r: import('@playwright/test').APIRequestContext) {
  const res = await r.get('/api/v1/sr/csrf', {
    headers: { Origin: new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin },
  });
  const token = ((await res.json()) as { token: string }).token;
  return {
    Origin: new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin,
    'X-SR-CSRF': token,
  };
}
