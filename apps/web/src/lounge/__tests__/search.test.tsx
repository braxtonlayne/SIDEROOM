import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../router';
import { resetStore, useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { useLoungeUi } from '../store';
import {
  applyFilterOption,
  currentToken,
  filterOptions,
  groupByChannel,
  searchHistory,
  sortParams,
} from '../searchUi';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const lookups = {
  users: [
    { id: '2', names: ['bob', 'Bob B'] },
    { id: '3', names: ['alice', 'Alice'] },
  ],
  channels: [
    { id: 'c1', name: 'general' },
    { id: 'c2', name: 'random' },
  ],
};

describe('search filter helpers', () => {
  it('lists filter names for a bare prefix', () => {
    const o = filterOptions('fr', lookups, { isDm: false });
    expect(o.map((x) => x.insert)).toEqual(['from:']);
    expect(filterOptions('', lookups).length).toBe(0);
    expect(filterOptions('i', lookups, { isDm: true })).toEqual([]);
  });

  it('offers people, channels, has values, booleans and dates', () => {
    expect(filterOptions('from:b', lookups).map((x) => x.insert)).toEqual(['from:bob ']);
    expect(filterOptions('mentions:al', lookups).map((x) => x.insert)).toEqual(['mentions:alice ']);
    expect(filterOptions('in:ran', lookups).map((x) => x.label)).toEqual(['#random']);
    expect(filterOptions('has:i', lookups).map((x) => x.insert)).toEqual(['has:image ']);
    expect(filterOptions('pinned:t', lookups).map((x) => x.insert)).toEqual(['pinned:true ']);
    const d = filterOptions('before:', lookups, { isDm: false, now: new Date(2026, 0, 15) });
    expect(d.map((x) => x.insert)).toEqual([
      'before:2026-01-15 ',
      'before:2026-01-14 ',
      'before:2026-01-08 ',
    ]);
  });

  it('replaces only the token being typed', () => {
    expect(currentToken('hello fro')).toEqual({ token: 'fro', start: 6 });
    expect(applyFilterOption('hello fro', { label: 'from:', insert: 'from:' })).toBe('hello from:');
  });

  it('maps sort choices to API params and groups hits by channel', () => {
    expect(sortParams('new')).toEqual({ sort_by: 'timestamp', sort_order: 'desc' });
    expect(sortParams('old')).toEqual({ sort_by: 'timestamp', sort_order: 'asc' });
    expect(sortParams('relevant')).toEqual({ sort_by: 'relevance', sort_order: 'desc' });
    const g = groupByChannel([
      { channel_id: 'a', id: '1' },
      { channel_id: 'b', id: '2' },
      { channel_id: 'a', id: '3' },
    ]);
    expect(g.map((x) => [x.channelId, x.hits.map((h) => h.id)])).toEqual([
      ['a', ['1', '3']],
      ['b', ['2']],
    ]);
  });

  it('keeps ten recent queries, newest first, and can clear them', () => {
    localStorage.clear();
    for (let i = 0; i < 12; i++) searchHistory.add(`q${i}`);
    searchHistory.add('q5');
    const list = searchHistory.list();
    expect(list).toHaveLength(10);
    expect(list[0]).toBe('q5');
    expect(new Set(list).size).toBe(10);
    searchHistory.clear();
    expect(searchHistory.list()).toEqual([]);
  });
});

const me = { id: '1', username: 'me', global_name: 'Me' };
const bob = { id: '2', username: 'bob', global_name: 'Bob' };

function seed() {
  resetStore();
  localStorage.clear();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useStore.setState({
    currentUserId: '1',
    users: { '1': me, '2': bob },
    guilds: { g1: { id: 'g1', name: 'Guild', owner_id: '1' } },
    guildOrder: ['g1'],
    channels: {
      c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', position: 1 },
      c2: { id: 'c2', type: 0, guild_id: 'g1', name: 'random', position: 2 },
    },
    channelIdsByGuild: { g1: ['c1', 'c2'] },
    roles: {
      g1: {
        id: 'g1',
        guild_id: 'g1',
        name: '@everyone',
        color: 0,
        position: 0,
        permissions: '3072',
      },
    },
    roleIdsByGuild: { g1: ['g1'] },
    memberIdsByGuild: { g1: ['1', '2'] },
    members: {
      'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
      'g1:2': { guild_id: 'g1', user_id: '2', roles: [] },
    },
    windows: {
      c1: {
        ids: [],
        pending: [],
        hasMoreBefore: false,
        atLatest: true,
        loading: false,
        error: null,
      },
    },
    messages: {},
    connection: 'connected',
  });
}

const hit = (id: string, channel: string, text: string) => ({
  id,
  channel_id: channel,
  guild_id: 'g1',
  author: bob,
  content: text,
  timestamp: '2026-01-01T00:00:00Z',
  hit: true,
  sr_snippet: { text, highlights: [] as [number, number][] },
});

describe('search box UI', () => {
  const params: URLSearchParams[] = [];
  beforeEach(() => {
    seed();
    params.length = 0;
    server.use(
      http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.get('*/api/v1/guilds/:gid/members', () => HttpResponse.json([])),
      http.get('*/api/v1/channels/:cid/messages', () => HttpResponse.json([])),
      http.get('*/api/v1/guilds/g1/messages/search', ({ request }) => {
        const p = new URL(request.url).searchParams;
        params.push(p);
        const offset = Number(p.get('offset') ?? 0);
        return HttpResponse.json({
          total_results: 60,
          messages: [
            [hit(`${offset}1`, 'c1', 'first')],
            [hit(`${offset}2`, 'c2', 'second')],
            [hit(`${offset}3`, 'c1', 'third')],
          ],
        });
      }),
    );
  });

  const open = async () => {
    const router = createAppRouter(createMemoryHistory({ initialEntries: ['/channels/g1/c1'] }));
    const view = render(<RouterProvider router={router} />);
    const box = await screen.findByRole('searchbox', { name: 'Search #general' });
    return { box, ...view };
  };

  it('suggests filter values and inserts the picked one', async () => {
    const { box } = await open();
    await userEvent.type(box, 'hello from:b');
    const list = await screen.findByRole('listbox', { name: 'Filter suggestions' });
    expect(within(list).getByRole('option', { name: /Bob/ })).toBeInTheDocument();
    await userEvent.keyboard('{Tab}');
    expect(box).toHaveValue('hello from:bob ');
  });

  it('groups results by channel, sorts, and pages', async () => {
    const { box } = await open();
    await userEvent.type(box, 'x{Enter}');
    const panel = await screen.findByRole('region', { name: 'Search results' });
    expect(await within(panel).findByText('60 results')).toBeInTheDocument();
    const heads = within(panel).getAllByRole('heading', { level: 3 });
    expect(heads.map((h) => h.textContent)).toEqual(['#general', '#random']);
    await userEvent.selectOptions(within(panel).getByRole('combobox'), 'old');
    await waitFor(() => expect(params.at(-1)!.get('sort_order')).toBe('asc'));
    await userEvent.click(within(panel).getByRole('button', { name: 'Next' }));
    await waitFor(() => expect(params.at(-1)!.get('offset')).toBe('25'));
    expect(await within(panel).findByText('Page 2 of 3')).toBeInTheDocument();
    expect(await axeViolations(panel)).toEqual([]);
  });

  it('keeps recent searches and can clear them', async () => {
    const { box } = await open();
    await userEvent.type(box, 'needle{Enter}');
    await screen.findByText('60 results');
    expect(searchHistory.list()).toEqual(['needle']);
    await userEvent.clear(box);
    const recent = await screen.findByRole('heading', { name: 'Recent searches' });
    expect(recent).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /needle/ }));
    await waitFor(() => expect(params.at(-1)!.get('q')).toBe('needle'));
    await userEvent.clear(box);
    await userEvent.click(await screen.findByRole('button', { name: 'Clear history' }));
    expect(searchHistory.list()).toEqual([]);
  });
});
