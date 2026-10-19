import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const announcer = (over: object = {}) => ({
  id: 'community.watch_party_announcer',
  name: 'Watch party announcer',
  description: 'Announce new watch parties.',
  section: 'Community',
  phase: 11,
  enabled: false,
  default_enabled: false,
  platform_disabled: false,
  settings: {},
  schema: {
    fields: [{ key: 'channel', label: 'Announce in', type: 'channel_picker', multi: false }],
  },
  ...over,
});
const other = { ...announcer(), id: 'lounge.starboard', name: 'Starboard' };

describe('Server settings > Watch Parties & Library', () => {
  it('is hidden without Manage Server', async () => {
    seed({ perms: [MANAGE.channels] });
    server.use(http.get('*/api/v1/guilds/g1/sr/features', () => HttpResponse.json([])));
    await openSettings('channels');
    expect(screen.queryByRole('button', { name: 'Watch Parties & Library' })).toBeNull();
  });

  it('explains where parties live and links to channels only with Manage Channels (D-264)', async () => {
    seed({ perms: [MANAGE.guild, MANAGE.channels] });
    server.use(http.get('*/api/v1/guilds/g1/sr/features', () => HttpResponse.json([announcer()])));
    await openSettings('watch-library');
    expect(await screen.findByRole('heading', { name: 'Where parties live' })).toBeInTheDocument();
    expect(
      screen.getByRole('link', { name: 'Open channels to create a Watch party room' }),
    ).toBeInTheDocument();
  });

  it('hides the channels link without Manage Channels', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(http.get('*/api/v1/guilds/g1/sr/features', () => HttpResponse.json([announcer()])));
    await openSettings('watch-library');
    expect(await screen.findByRole('heading', { name: 'Where parties live' })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /Watch party room/ })).toBeNull();
  });

  it('shows only the two modules and saves the announce channel', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/sr/features', () => HttpResponse.json([announcer(), other])),
      http.patch(
        '*/api/v1/guilds/g1/sr/features/community.watch_party_announcer',
        async ({ request }) => {
          await cap.record(request);
          return HttpResponse.json(announcer({ enabled: true }));
        },
      ),
    );
    await openSettings('watch-library');
    expect(
      await screen.findByRole('heading', { name: 'Who can start a watch party' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Watch party announcer' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Starboard' })).toBeNull();
    await userEvent.click(screen.getByRole('switch', { name: 'Enable Watch party announcer' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ enabled: true });
  });
});
