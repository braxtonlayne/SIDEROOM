// Feature-module UIs (FTR-BDAY/LVL/RSS/POLLT/SUGG/TICK/RR-001) and the server tag chip (LNG-SSET-012).
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { FeaturesSection } from '../../settings/server/sections/Features';
import { MessagePicker } from '../../settings/server/sections/MessagePicker';
import { RssPanel, keywordsOf } from '../../settings/server/sections/RssFeeds';
import { BirthdayCard, daysIn } from '../../settings/user/sections/BirthdayCard';
import { useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { CommunityDialog } from '../community/CommunityDialog';
import { Leveling } from '../community/Leveling';
import { Suggestions } from '../community/Suggestions';
import { Tickets } from '../community/Tickets';
import { ServerTagChip } from '../components/ServerTagChip';
import { useModulesStore } from '../features/useModules';
import { serverTagOf, toLoungeUser } from '../liveMap';
import { PollModal } from '../msg/PollModal';
import { runSlash } from '../msg/slashCommands';
import { baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const goto = vi.fn();
vi.mock('../nav', async (orig) => ({
  ...(await orig<typeof import('../nav')>()),
  useLoungeGoto: () => goto,
}));

const modules = (on: string[], respond: string[] = []) =>
  http.get('*/api/v1/guilds/g1/modules', () =>
    HttpResponse.json({
      modules: Object.fromEntries(on.map((k) => [k, true])),
      // G-71: the server says where the viewer may respond (Manage Server or the staff role).
      member_capabilities: Object.fromEntries(
        on
          .filter((k) => k === 'community.suggestions' || k === 'community.tickets')
          .map((k) => [k, { can_respond: respond.includes(k) }]),
      ),
    }),
  );

/** Someone other than the viewer owns the server, so the viewer has no staff rights. */
const asMember = () =>
  useStore.setState((s) => ({
    guilds: { ...s.guilds, g1: { ...s.guilds.g1!, owner_id: '2' } },
  }));

beforeEach(() => {
  seedLounge({ messages: [] });
  useModulesStore.getState().reset();
  baseHandlers();
  goto.mockReset();
});

describe('server tag chip (LNG-SSET-012)', () => {
  it('maps primary_guild to a chip and honours identity_enabled', () => {
    expect(
      serverTagOf({
        primary_guild: { identity_enabled: true, tag: 'bk', badge: 3, color: 255 },
      }),
    ).toEqual({ tag: 'BK', tagBadge: 3, tagColor: 255 });
    expect(
      serverTagOf({ primary_guild: { identity_enabled: false, tag: 'bk', badge: 3, color: 1 } }),
    ).toEqual({});
    expect(serverTagOf({ primary_guild: null })).toEqual({});
    expect(serverTagOf({})).toEqual({});
  });

  it('puts the tag on the user shown in chat and in the member list', () => {
    const u = toLoungeUser({
      id: '9',
      username: 'ann',
      primary_guild: { identity_enabled: true, tag: 'ab', badge: 2, color: 0 },
    });
    expect(u).toMatchObject({ tag: 'AB', tagBadge: 2 });
    expect(toLoungeUser({ id: '9', username: 'ann' }).tag).toBeUndefined();
  });

  it('draws the badge and tag, and nothing without a tag', async () => {
    const { container, rerender } = render(
      <p>
        Ann <ServerTagChip user={{ tag: 'BK', tagBadge: 1, tagColor: 0xff0000 }} />
      </p>,
    );
    const chip = screen.getByTitle('Server tag BK');
    expect(chip).toHaveTextContent('♥ BK');
    expect(await axeViolations(container)).toEqual([]);
    rerender(
      <p>
        Ann <ServerTagChip user={{}} />
      </p>,
    );
    expect(screen.queryByTitle(/Server tag/)).toBeNull();
  });
});

describe('leveling (FTR-LVL-001)', () => {
  const card = {
    user_id: '1',
    username: 'me',
    display_name: 'Me Myself',
    xp: 420,
    level: 4,
    level_xp: 20,
    level_span: 100,
    rank: 2,
    messages: 31,
    opted_out: false,
    // G-70: the rank card carries the server's level roles for every member.
    level_roles: [
      { level: 5, role_id: 'r5' },
      { level: 2, role_id: 'r2' },
    ],
  };
  const board = [
    {
      rank: 1,
      user_id: '2',
      username: 'bob',
      display_name: 'Bob',
      avatar_key: null,
      xp: 900,
      level: 7,
    },
    {
      rank: 2,
      user_id: '1',
      username: 'me',
      display_name: 'Me Myself',
      avatar_key: null,
      xp: 420,
      level: 4,
    },
  ];

  it('shows the rank card, the level roles and the leaderboard, and opts out after a warning', async () => {
    const puts: unknown[] = [];
    server.use(
      http.get('*/api/v1/guilds/g1/leveling/rank/:who', () => HttpResponse.json(card)),
      http.get('*/api/v1/guilds/g1/leveling/leaderboard', () => HttpResponse.json(board)),
      http.put('*/api/v1/users/@me/guilds/g1/leveling', async ({ request }) => {
        puts.push(await request.json());
        return new HttpResponse(null, { status: 204 });
      }),
    );
    useStore.setState((s) => ({
      roles: {
        ...s.roles,
        r5: { id: 'r5', guild_id: 'g1', name: 'Regular', color: 0, position: 1, permissions: '0' },
      },
      roleIdsByGuild: { g1: [...(s.roleIdsByGuild.g1 ?? []), 'r5'] },
    }));
    const { container } = render(<Leveling guildId="g1" />);
    expect(await screen.findByText('Level 4, rank #2, 420 XP')).toBeInTheDocument();
    expect(screen.getByRole('progressbar', { name: 'Progress to level 5' })).toHaveAttribute(
      'aria-valuenow',
      '20',
    );
    const rows = within(await screen.findByRole('list', { name: 'Leaderboard' })).getAllByRole(
      'listitem',
    );
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('Bob');
    // Level roles come sorted, with the one already reached marked.
    const rewards = await screen.findByRole('region', { name: 'Level roles' });
    expect(within(rewards).getAllByRole('listitem')[0]).toHaveTextContent('Level 2: r2 (earned)');
    expect(within(rewards).getAllByRole('listitem')[1]).toHaveTextContent('Level 5: Regular');
    expect(await axeViolations(container)).toEqual([]);

    await userEvent.click(screen.getByRole('button', { name: 'Opt out of leveling' }));
    expect(puts).toEqual([]);
    expect(screen.getByText(/deletes your XP/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Opt out and delete my XP' }));
    await waitFor(() => expect(puts).toEqual([{ enabled: false }]));
  });

  it('offers to take part again when opted out', async () => {
    server.use(
      http.get('*/api/v1/guilds/g1/leveling/rank/:who', () =>
        HttpResponse.json({ ...card, opted_out: true, rank: null, xp: 0, level_roles: [] }),
      ),
      http.get('*/api/v1/guilds/g1/leveling/leaderboard', () => HttpResponse.json([])),
    );
    render(<Leveling guildId="g1" />);
    expect(await screen.findByText(/You opted out of leveling/)).toBeInTheDocument();
    expect(screen.getByText('Nobody has earned XP yet.')).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Level roles' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Take part again' })).toBeInTheDocument();
  });
});

describe('suggestions (FTR-SUGG-001)', () => {
  const sugg = (id: string, over: Record<string, unknown> = {}) => ({
    id,
    channel_id: 'c1',
    message_id: `m${id}`,
    author_id: '2',
    content: `Idea ${id}`,
    status: 'open',
    response: '',
    responder_id: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    votes: { up: 4, down: 1 },
    my_vote: null,
    ...over,
  });

  it('submits, shows votes and statuses, and lets staff respond', async () => {
    const posts: unknown[] = [];
    const patches: unknown[] = [];
    server.use(
      modules(['community.suggestions'], ['community.suggestions']),
      http.get('*/api/v1/guilds/g1/suggestions', () =>
        HttpResponse.json([
          sugg('1', { my_vote: 'up' }),
          sugg('2', { status: 'approved', response: 'Soon!' }),
        ]),
      ),
      http.post('*/api/v1/guilds/g1/suggestions', async ({ request }) => {
        posts.push(await request.json());
        return HttpResponse.json(sugg('3'));
      }),
      http.patch('*/api/v1/guilds/g1/suggestions/1', async ({ request }) => {
        patches.push(await request.json());
        return HttpResponse.json(sugg('1', { status: 'considering' }));
      }),
    );
    const { container } = render(<Suggestions guildId="g1" />);
    expect(await screen.findByText('Idea 1')).toBeInTheDocument();
    // The counts and the viewer's own vote come with the suggestions; nothing counts reactions any more.
    expect(await screen.findAllByText(/4 up, 1 down/)).toHaveLength(2);
    expect(screen.getByText(/\(you voted up\)/)).toBeInTheDocument();
    expect(screen.getByText('Soon!')).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);

    await userEvent.type(screen.getByLabelText('Your suggestion'), 'More emoji');
    await userEvent.click(screen.getByRole('button', { name: 'Post suggestion' }));
    await waitFor(() => expect(posts).toEqual([{ content: 'More emoji' }]));

    const row = screen.getByText('Idea 1').closest('li')!;
    await userEvent.click(
      await within(row).findByRole('button', { name: 'Set status or respond' }),
    );
    await userEvent.click(within(row).getByRole('combobox', { name: 'Status' }));
    await userEvent.click(screen.getByRole('option', { name: 'Considering' }));
    await userEvent.type(within(row).getByLabelText('Response'), 'Looking into it');
    await userEvent.click(within(row).getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(patches).toEqual([{ status: 'considering', response: 'Looking into it' }]),
    );
  });

  it('hides the staff controls from ordinary members', async () => {
    asMember();
    server.use(
      modules(['community.suggestions']),
      http.get('*/api/v1/guilds/g1/suggestions', () => HttpResponse.json([sugg('1')])),
    );
    render(<Suggestions guildId="g1" />);
    expect(await screen.findByText('Idea 1')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Set status or respond' })).toBeNull();
  });

  it('shows the respond controls to a member the server lets respond, even without Manage Server', async () => {
    asMember();
    server.use(
      modules(['community.suggestions'], ['community.suggestions']),
      http.get('*/api/v1/guilds/g1/suggestions', () => HttpResponse.json([sugg('1')])),
    );
    render(<Suggestions guildId="g1" />);
    expect(await screen.findByText('Idea 1')).toBeInTheDocument();
    expect(
      await screen.findByRole('button', { name: 'Set status or respond' }),
    ).toBeInTheDocument();
  });
});

describe('tickets (FTR-TICK-001)', () => {
  const ticket = (id: string, over: Record<string, unknown> = {}) => ({
    id,
    opener_id: '2',
    thread_id: `t${id}`,
    subject: `Problem ${id}`,
    status: 'open',
    closed_by: null,
    closed_at: null,
    created_at: '2026-01-01T00:00:00Z',
    ...over,
  });

  it('opens a ticket and goes to its thread', async () => {
    const posts: unknown[] = [];
    const onNavigate = vi.fn();
    server.use(
      http.get('*/api/v1/guilds/g1/tickets', () => HttpResponse.json([])),
      http.post('*/api/v1/guilds/g1/tickets', async ({ request }) => {
        posts.push(await request.json());
        return HttpResponse.json(ticket('9', { opener_id: '1' }));
      }),
    );
    asMember();
    const { container } = render(<Tickets guildId="g1" onNavigate={onNavigate} />);
    expect(await screen.findByText('No tickets.')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Your tickets' })).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.type(screen.getByLabelText('What do you need help with?'), 'Cannot log in');
    await userEvent.click(screen.getByRole('button', { name: 'Open a ticket' }));
    await waitFor(() => expect(posts).toEqual([{ subject: 'Cannot log in' }]));
    await waitFor(() => expect(goto).toHaveBeenCalledWith('g1', 't9'));
    expect(onNavigate).toHaveBeenCalled();
  });

  it("shows staff everyone's tickets and closes one", async () => {
    const closed: string[] = [];
    server.use(
      modules(['community.tickets'], ['community.tickets']),
      http.get('*/api/v1/guilds/g1/tickets', () =>
        HttpResponse.json([ticket('1'), ticket('2', { status: 'closed' })]),
      ),
      http.post('*/api/v1/guilds/g1/tickets/1/close', () => {
        closed.push('1');
        return HttpResponse.json(ticket('1', { status: 'closed' }));
      }),
    );
    render(<Tickets guildId="g1" onNavigate={() => undefined} />);
    expect(await screen.findByRole('heading', { name: 'All tickets' })).toBeInTheDocument();
    const rows = await screen.findAllByRole('listitem');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('opened by Bob');
    // A closed ticket has neither action.
    expect(within(rows[1]!).queryByRole('button')).toBeNull();
    await userEvent.click(within(rows[0]!).getByRole('button', { name: 'Close ticket' }));
    await waitFor(() => expect(closed).toEqual(['1']));
  });
});

describe('community dialog', () => {
  it('lists only the tabs whose module is on and opens the requested one', async () => {
    server.use(
      modules(['community.suggestions', 'community.birthdays']),
      http.get('*/api/v1/guilds/g1/suggestions', () => HttpResponse.json([])),
      http.get('*/api/v1/guilds/g1/birthdays', () =>
        HttpResponse.json([{ user_id: '2', month: 3, day: 9 }]),
      ),
    );
    render(<CommunityDialog guildId="g1" tab="birthdays" onClose={() => undefined} />);
    const tabs = await screen.findAllByRole('tab');
    expect(tabs.map((t) => t.textContent)).toEqual(['Suggestions', 'Birthdays']);
    expect(await screen.findByText('Bob')).toBeInTheDocument();
    expect(screen.getByText('March 9')).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('says so when no member feature is on', async () => {
    server.use(modules([]));
    render(<CommunityDialog guildId="g1" onClose={() => undefined} />);
    expect(await screen.findByText(/has not turned on any community features/)).toBeInTheDocument();
  });

  it('has /rank, /suggest and /ticket commands that open it', () => {
    const ctx = { guild: true, threads: true, tts: true };
    expect(runSlash('/rank', ctx)).toEqual({ kind: 'community', tab: 'rank' });
    expect(runSlash('/suggest', ctx)).toEqual({ kind: 'community', tab: 'suggestions' });
    expect(runSlash('/ticket', ctx)).toEqual({ kind: 'community', tab: 'tickets' });
    expect(runSlash('/rank', { ...ctx, guild: false }).kind).toBe('error');
  });
});

describe('birthday card (FTR-BDAY-001)', () => {
  it('knows the days of each month', () => {
    expect([daysIn(1), daysIn(2), daysIn(4)]).toEqual([31, 29, 30]);
  });

  it('saves month, day and privacy, and removes the birthday', async () => {
    const puts: unknown[] = [];
    let removed = 0;
    server.use(
      http.get('*/api/v1/users/@me/birthday', () => HttpResponse.json({}, { status: 404 })),
      http.put('*/api/v1/users/@me/birthday', async ({ request }) => {
        const b = (await request.json()) as Record<string, unknown>;
        puts.push(b);
        return HttpResponse.json(b);
      }),
      http.delete('*/api/v1/users/@me/birthday', () => {
        removed += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const { container } = render(<BirthdayCard />);
    await userEvent.click(await screen.findByRole('combobox', { name: 'Month' }));
    await userEvent.click(screen.getByRole('option', { name: 'February' }));
    await userEvent.click(screen.getByRole('combobox', { name: 'Day' }));
    await userEvent.click(screen.getByRole('option', { name: '14' }));
    await userEvent.click(screen.getByRole('checkbox', { name: /Let servers show my birthday/ }));
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole('button', { name: 'Save birthday' }));
    await waitFor(() =>
      expect(puts).toEqual([{ month: 2, day: 14, year: null, visibility: 'hidden' }]),
    );
    expect(await screen.findByText('Birthday saved.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Remove birthday' }));
    await waitFor(() => expect(removed).toBe(1));
  });

  it('shows a saved birthday and rejects a nonsense year', async () => {
    server.use(
      http.get('*/api/v1/users/@me/birthday', () =>
        HttpResponse.json({ month: 12, day: 25, year: 1990, visibility: 'servers' }),
      ),
    );
    render(<BirthdayCard />);
    expect(await screen.findByRole('combobox', { name: 'Month' })).toHaveTextContent('December');
    const year = screen.getByLabelText('Birth year (optional)');
    expect(year).toHaveValue('1990');
    await userEvent.clear(year);
    await userEvent.type(year, '1800');
    expect(screen.getByText('Enter a year from 1900 to 2100.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save birthday' })).toBeDisabled();
  });
});

describe('poll templates in the poll modal (FTR-POLLT-001)', () => {
  const templates = [
    {
      id: 'p1',
      name: 'Movie night',
      question: 'Which movie?',
      answers: ['Alien', 'Arrival', 'Aliens'],
      allow_multiselect: true,
      duration_hours: 12,
      created_by: '1',
      created_at: '2026-01-01T00:00:00Z',
    },
  ];

  it('fills the form from a template and saves the form as a new one', async () => {
    const posts: unknown[] = [];
    server.use(
      modules(['chat.poll_templates']),
      http.get('*/api/v1/guilds/g1/poll-templates', () => HttpResponse.json(templates)),
      http.post('*/api/v1/guilds/g1/poll-templates', async ({ request }) => {
        posts.push(await request.json());
        return HttpResponse.json({ ...templates[0], id: 'p2', name: 'Saved' });
      }),
    );
    function Host() {
      const [open] = useState(true);
      return (
        <PollModal open={open} onClose={() => undefined} onCreate={() => undefined} guildId="g1" />
      );
    }
    render(<Host />);
    await userEvent.click(await screen.findByRole('combobox', { name: 'Start from a template' }));
    await userEvent.click(screen.getByRole('option', { name: 'Movie night' }));
    expect(screen.getByLabelText('Question')).toHaveValue('Which movie?');
    expect(screen.getByLabelText('Answer 3')).toHaveValue('Aliens');
    expect(screen.getByRole('checkbox', { name: /multiple/i })).toBeChecked();
    expect(screen.getByRole('combobox', { name: /Duration/ })).toHaveValue('12');
    expect(await axeViolations(document.body)).toEqual([]);

    await userEvent.type(screen.getByLabelText('Save this poll as a template named'), 'Saved');
    await userEvent.click(screen.getByRole('button', { name: 'Save template' }));
    await waitFor(() =>
      expect(posts).toEqual([
        {
          name: 'Saved',
          question: 'Which movie?',
          answers: ['Alien', 'Arrival', 'Aliens'],
          allow_multiselect: true,
          duration_hours: 12,
        },
      ]),
    );
  });

  it('offers nothing when the module is off', async () => {
    server.use(modules([]));
    render(<PollModal open onClose={() => undefined} onCreate={() => undefined} guildId="g1" />);
    await screen.findByLabelText('Question');
    await waitFor(() => expect(useModulesStore.getState().byGuild.g1).toBeDefined());
    expect(screen.queryByRole('combobox', { name: 'Start from a template' })).toBeNull();
  });
});

describe('rss feed manager (FTR-RSS-001)', () => {
  const feed = (over: Record<string, unknown> = {}) => ({
    id: 'f1',
    channel_id: 'c1',
    url: 'https://example.com/feed.xml',
    title: 'Example blog',
    include_keywords: [],
    exclude_keywords: [],
    poll_interval_secs: 3600,
    enabled: true,
    last_checked_at: '2026-01-01T00:00:00Z',
    last_status: 'ok',
    failures: 0,
    created_at: '2026-01-01T00:00:00Z',
    ...over,
  });

  it('splits keywords on commas', () => {
    expect(keywordsOf(' a, b ,, c ')).toEqual(['a', 'b', 'c']);
    expect(keywordsOf('')).toEqual([]);
  });

  it('lists feeds, adds one, pauses and removes it', async () => {
    const adds: unknown[] = [];
    const patches: unknown[] = [];
    let removed = 0;
    server.use(
      http.get('*/api/v1/guilds/g1/rss-feeds', () => HttpResponse.json([feed()])),
      http.post('*/api/v1/guilds/g1/rss-feeds', async ({ request }) => {
        adds.push(await request.json());
        return HttpResponse.json(feed({ id: 'f2' }));
      }),
      http.patch('*/api/v1/guilds/g1/rss-feeds/f1', async ({ request }) => {
        patches.push(await request.json());
        return HttpResponse.json(feed({ enabled: false }));
      }),
      http.delete('*/api/v1/guilds/g1/rss-feeds/f1', () => {
        removed += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const { container } = render(<RssPanel guildId="g1" />);
    expect(await screen.findByText('Example blog')).toBeInTheDocument();
    expect(screen.getByText(/posts in #general/)).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);

    await userEvent.type(screen.getByLabelText('Feed address'), 'https://example.org/atom');
    await userEvent.click(screen.getByRole('combobox', { name: 'Post in channel' }));
    await userEvent.click(screen.getByRole('option', { name: 'random' }));
    await userEvent.type(screen.getByLabelText('Only items containing'), 'rust, wasm');
    await userEvent.click(screen.getByRole('button', { name: 'Follow feed' }));
    await waitFor(() =>
      expect(adds).toEqual([
        {
          url: 'https://example.org/atom',
          channel_id: 'c2',
          include_keywords: ['rust', 'wasm'],
          exclude_keywords: [],
          poll_interval_secs: 3600,
        },
      ]),
    );

    await userEvent.click(screen.getByRole('switch', { name: /Post new items from Example blog/ }));
    await waitFor(() => expect(patches).toEqual([{ enabled: false }]));
    await userEvent.click(screen.getByRole('button', { name: 'Stop following Example blog' }));
    await waitFor(() => expect(removed).toBe(1));
  });

  it("shows the server's refusal of an unsafe address", async () => {
    server.use(
      http.get('*/api/v1/guilds/g1/rss-feeds', () => HttpResponse.json([])),
      http.post('*/api/v1/guilds/g1/rss-feeds', () =>
        HttpResponse.json({ message: 'That address is not allowed.' }, { status: 400 }),
      ),
    );
    render(<RssPanel guildId="g1" />);
    await screen.findByText('This server follows no feeds yet.');
    await userEvent.type(screen.getByLabelText('Feed address'), 'http://127.0.0.1/feed');
    await userEvent.click(screen.getByRole('combobox', { name: 'Post in channel' }));
    await userEvent.click(screen.getByRole('option', { name: 'general' }));
    await userEvent.click(screen.getByRole('button', { name: 'Follow feed' }));
    expect(await screen.findByText(/not allowed/)).toBeInTheDocument();
  });
});

describe('reaction roles message picker (FTR-RR-001)', () => {
  it("loads a channel's recent messages and returns the chosen id", async () => {
    server.use(
      http.get('*/api/v1/channels/c1/messages', () =>
        HttpResponse.json([
          {
            id: '501',
            content: 'React below to pick a colour',
            timestamp: '2026-01-01T00:00:00Z',
            author: { username: 'bob', global_name: 'Bob' },
          },
          {
            id: '500',
            content: '',
            timestamp: '2026-01-01T00:00:00Z',
            author: { username: 'me' },
          },
        ]),
      ),
    );
    const picked: string[] = [];
    const { container } = render(
      <MessagePicker guildId="g1" label="Message id" value="" onChange={(id) => picked.push(id)} />,
    );
    await userEvent.click(screen.getByRole('combobox', { name: 'Channel' }));
    await userEvent.click(screen.getByRole('option', { name: 'general' }));
    await userEvent.click(await screen.findByRole('combobox', { name: 'Message' }));
    const options = await screen.findAllByRole('option');
    expect(options[0]).toHaveTextContent('Bob: React below to pick a colour');
    expect(options[1]).toHaveTextContent('me: (no text)');
    await userEvent.click(options[0]!);
    expect(picked).toEqual(['501']);
    expect(await axeViolations(container)).toEqual([]);
  });

  it('keeps an already chosen message visible before any channel is opened', () => {
    render(<MessagePicker guildId="g1" label="Message id" value="77" onChange={() => undefined} />);
    expect(screen.getByRole('combobox', { name: 'Message' })).toHaveValue('Message 77 (chosen)');
  });

  it('is used for the message field of reaction-role panels in Server Settings', async () => {
    const schema = {
      fields: [
        {
          key: 'panels',
          label: 'Reaction role messages',
          type: 'list',
          max_items: 5,
          item: {
            type: 'group',
            fields: [{ key: 'message', label: 'Message id', type: 'string', max_len: 20 }],
          },
        },
      ],
    };
    server.use(
      http.get('*/api/v1/guilds/g1/sr/features', () =>
        HttpResponse.json([
          {
            id: 'lounge.reaction_roles',
            name: 'Reaction roles',
            description: 'Members pick roles by reacting.',
            section: 'Lounge',
            phase: 11,
            enabled: true,
            default_enabled: false,
            platform_disabled: false,
            settings: { panels: [{ message: '77' }] },
            schema,
          },
        ]),
      ),
    );
    render(<FeaturesSection guildId="g1" viewer={{} as never} />);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Show settings for Reaction roles' }),
    );
    expect(await screen.findByRole('combobox', { name: 'Channel' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Message' })).toHaveValue('Message 77 (chosen)');
  });
});
