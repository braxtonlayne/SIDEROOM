// Shared seeding for the wave-4 Lounge gap tests: a signed-in owner of guild g1 with channels c1/c2.
import { render } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { resetStore, useStore } from '../../store';
import type * as S from '../../store/types';
import { http, HttpResponse, server } from '../../test/server';
import { resetComposerDrafts } from '../Composer';
import { useLoungeUi } from '../store';
import { useIgnores } from '../shell/ignores';
import { useSocial } from '../shell/social';
import { usePrefsStore, DEFAULT_PREFS } from '../../settings/user/prefs';

export const me: S.User = { id: '1', username: 'me', global_name: 'Me Myself' };
export const bob: S.User = { id: '2', username: 'bob', global_name: 'Bob' };

export const NOW = Date.now();
export const iso = (minAgo: number) => new Date(NOW - minAgo * 60_000).toISOString();

export const msg = (id: string, over: Partial<S.Message> = {}): S.Message => ({
  id,
  channel_id: 'c1',
  author: bob,
  content: '',
  timestamp: iso(60 - (Number(id.slice(-2)) || 0)),
  ...over,
});

export interface SeedOptions {
  messages: S.Message[];
  atLatest?: boolean;
  roles?: S.Role[];
  members?: Record<string, S.Member>;
  channels?: Record<string, S.Channel>;
  users?: Record<string, S.User>;
}

export function seedLounge(o: SeedOptions) {
  resetStore();
  localStorage.clear();
  resetComposerDrafts();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useSocial.getState().reset();
  useIgnores.getState().reset();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  const everyone: S.Role = {
    id: 'g1',
    guild_id: 'g1',
    name: '@everyone',
    color: 0,
    position: 0,
    permissions: '3072',
  };
  const roles = [everyone, ...(o.roles ?? [])];
  const channels: Record<string, S.Channel> = o.channels ?? {
    cat: { id: 'cat', type: 4, guild_id: 'g1', name: 'Text Channels', position: 0 },
    c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', parent_id: 'cat', position: 1 },
    c2: { id: 'c2', type: 0, guild_id: 'g1', name: 'random', parent_id: 'cat', position: 2 },
  };
  useStore.setState({
    currentUserId: '1',
    users: { '1': me, '2': bob, ...o.users },
    guilds: { g1: { id: 'g1', name: 'Store Guild', owner_id: '1' } },
    guildOrder: ['g1'],
    channels,
    channelIdsByGuild: { g1: Object.keys(channels) },
    roles: Object.fromEntries(roles.map((r) => [r.id, r])),
    roleIdsByGuild: { g1: roles.map((r) => r.id) },
    memberIdsByGuild: { g1: ['1', '2'] },
    members: o.members ?? {
      'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
      'g1:2': { guild_id: 'g1', user_id: '2', roles: [] },
    },
    windows: {
      c1: {
        ids: o.messages.map((m) => m.id),
        pending: [],
        hasMoreBefore: false,
        atLatest: o.atLatest ?? true,
        loading: false,
        error: null,
      },
    },
    messages: Object.fromEntries(o.messages.map((m) => [m.id, m])),
    connection: 'connected',
  });
}

export function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  const view = render(<RouterProvider router={router} />);
  return { router, ...view };
}

/** Requests the app made, in order, as `METHOD path?query` with the parsed JSON body. */
export const calls: { key: string; body: unknown }[] = [];
export const record =
  (method: string) =>
  async ({ request }: { request: Request }) => {
    let body: unknown = null;
    try {
      body = await request.clone().json();
    } catch {
      /* no body */
    }
    const u = new URL(request.url);
    calls.push({ key: `${method} ${u.pathname.replace('/api/v1', '')}${u.search}`, body });
    return undefined;
  };

/** Handlers every Lounge page load needs. */
export function baseHandlers() {
  calls.length = 0;
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.get('*/api/v1/guilds/:gid/members', () => HttpResponse.json([])),
    http.get('*/api/v1/channels/:cid/messages', () => HttpResponse.json([])),
    http.post(
      '*/api/v1/channels/:cid/messages/:mid/ack',
      () => new HttpResponse(null, { status: 204 }),
    ),
  );
}
