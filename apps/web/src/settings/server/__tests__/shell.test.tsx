import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { bits, http, HttpResponse, MANAGE, openSettings, restRoles, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const guildObj = {
  id: 'g1',
  name: 'Book Club',
  icon: null,
  description: null,
  owner_id: '9',
  system_channel_id: null,
  verification_level: 0,
  default_message_notifications: 0,
  explicit_content_filter: 0,
  preferred_locale: 'en-US',
  features: [],
};

function stubOverview() {
  server.use(
    http.get('*/api/v1/guilds/g1', () => HttpResponse.json(guildObj)),
    http.get('*/api/v1/guilds/g1/sr/community', () =>
      HttpResponse.json({
        community_enabled: false,
        rules_channel_id: null,
        public_updates_channel_id: null,
        safety_alerts_channel_id: null,
        description: null,
        preferred_locale: 'en-US',
        onboarding_enabled: false,
        welcome_screen_enabled: false,
        guide_enabled: false,
        invites_paused: false,
        raid_alert_joins_per_minute: 0,
        age_restricted: false,
        guild_id: 'g1',
      }),
    ),
  );
}

const nav = () => screen.getByRole('navigation', { name: /settings/i });

describe('server settings shell', () => {
  it('lists only the sections the viewer may open, grouped', async () => {
    seed({ perms: [MANAGE.ban, MANAGE.audit] });
    server.use(http.get('*/api/v1/guilds/g1/bans', () => HttpResponse.json([])));
    await openSettings('bans');
    await screen.findByRole('heading', { name: 'Bans' });
    const names = within(nav())
      .getAllByRole('button')
      .map((b) => b.textContent);
    expect(names).toEqual(['Members', 'Bans', 'Audit Log']);
    expect(within(nav()).queryByRole('button', { name: 'Roles' })).toBeNull();
    expect(within(nav()).queryByRole('button', { name: 'Delete Server' })).toBeNull();
  });

  it('redirects a hidden section to the first visible one', async () => {
    seed({ perms: [MANAGE.ban] });
    server.use(http.get('*/api/v1/guilds/g1/bans', () => HttpResponse.json([])));
    const { router } = await openSettings('roles');
    await waitFor(() => expect(router.state.location.pathname).toBe('/settings/server/g1/members'));
  });

  it('shows no-access copy without any settings permission', async () => {
    seed({ perms: [] });
    await openSettings('overview');
    expect(
      await screen.findByText('You do not have access to server settings'),
    ).toBeInTheDocument();
  });

  it('owner sees Delete Server; section nav has a single tab stop and arrow keys', async () => {
    seed({ perms: [], ownerId: '1' });
    stubOverview();
    server.use(http.get('*/api/v1/guilds/g1/roles', () => HttpResponse.json(restRoles())));
    await openSettings('overview');
    await screen.findByRole('heading', { name: 'Server Overview' });
    const buttons = within(nav()).getAllByRole('button');
    expect(buttons.filter((b) => b.tabIndex === 0)).toHaveLength(1);
    expect(within(nav()).getByRole('button', { name: 'Delete Server' })).toBeInTheDocument();
    buttons[0]!.focus();
    await userEvent.keyboard('{ArrowDown}');
    expect(buttons[1]).toHaveFocus();
  });

  it('ESC closes back to the server; with unsaved changes it warns instead', async () => {
    seed({ perms: [MANAGE.guild] });
    stubOverview();
    const { router } = await openSettings('overview');
    const name = await screen.findByLabelText('Server name');
    await userEvent.type(name, ' 2');
    expect(await screen.findByText(/unsaved changes/)).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    expect(router.state.location.pathname).toBe('/settings/server/g1/overview');
    expect(screen.getByRole('alert')).toHaveAttribute('data-nudge');
    await userEvent.click(screen.getByRole('button', { name: 'Reset' }));
    expect(screen.queryByText(/unsaved changes/)).toBeNull();
  });

  it('is axe clean', async () => {
    seed({ perms: [MANAGE.guild] });
    stubOverview();
    const { container } = await openSettings('overview');
    await screen.findByLabelText('Server name');
    expect(await axeViolations(container)).toEqual([]);
  });
});

void bits;
