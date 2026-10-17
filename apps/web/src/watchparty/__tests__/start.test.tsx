// "Watch together": the destination picker (WP-FE-008/009), the title picker used when the call bar starts a
// party, and the channel card (WP-FE-010).
import { ThemeProvider } from '@sideroom/ui';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { emitDispatch } from '../../store/events';
import { http, HttpResponse, server } from '../../test/server';
import { StartPartyDialog, type StartPartyDialogProps } from '../StartPartyDialog';
import { WatchPartyStrip } from '../WatchPartyStrip';
import { partyIdOf, PartyMessageCard } from '../PartyMessageCard';
import { FakeServer, person } from './fakes';

vi.mock('../../store/connect', async (orig) => ({
  ...(await orig<typeof import('../../store/connect')>()),
  holdGateway: () => () => undefined,
}));

const ch = (id: string, name: string, type: string, extra: object = {}) => ({
  id,
  guildId: '5',
  type,
  name,
  position: 0,
  unread: false,
  mentionCount: 0,
  canSend: true,
  ...extra,
});

vi.mock('../../lounge/data', () => ({
  useGuilds: () => [{ id: '5', name: 'Side Room HQ' }],
  useAllChannels: () => ({
    '5': [
      ch('50', 'general', 'text'),
      ch('51', 'house-notes', 'announcement', { canSend: false }),
      ch('52', 'Movie night', 'voice'),
      ch('53', 'staff', 'text', { locked: true }),
    ],
  }),
  useDms: () => [
    { id: '60', recipientIds: ['2'], lastMessageAt: '', unread: false, mentionCount: 0 },
  ],
  useUsers: () => ({ '2': { id: '2', username: 'ana', displayName: 'Ana' } }),
}));

function mount(ui: React.ReactNode) {
  const root = createRootRoute({
    component: () => <ThemeProvider section="cinema">{ui}</ThemeProvider>,
  });
  const history = createMemoryHistory({ initialEntries: ['/'] });
  const router = createRouter({ routeTree: root, history });
  const view = render(<RouterProvider router={router} />);
  return { history, ...view };
}

const created: unknown[] = [];
function onCreate(status = 201, body: Record<string, unknown> = { id: '900' }) {
  created.length = 0;
  server.use(
    http.post('*/api/v1/watch-parties', async ({ request }) => {
      created.push(await request.json());
      return HttpResponse.json(body, { status });
    }),
  );
}

const withTitle: StartPartyDialogProps['title'] = {
  titleId: '10',
  episodeId: null,
  variantId: null,
  label: 'Akira',
};

describe('StartPartyDialog with a title', () => {
  it('offers private, the channels you can post in, and DMs, and passes axe', async () => {
    const onClose = vi.fn();
    const view = mount(<StartPartyDialog open onClose={onClose} title={withTitle} />);
    const where = await screen.findByRole('combobox', { name: 'Where to watch' });
    const labels = within(where)
      .getAllByRole('option')
      .map((o) => o.textContent);
    expect(labels).toEqual([
      'Private party (invite link)',
      '#general',
      'Voice: Movie night',
      'Ana',
    ]);
    // Announcement channel you cannot post in, and the locked channel, are not offered.
    expect(labels.join()).not.toMatch(/house-notes|staff/);
    const found = await axeViolations(view.baseElement);
    expect(found.map((v) => JSON.stringify(v).slice(0, 300))).toEqual([]);
  });

  it('starts a private party and opens the theatre', async () => {
    onCreate();
    const onClose = vi.fn();
    const { history } = mount(<StartPartyDialog open onClose={onClose} title={withTitle} />);
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Start party' }));
    await waitFor(() => expect(history.location.pathname).toBe('/cinema/party/900'));
    expect(created).toEqual([
      { title_id: '10', episode_id: null, variant_id: null, channel_id: null },
    ]);
    expect(onClose).toHaveBeenCalled();
  });

  it('binds the party to the chosen channel', async () => {
    onCreate();
    mount(<StartPartyDialog open onClose={() => undefined} title={withTitle} />);
    const user = userEvent.setup();
    await user.selectOptions(await screen.findByRole('combobox', { name: 'Where to watch' }), '50');
    await user.click(screen.getByRole('button', { name: 'Start party' }));
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toMatchObject({ channel_id: '50' });
  });

  it('preselects the voice channel of the call bar', async () => {
    onCreate();
    mount(<StartPartyDialog open onClose={() => undefined} title={withTitle} channelId="52" />);
    expect(await screen.findByRole('combobox', { name: 'Where to watch' })).toHaveValue('52');
  });

  it('explains a refusal and keeps the dialog open', async () => {
    onCreate(403, { code: 50013, message: 'Missing Permissions' });
    const onClose = vi.fn();
    mount(<StartPartyDialog open onClose={onClose} title={withTitle} channelId="50" />);
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Start party' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/cannot start a watch party there/);
    expect(onClose).not.toHaveBeenCalled();
  });

  it('shows the server message when the channel already has a party', async () => {
    onCreate(409, { code: 900010, message: 'This channel already has a watch party.' });
    mount(<StartPartyDialog open onClose={() => undefined} title={withTitle} channelId="50" />);
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Start party' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'This channel already has a watch party.',
    );
  });

  it('asks for age confirmation outside the Cinema, where there is no gate dialog', async () => {
    onCreate(403, { code: 900011, message: 'age' });
    mount(<StartPartyDialog open onClose={() => undefined} title={withTitle} />);
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Start party' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/18 or older/);
  });
});

describe('StartPartyDialog for a Live TV channel (G-16)', () => {
  it('starts without asking for a title and sends the channel', async () => {
    onCreate();
    const { history } = mount(
      <StartPartyDialog
        open
        onClose={() => undefined}
        liveChannel={{ channelId: '3', label: 'Channel 101, Toonami' }}
        channelId="52"
      />,
    );
    expect(await screen.findByText('Start a party for Channel 101, Toonami.')).toBeInTheDocument();
    expect(screen.queryByLabelText('Search the cinema')).toBeNull();
    const start = screen.getByRole('button', { name: 'Start party' });
    expect(start).toBeEnabled();
    await userEvent.setup().click(start);
    await waitFor(() => expect(history.location.pathname).toBe('/cinema/party/900'));
    expect(created).toEqual([{ tv_channel_id: '3', channel_id: '52' }]);
  });
});

describe('StartPartyDialog without a title (the voice call bar)', () => {
  it('searches the cinema, picks a film and starts the party in the call channel', async () => {
    onCreate();
    server.use(
      http.get('*/api/v1/cinema/search', ({ request }) => {
        expect(new URL(request.url).searchParams.get('q')).toBe('aki');
        return HttpResponse.json({
          titles: [
            { id: '10', kind: 'movie', title: 'Akira', year: 1988 },
            { id: '11', kind: 'series', title: 'Akira: Origins', season_count: 2 },
          ],
        });
      }),
    );
    const { history } = mount(<StartPartyDialog open onClose={() => undefined} channelId="52" />);
    const user = userEvent.setup();
    expect(await screen.findByRole('button', { name: 'Start party' })).toBeDisabled();
    await user.type(await screen.findByLabelText('Search the cinema'), 'aki');
    await user.click(await screen.findByRole('button', { name: 'Select Akira' }));
    expect(screen.getByRole('button', { name: 'Start party' })).toBeEnabled();
    await user.click(screen.getByRole('button', { name: 'Start party' }));
    await waitFor(() => expect(history.location.pathname).toBe('/cinema/party/900'));
    expect(created[0]).toEqual({
      title_id: '10',
      episode_id: null,
      variant_id: null,
      channel_id: '52',
    });
  });

  it('picks an episode of a series', async () => {
    onCreate();
    server.use(
      http.get('*/api/v1/cinema/search', () =>
        HttpResponse.json({
          titles: [{ id: '11', kind: 'series', title: 'Dark', season_count: 2 }],
        }),
      ),
      http.get('*/api/v1/cinema/titles/11/episodes', () =>
        HttpResponse.json({
          episodes: [
            { id: '111', season_number: 1, number: 1, title: 'Secrets' },
            { id: '112', season_number: 1, number: 2, title: 'Lies' },
          ],
        }),
      ),
      http.get('*/api/v1/cinema/titles/11/watch-state', () => HttpResponse.json({ items: [] })),
    );
    mount(<StartPartyDialog open onClose={() => undefined} />);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('Search the cinema'), 'dark');
    await user.click(await screen.findByRole('button', { name: 'Choose an episode of Dark' }));
    await user.click(await screen.findByRole('button', { name: 'Select season 1 episode 2' }));
    await user.click(screen.getByRole('button', { name: 'Start party' }));
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toMatchObject({ title_id: '11', episode_id: '112' });
  });
});

describe('WatchPartyStrip', () => {
  it('shows a live party in the channel, updates from the gateway and joins', async () => {
    const fake = new FakeServer(() => Date.now(), '1', { host: false });
    fake.party = {
      ...fake.party,
      binding: 'channel',
      channel_id: '50',
      guild_id: '5',
      participants: [person('1'), person('2', { is_host: true })],
    };
    server.use(
      http.get('*/api/v1/channels/50/watch-party', () =>
        HttpResponse.json({ ...fake.party, playback: fake.view() }),
      ),
    );
    const { history } = mount(<WatchPartyStrip channelId="50" />);
    const strip = await screen.findByRole('complementary', { name: 'Watch party' });
    expect(strip).toHaveTextContent('Akira');
    expect(strip).toHaveTextContent('2 watching');

    fake.party = { ...fake.party, participants: [...fake.party.participants, person('3')] };
    emitDispatch('WATCH_PARTY_UPDATE', {
      type: 'participant_joined',
      party: { ...fake.party, playback: fake.view() },
    });
    await waitFor(() => expect(strip).toHaveTextContent('3 watching'));

    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Join the watch party for Akira' }));
    expect(history.location.pathname).toBe('/cinema/party/900');

    emitDispatch('WATCH_PARTY_UPDATE', {
      type: 'ended',
      party: { ...fake.party, ended: true, playback: fake.view() },
    });
    await waitFor(() =>
      expect(screen.queryByRole('complementary', { name: 'Watch party' })).not.toBeInTheDocument(),
    );
  });

  it('renders nothing for a channel without a party', async () => {
    server.use(
      http.get('*/api/v1/channels/51/watch-party', () =>
        HttpResponse.json({ message: 'none' }, { status: 404 }),
      ),
    );
    mount(<WatchPartyStrip channelId="51" />);
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByRole('complementary', { name: 'Watch party' })).not.toBeInTheDocument();
  });
});

describe('PartyMessageCard (WP-FE-010)', () => {
  const card = (channelId: string, id = '900') => ({
    channelId,
    embeds: [
      {
        type: 'rich',
        title: 'Akira',
        description: 'S1 E1 · 1 watching',
        url: `/cinema/party/${id}`,
      },
    ],
  });

  it('reads the party id from the card link', () => {
    expect(partyIdOf('/cinema/party/900')).toBe('900');
    expect(partyIdOf('https://x.test/cinema/party/12?code=a')).toBe('12');
    expect(partyIdOf('https://x.test/other')).toBeNull();
    expect(partyIdOf(undefined)).toBeNull();
  });

  it('follows the live party while it runs, joins, and falls back to the snapshot when it ended', async () => {
    const fake = new FakeServer(() => Date.now(), '1', { host: false });
    fake.party = {
      ...fake.party,
      binding: 'channel',
      channel_id: '50',
      guild_id: '5',
      participants: [person('1'), person('2', { is_host: true })],
    };
    server.use(
      http.get('*/api/v1/channels/50/watch-party', () =>
        HttpResponse.json({ ...fake.party, playback: fake.view() }),
      ),
    );
    const { history } = mount(<PartyMessageCard message={card('50')} />);
    const el = await screen.findByRole('complementary', { name: 'Watch party: Akira' });
    await waitFor(() => expect(el).toHaveTextContent('2 watching'));
    expect(el).toHaveAttribute('data-live', 'true');
    // The headcount comes from the gateway, not from the message snapshot.
    fake.party = { ...fake.party, participants: [...fake.party.participants, person('3')] };
    emitDispatch('WATCH_PARTY_UPDATE', {
      type: 'participant_joined',
      party: { ...fake.party, playback: fake.view() },
    });
    await waitFor(() => expect(el).toHaveTextContent('3 watching'));
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Join the watch party for Akira' }));
    expect(history.location.pathname).toBe('/cinema/party/900');
    // After the end the card shows the server's snapshot and no Join button.
    emitDispatch('WATCH_PARTY_UPDATE', {
      type: 'ended',
      party: { ...fake.party, ended: true, playback: fake.view() },
    });
    await waitFor(() => expect(el).toHaveAttribute('data-live', 'false'));
    expect(el).toHaveTextContent('Akira');
    expect(el).toHaveTextContent('S1 E1 · 1 watching');
    expect(el).toHaveTextContent('Ended');
    expect(screen.queryByRole('button', { name: /Join the watch party/ })).not.toBeInTheDocument();
  });

  it('is a snapshot of an older party when the channel has another one now', async () => {
    const fake = new FakeServer(() => Date.now(), '1', { host: false });
    fake.party = { ...fake.party, binding: 'channel', channel_id: '50', guild_id: '5' };
    server.use(
      http.get('*/api/v1/channels/50/watch-party', () =>
        HttpResponse.json({ ...fake.party, playback: fake.view() }),
      ),
    );
    mount(<PartyMessageCard message={card('50', '123')} />);
    const el = await screen.findByRole('complementary', { name: 'Watch party: Akira' });
    expect(el).toHaveAttribute('data-live', 'false');
    expect(screen.queryByRole('button', { name: /Join the watch party/ })).not.toBeInTheDocument();
  });
});
