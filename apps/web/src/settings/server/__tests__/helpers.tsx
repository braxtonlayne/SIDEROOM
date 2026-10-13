import { act, render, screen, waitFor } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../../router';
import { resetStore, useStore } from '../../../store';
import { http, HttpResponse, server } from '../../../test/server';

export { http, HttpResponse, server };

export const bits = (...n: number[]) => n.reduce((a, b) => a | (1n << BigInt(b)), 0n).toString();

export const MANAGE = {
  guild: 5,
  roles: 28,
  channels: 4,
  ban: 2,
  kick: 1,
  audit: 7,
  expressions: 30,
  webhooks: 29,
  moderate: 40,
  admin: 3,
};

export interface Seed {
  /** Permission bit numbers of the viewer's own role. */
  perms: number[];
  /** Position of the viewer's role. */
  position?: number;
  ownerId?: string;
}

/** Guild g1: roles admin(10) > viewer role(position) / mod(5) > low(1); members 1 (me), 2 bob(low), 3 carol(mod). */
export function seed({ perms, position = 7, ownerId = '9' }: Seed) {
  resetStore();
  const role = (id: string, name: string, pos: number, p = '0') => ({
    id,
    guild_id: 'g1',
    name,
    color: 0,
    position: pos,
    permissions: p,
  });
  useStore.setState({
    currentUserId: '1',
    users: {
      '1': { id: '1', username: 'me' },
      '2': { id: '2', username: 'bob' },
      '3': { id: '3', username: 'carol' },
      '9': { id: '9', username: 'owner' },
    },
    guilds: { g1: { id: 'g1', name: 'Book Club', owner_id: ownerId } },
    guildOrder: ['g1'],
    channels: {
      cat: { id: 'cat', type: 4, guild_id: 'g1', name: 'Text', position: 0 },
      c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', parent_id: 'cat', position: 0 },
      c2: { id: 'c2', type: 0, guild_id: 'g1', name: 'random', parent_id: 'cat', position: 1 },
    },
    channelIdsByGuild: { g1: ['cat', 'c1', 'c2'] },
    roles: {
      g1: role('g1', '@everyone', 0, bits(10, 11)),
      mine: role('mine', 'Viewer', position, bits(...perms)),
      admin: role('admin', 'Admin', 10, bits(3)),
      mod: role('mod', 'Mod', 5, bits(1, 2)),
      low: role('low', 'Member', 1),
    },
    roleIdsByGuild: { g1: ['g1', 'mine', 'admin', 'mod', 'low'] },
    memberIdsByGuild: { g1: ['1', '2', '3'] },
    members: {
      'g1:1': { guild_id: 'g1', user_id: '1', roles: ['mine'] },
      'g1:2': { guild_id: 'g1', user_id: '2', roles: ['low'] },
      'g1:3': { guild_id: 'g1', user_id: '3', roles: ['mod'] },
    },
    connection: 'connected',
  });
}

export const restRole = (id: string, name: string, position: number, over: object = {}) => ({
  id,
  name,
  color: 0,
  hoist: false,
  position,
  permissions: '0',
  managed: false,
  mentionable: false,
  ...over,
});

export const restRoles = () => [
  restRole('g1', '@everyone', 0, { permissions: bits(10, 11) }),
  restRole('low', 'Member', 1),
  restRole('mod', 'Mod', 5, { permissions: bits(1, 2) }),
  restRole('mine', 'Viewer', 7),
  restRole('admin', 'Admin', 10, { permissions: bits(3) }),
];

export const memberObj = (id: string, name: string, roles: string[], extra: object = {}) => ({
  user: { id, username: name },
  nick: null,
  roles,
  joined_at: '2026-01-01T00:00:00Z',
  communication_disabled_until: null,
  ...extra,
});

/** Handlers every settings page needs (CSRF, members for the viewer lookup). */
export function baseHandlers() {
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.get('*/api/v1/guilds/:gid/members', () =>
      HttpResponse.json([
        memberObj('1', 'me', ['mine']),
        memberObj('2', 'bob', ['low']),
        memberObj('3', 'carol', ['mod']),
      ]),
    ),
  );
}

export async function openSettings(section: string) {
  baseHandlers();
  const router = createAppRouter(
    createMemoryHistory({ initialEntries: [`/settings/server/g1/${section}`] }),
  );
  const view = render(<RouterProvider router={router} />);
  await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
  return { router, ...view };
}

/** Capture the JSON body of a request. */
export function capture() {
  const calls: { url: string; method: string; body: unknown; headers: Headers }[] = [];
  return {
    calls,
    record: async (request: Request) => {
      let body: unknown = undefined;
      try {
        body = await request.clone().json();
      } catch {
        /* no body */
      }
      calls.push({ url: request.url, method: request.method, body, headers: request.headers });
    },
  };
}

export const flush = () => act(async () => {});
