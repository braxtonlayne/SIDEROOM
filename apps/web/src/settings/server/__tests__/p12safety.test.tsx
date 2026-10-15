import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useStore } from '../../../store';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const guildObj = (over: object = {}) => ({
  id: 'g1',
  name: 'Book Club',
  icon: null,
  description: null,
  owner_id: '9',
  system_channel_id: 'c1',
  verification_level: 0,
  default_message_notifications: 0,
  explicit_content_filter: 0,
  mfa_level: 0,
  system_channel_flags: 0,
  widget_enabled: false,
  widget_channel_id: null,
  preferred_locale: 'en-US',
  features: [],
  ...over,
});

const community = (over: object = {}) => ({
  guild_id: 'g1',
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
  dm_spam_filter: false,
  explicit_media_filter_dms: false,
  hide_suspicious_links: false,
  warn_new_account_days: 0,
  age_restricted: false,
  ...over,
});

describe('System messages and the widget (Overview)', () => {
  it('saves the system message switches as system_channel_flags', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1', () => HttpResponse.json(guildObj({ system_channel_flags: 4 }))),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community())),
      http.get('*/api/v1/guilds/g1/widget', () =>
        HttpResponse.json({ enabled: false, channel_id: null }),
      ),
      http.patch('*/api/v1/guilds/g1', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(guildObj());
      }),
    );
    await openSettings('overview');
    const group = await screen.findByRole('group', { name: 'Send these messages' });
    const join = within(group).getByRole('checkbox', {
      name: 'A welcome message when someone joins',
    });
    const tips = within(group).getByRole('checkbox', {
      name: 'Helpful tips for setting up the server',
    });
    expect(join).toBeChecked();
    expect(tips).not.toBeChecked();
    await userEvent.click(join);
    await userEvent.click(tips);
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await screen.findByText('Server settings saved.');
    // Bit 0 (join) now suppressed; bit 2 (tips) no longer suppressed.
    expect(cap.calls[0]!.body).toMatchObject({ system_channel_flags: 1 });
  });

  it('turns the widget on and picks its channel', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1', () => HttpResponse.json(guildObj())),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community())),
      http.get('*/api/v1/guilds/g1/widget', () =>
        HttpResponse.json({ enabled: false, channel_id: null }),
      ),
      http.patch('*/api/v1/guilds/g1/widget', async ({ request }) => {
        await cap.record(request);
        const b = (await request.clone().json()) as { enabled?: boolean; channel_id?: string };
        return HttpResponse.json({ enabled: b.enabled ?? false, channel_id: b.channel_id ?? null });
      }),
    );
    await openSettings('overview');
    await userEvent.click(await screen.findByRole('checkbox', { name: 'Enable the server widget' }));
    await screen.findByText('Widget settings saved.');
    await userEvent.selectOptions(screen.getByLabelText('Invite channel'), 'c1');
    await waitFor(() => expect(cap.calls).toHaveLength(2));
    expect(cap.calls[0]!.body).toEqual({ enabled: true });
    expect(cap.calls[1]!.body).toEqual({ channel_id: 'c1' });
  });
});

describe('Protection (Moderation)', () => {
  const baseline = () =>
    server.use(
      http.get('*/api/v1/guilds/g1/automod/rules', () => HttpResponse.json([])),
    );

  it('the owner switches on the 2FA requirement for moderators', async () => {
    seed({ perms: [MANAGE.guild], ownerId: '1' });
    baseline();
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1', () => HttpResponse.json(guildObj({ owner_id: '1' }))),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community())),
      http.post('*/api/v1/guilds/g1/mfa', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json({ level: 1 });
      }),
    );
    await openSettings('moderation');
    const sw = await screen.findByRole('checkbox', { name: 'Require 2FA for moderator actions' });
    expect(sw).toBeEnabled();
    await userEvent.click(sw);
    await screen.findByText('Two-factor requirement saved.');
    expect(cap.calls[0]!.body).toEqual({ level: 1 });
    expect(sw).toBeChecked();
  });

  it('only the owner can change it', async () => {
    seed({ perms: [MANAGE.guild], ownerId: '9' });
    baseline();
    server.use(
      http.get('*/api/v1/guilds/g1', () => HttpResponse.json(guildObj({ mfa_level: 1 }))),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community())),
    );
    await openSettings('moderation');
    const sw = await screen.findByRole('checkbox', { name: 'Require 2FA for moderator actions' });
    expect(sw).toBeDisabled();
    expect(sw).toBeChecked();
    expect(screen.getByText('Only the server owner can change this.')).toBeInTheDocument();
  });

  it('saves the DM and spam protection switches and the new account warning', async () => {
    seed({ perms: [MANAGE.guild] });
    baseline();
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1', () => HttpResponse.json(guildObj())),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community())),
      http.patch('*/api/v1/guilds/g1/sr/community', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(community(await request.clone().json()));
      }),
    );
    await openSettings('moderation');
    await userEvent.click(
      await screen.findByRole('checkbox', { name: 'Filter spam in direct messages from members' }),
    );
    await screen.findByText('Protection settings saved.');
    expect(cap.calls[0]!.body).toEqual({ dm_spam_filter: true });
    const days = screen.getByLabelText('Warn about new accounts');
    await userEvent.clear(days);
    await userEvent.type(days, '400');
    expect(screen.getByText('Enter a whole number of days from 0 to 365.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    await userEvent.clear(days);
    await userEvent.type(days, '14');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(cap.calls).toHaveLength(2));
    expect(cap.calls[1]!.body).toEqual({ warn_new_account_days: 14 });
    expect(useStore.getState().currentUserId).toBe('1');
  });
});
