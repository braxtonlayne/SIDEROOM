import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const guildObj = {
  id: 'g1',
  name: 'Book Club',
  icon: null,
  description: 'Readers',
  owner_id: '9',
  system_channel_id: null,
  verification_level: 0,
  default_message_notifications: 0,
  explicit_content_filter: 0,
  preferred_locale: 'en-US',
  features: [],
};
const community = {
  guild_id: 'g1',
  community_enabled: true,
  rules_channel_id: null,
  public_updates_channel_id: null,
  safety_alerts_channel_id: null,
  description: null,
  preferred_locale: 'en-US',
  onboarding_enabled: false,
  welcome_screen_enabled: true,
  guide_enabled: false,
  invites_paused: false,
  raid_alert_joins_per_minute: 0,
  age_restricted: false,
};

describe('Overview', () => {
  it('loads, saves only changed fields, and shows community flags read-only', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1', () => HttpResponse.json(guildObj)),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community)),
      http.patch('*/api/v1/guilds/g1', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json({ ...guildObj, name: 'Book Club 2' });
      }),
    );
    await openSettings('overview');
    const name = await screen.findByLabelText('Server name');
    expect(name).toHaveValue('Book Club');
    await waitFor(() =>
      expect(screen.getByText('Welcome screen').nextSibling).toHaveTextContent('On'),
    );
    expect(screen.getByRole('button', { name: 'Change server icon' })).toBeDisabled();
    await userEvent.clear(name);
    await userEvent.type(name, 'Book Club 2');
    await userEvent.click(screen.getByRole('radio', { name: 'Only @mentions' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await screen.findByText('Server settings saved.');
    expect(cap.calls[0]!.body).toEqual({ name: 'Book Club 2', default_message_notifications: 1 });
    await waitFor(() => expect(screen.queryByText(/unsaved changes/)).toBeNull());
  });

  it('validates the name and surfaces server errors', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      http.get('*/api/v1/guilds/g1', () => HttpResponse.json(guildObj)),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community)),
      http.patch('*/api/v1/guilds/g1', () =>
        HttpResponse.json({ message: 'Missing Permissions' }, { status: 403 }),
      ),
    );
    await openSettings('overview');
    const name = await screen.findByLabelText('Server name');
    await userEvent.clear(name);
    await userEvent.type(name, 'x');
    expect(
      screen.getByText('Server name must be between 2 and 100 characters.'),
    ).toBeInTheDocument();
    await userEvent.type(name, 'y');
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(await screen.findByText('You do not have permission to do that.')).toBeInTheDocument();
    expect(screen.getByText(/unsaved changes/)).toBeInTheDocument();
  });

  it('owner deletes the server only after typing its name', async () => {
    seed({ perms: [], ownerId: '1' });
    let deleted = false;
    server.use(
      http.get('*/api/v1/guilds/g1', () => HttpResponse.json({ ...guildObj, owner_id: '1' })),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community)),
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json([])),
      http.get('*/api/v1/guilds/g1/roles', () => HttpResponse.json([])),
      http.delete('*/api/v1/guilds/g1', () => {
        deleted = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('delete');
    await userEvent.click(await screen.findByRole('button', { name: 'Delete server' }));
    const confirm = screen.getAllByRole('button', { name: 'Delete server' }).at(-1)!;
    expect(confirm).toBeDisabled();
    await userEvent.type(screen.getByLabelText(/Type "Book Club" to confirm/), 'Book Club');
    await userEvent.click(confirm);
    await waitFor(() => expect(deleted).toBe(true));
  });
});
