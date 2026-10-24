import { apiPost, type SeededUser, type SignedIn } from './fixtures';

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
