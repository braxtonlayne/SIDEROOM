import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { loadPermissions } from '@sideroom/wasm';
import { afterEach, beforeEach } from 'vitest';
import { resetChannelStatus, useChannelStatus } from '../../../voice/channelStatus';
import { setPermEngine } from '../../../lounge/permEngine';
import {
  bits,
  capture,
  http,
  HttpResponse,
  MANAGE,
  openSettings,
  restRole,
  restRoles,
  seed,
  server,
} from './helpers';

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

describe('Server traits (J-07, LNG-SSET-007)', () => {
  it('edits up to five traits and saves them through the profile endpoint', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    const main = capture();
    server.use(
      http.get('*/api/v1/guilds/g1', () =>
        HttpResponse.json({
          ...guildObj,
          sr_profile: { traits: [{ emoji_name: '🎮', label: 'Gamers' }] },
        }),
      ),
      http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community)),
      http.patch('*/api/v1/guilds/g1', async ({ request }) => {
        await main.record(request);
        return HttpResponse.json(guildObj);
      }),
      http.patch('*/api/v1/guilds/g1/sr/profile', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json({
          ...guildObj,
          sr_profile: { traits: [{ emoji_name: '🎮', label: 'Gamers' }, { label: 'Cozy' }] },
        });
      }),
    );
    await openSettings('overview');
    // The saved trait is in its slot and the "not backed" note for traits is gone.
    expect(await screen.findByLabelText('Label of trait 1')).toHaveValue('Gamers');
    expect(screen.queryByText(/Server traits and the server widget/)).toBeNull();

    // Fill the slots; the sixth cannot be added.
    for (let n = 2; n <= 5; n++) {
      await userEvent.click(screen.getByRole('button', { name: 'Add a trait' }));
      await userEvent.type(screen.getByLabelText(`Label of trait ${n}`), `T${n}`);
    }
    expect(screen.getByRole('button', { name: 'Add a trait' })).toBeDisabled();
    for (let n = 5; n >= 3; n--)
      await userEvent.click(screen.getByRole('button', { name: `Remove trait ${n}` }));
    await userEvent.clear(screen.getByLabelText('Label of trait 2'));
    await userEvent.type(screen.getByLabelText('Label of trait 2'), '  Cozy ');

    // Mentions and markdown are refused before anything is sent.
    await userEvent.type(screen.getByLabelText('Label of trait 1'), '**');
    expect(await screen.findByText('A label cannot contain mentions or markdown.')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(await screen.findByText(/Every trait needs a label/)).toBeVisible();
    expect(cap.calls).toHaveLength(0);
    await userEvent.type(screen.getByLabelText('Label of trait 1'), '{Backspace}{Backspace}');

    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({
      traits: [{ emoji_name: '🎮', label: 'Gamers' }, { label: 'Cozy' }],
    });
    // Only the traits changed, so the main guild endpoint was not called.
    expect(main.calls).toHaveLength(0);
  }, 30000);
});

describe('Role colour gradients (J-08, LNG-SSET-032)', () => {
  const rolesHandler = (over: Record<string, object> = {}) =>
    server.use(
      http.get('*/api/v1/guilds/g1/roles', () =>
        HttpResponse.json(restRoles().map((r) => (over[r.id] ? { ...r, ...over[r.id] } : r))),
      ),
    );

  it('switches a role to a two-colour gradient and sends the colors object', async () => {
    seed({ perms: [MANAGE.roles] });
    rolesHandler({ low: { color: 0xff0000 } });
    const cap = capture();
    server.use(
      http.patch('*/api/v1/guilds/g1/roles/low', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(restRole('low', 'Member', 1));
      }),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Member' }));
    expect(await screen.findByLabelText('Solid')).toBeChecked();
    await userEvent.click(screen.getByLabelText('Gradient'));
    expect(await screen.findByLabelText('Second role color')).toBeInTheDocument();
    const preview = screen.getByTestId('role-gradient-preview');
    expect(preview).toHaveAttribute('data-gradient');
    expect(preview.style.backgroundImage).toContain('linear-gradient');
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    // The second stop starts as the first, so it is a real two-stop colors object.
    expect(cap.calls[0]!.body).toEqual({
      colors: { primary_color: 0xff0000, secondary_color: 0xff0000, tertiary_color: null },
    });
  });

  it('the holographic preset is Discord’s fixed triple, and Solid clears the gradient', async () => {
    seed({ perms: [MANAGE.roles] });
    rolesHandler({
      low: {
        color: 11127295,
        colors: { primary_color: 11127295, secondary_color: 16759788, tertiary_color: 16761760 },
      },
    });
    const cap = capture();
    server.use(
      http.patch('*/api/v1/guilds/g1/roles/low', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(restRole('low', 'Member', 1));
      }),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Member' }));
    expect(await screen.findByLabelText('Holographic')).toBeChecked();
    // No pickers while holographic: the three stops are fixed.
    expect(screen.queryByLabelText('Second role color')).toBeNull();
    await userEvent.click(screen.getByLabelText('Solid'));
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({
      colors: { primary_color: 11127295, secondary_color: null, tertiary_color: null },
    });
  });

  it('choosing Holographic sends Discord’s fixed triple', async () => {
    seed({ perms: [MANAGE.roles] });
    rolesHandler();
    const cap = capture();
    server.use(
      http.patch('*/api/v1/guilds/g1/roles/mod', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(restRole('mod', 'Mod', 5));
      }),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Mod' }));
    await userEvent.click(await screen.findByLabelText('Holographic'));
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({
      colors: { primary_color: 11127295, secondary_color: 16759788, tertiary_color: 16761760 },
    });
  });
});

const wasmPath = resolve(process.cwd(), '../../packages/wasm/dist/permissions/permissions_bg.wasm');
const built = existsSync(wasmPath);

describe.skipIf(!built)('View Server As Role dialog (J-09, LNG-SSET-036)', () => {
  // The fixture ids are words ('g1', 'admin'); the engine wants snowflakes, so number them as they appear.
  beforeEach(async () => {
    const api = await loadPermissions(readFileSync(wasmPath));
    const ids = new Map<string, string>();
    const n = (id: string) => {
      if (!ids.has(id)) ids.set(id, String(ids.size + 1));
      return ids.get(id)!;
    };
    setPermEngine({
      compute: (i) =>
        api.compute({
          ...i,
          guild_id: n(i.guild_id),
          member_id: n(i.member_id),
          member_role_ids: i.member_role_ids.map(n),
          overwrites: i.overwrites.map((o) => ({ ...o, id: n(o.id) })),
        }),
    });
  });
  afterEach(() => setPermEngine(null));

  const ch = (id: string, name: string, type: number, extra: object = {}) => ({
    id,
    type,
    guild_id: 'g1',
    name,
    topic: null,
    position: 0,
    parent_id: null,
    nsfw: false,
    rate_limit_per_user: 0,
    permission_overwrites: [],
    ...extra,
  });

  it('a deny overwrite hides a channel and ADMINISTRATOR sees all', async () => {
    seed({ perms: [MANAGE.roles] });
    server.use(
      http.get('*/api/v1/guilds/g1/roles', () => HttpResponse.json(restRoles())),
      http.get('*/api/v1/guilds/g1/channels', () =>
        HttpResponse.json([
          ch('c1', 'general', 0, { position: 0 }),
          ch('c2', 'staff-only', 0, {
            position: 1,
            permission_overwrites: [{ id: 'g1', type: 0, allow: '0', deny: bits(10) }],
          }),
        ]),
      ),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'View Server As Role' }));
    const dialog = await screen.findByRole('dialog', { name: 'View Server As Role' });
    const list = await within(dialog).findByRole('list', { name: 'Visible channels' });
    expect(within(list).getByText('general')).toBeInTheDocument();
    expect(within(list).queryByText('staff-only')).toBeNull();
    expect(within(dialog).getByText('1 of 2 channels visible.')).toBeVisible();

    await userEvent.click(within(dialog).getByRole('checkbox', { name: 'Admin' }));
    expect(await within(dialog).findByText('2 of 2 channels visible.')).toBeVisible();
    expect(within(dialog).getByText('staff-only')).toBeInTheDocument();
    expect(within(dialog).getByText(/has Administrator/)).toBeVisible();
  });
});

describe('Voice status in the channel editor (J-09, LNG-CHN-010)', () => {
  afterEach(() => resetChannelStatus());

  it('sets the status over PUT /channels/{id}/voice-status and clears it with null', async () => {
    seed({ perms: [MANAGE.channels] });
    const put = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () =>
        HttpResponse.json([
          {
            id: 'v1',
            type: 2,
            guild_id: 'g1',
            name: 'Lounge',
            topic: null,
            position: 0,
            parent_id: null,
            nsfw: false,
            rate_limit_per_user: 0,
            permission_overwrites: [],
          },
          {
            id: 'c1',
            type: 0,
            guild_id: 'g1',
            name: 'general',
            topic: null,
            position: 1,
            parent_id: null,
            nsfw: false,
            rate_limit_per_user: 0,
            permission_overwrites: [],
          },
        ]),
      ),
      http.put('*/api/v1/channels/v1/voice-status', async ({ request }) => {
        await put.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('channels');
    await userEvent.click(await screen.findByRole('button', { name: /^([^ ]+ )?Lounge$/ }));
    await userEvent.click(await screen.findByRole('tab', { name: 'Voice' }));
    await userEvent.type(screen.getByLabelText('Channel status'), 'Movie night');
    await userEvent.click(screen.getByRole('button', { name: 'Set status' }));
    await waitFor(() => expect(put.calls).toHaveLength(1));
    expect(put.calls[0]!.body).toEqual({ status: 'Movie night' });
    expect(await screen.findByText('Status set.')).toBeVisible();

    // The gateway event arrives; the field follows and Clear sends null.
    useChannelStatus.setState({ byChannel: { v1: 'Movie night' } });
    await userEvent.click(await screen.findByRole('button', { name: 'Clear status' }));
    await waitFor(() => expect(put.calls).toHaveLength(2));
    expect(put.calls[1]!.body).toEqual({ status: null });
    expect(await screen.findByText('Status cleared.')).toBeVisible();

    // A text channel has no Voice tab.
    await userEvent.click(screen.getByRole('button', { name: '# general' }));
    await waitFor(() => expect(screen.queryByRole('tab', { name: 'Voice' })).toBeNull());
  }, 30000);
});
