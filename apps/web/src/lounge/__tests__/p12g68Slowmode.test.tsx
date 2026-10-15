// Slowmode presets applied on demand (FTR-SLOW-001, G-74): the channel menu entry and its dialog.
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../router';
import { resetStore, useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { useLoungeUi } from '../store';
import { useNotifSettings } from '../shell/notifSettings';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const EVERYONE = String((1 << 10) | (1 << 11) | 1);

function seed(ownerId: string) {
  resetStore();
  localStorage.clear();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useNotifSettings.setState({ byGuild: {} });
  useStore.setState({
    currentUserId: '1',
    users: { '1': { id: '1', username: 'me' }, '2': { id: '2', username: 'bob' } },
    guilds: { g1: { id: 'g1', name: 'Alpha Guild', owner_id: ownerId } },
    guildOrder: ['g1'],
    channels: {
      cat: { id: 'cat', type: 4, guild_id: 'g1', name: 'Text Channels', position: 0 },
      c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', parent_id: 'cat', position: 0 },
      c2: { id: 'c2', type: 0, guild_id: 'g1', name: 'random', parent_id: 'cat', position: 1 },
    },
    channelIdsByGuild: { g1: ['cat', 'c1', 'c2'] },
    roles: {
      g1: {
        id: 'g1',
        guild_id: 'g1',
        name: '@everyone',
        color: 0,
        position: 0,
        permissions: EVERYONE,
      },
    },
    roleIdsByGuild: { g1: ['g1'] },
    memberIdsByGuild: { g1: ['1', '2'] },
    members: {
      'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
      'g1:2': { guild_id: 'g1', user_id: '2', roles: [] },
    },
    connection: 'connected',
  });
}

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

const presets = (enabled = true) =>
  http.get('*/api/v1/guilds/g1/sr/slowmode/presets', () =>
    HttpResponse.json({
      enabled,
      presets: enabled
        ? [
            { id: 'light', name: 'Light', seconds: 5 },
            { id: 'strict', name: 'Strict', seconds: 120 },
          ]
        : [],
    }),
  );

async function openMenuOnRandom() {
  const sidebar = await screen.findByTestId('channel-sidebar');
  await userEvent.pointer({
    keys: '[MouseRight]',
    target: within(sidebar).getByRole('link', { name: 'random' }),
  });
}

describe('Apply Slowmode Preset (G-74)', () => {
  it('applies a preset for a while from the channel menu and reports what changed', async () => {
    seed('1');
    const bodies: unknown[] = [];
    server.use(
      presets(),
      http.post('*/api/v1/guilds/g1/sr/slowmode/apply', async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json({
          applied: [
            { channel_id: 'c2', before: 0, seconds: 120, revert_at: Date.UTC(2030, 0, 1, 12, 0) },
          ],
        });
      }),
    );
    at('/channels/g1/c1');
    await openMenuOnRandom();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Apply Slowmode Preset' }));
    const dialog = await screen.findByRole('dialog', { name: 'Slowmode preset for #random' });
    expect(await within(dialog).findByRole('combobox', { name: 'Preset' })).toBeInTheDocument();
    expect(await axeViolations(dialog)).toEqual([]);

    await userEvent.click(within(dialog).getByRole('combobox', { name: 'Preset' }));
    await userEvent.click(await screen.findByRole('option', { name: 'Strict (120 seconds)' }));
    await userEvent.click(within(dialog).getByRole('combobox', { name: 'How long' }));
    await userEvent.click(await screen.findByRole('option', { name: 'For 1 hour' }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Apply preset' }));
    await waitFor(() =>
      expect(bodies).toEqual([{ preset_id: 'strict', channel_ids: ['c2'], minutes: 60 }]),
    );
    expect(
      await within(dialog).findByText(/#random now has slowmode 120 seconds/),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(/goes back to off at/)).toBeInTheDocument();
  });

  it('leaves out minutes when the preset stays until changed', async () => {
    seed('1');
    const bodies: unknown[] = [];
    server.use(
      presets(),
      http.post('*/api/v1/guilds/g1/sr/slowmode/apply', async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json({
          applied: [{ channel_id: 'c2', before: 0, seconds: 5, revert_at: null }],
        });
      }),
    );
    at('/channels/g1/c1');
    await openMenuOnRandom();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Apply Slowmode Preset' }));
    const dialog = await screen.findByRole('dialog', { name: 'Slowmode preset for #random' });
    await userEvent.click(await within(dialog).findByRole('button', { name: 'Apply preset' }));
    await waitFor(() => expect(bodies).toEqual([{ preset_id: 'light', channel_ids: ['c2'] }]));
    expect(
      await within(dialog).findByText('#random now has slowmode 5 seconds.'),
    ).toBeInTheDocument();
  });

  it('says so when the module is off, and offers nothing to members', async () => {
    seed('1');
    server.use(presets(false));
    at('/channels/g1/c1');
    await openMenuOnRandom();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Apply Slowmode Preset' }));
    const dialog = await screen.findByRole('dialog', { name: 'Slowmode preset for #random' });
    expect(await within(dialog).findByText(/not turned on for this server/)).toBeInTheDocument();
    expect(within(dialog).queryByRole('button', { name: 'Apply preset' })).toBeNull();
  });

  it('has no menu entry for someone without Manage Channels', async () => {
    seed('2');
    at('/channels/g1/c1');
    await openMenuOnRandom();
    await screen.findByRole('menuitem', { name: 'Mark As Read' });
    expect(screen.queryByRole('menuitem', { name: 'Apply Slowmode Preset' })).toBeNull();
  });
});

describe('Slowmode schedules module card (G-74)', () => {
  it('offers the preset form for any channels once the module is on', async () => {
    seed('1');
    const bodies: unknown[] = [];
    server.use(
      presets(),
      http.get('*/api/v1/guilds/g1/sr/features', () =>
        HttpResponse.json([
          {
            id: 'lounge.slowmode_plus',
            name: 'Slowmode schedules',
            description: 'Raise slowmode on a schedule.',
            section: 'Moderation',
            phase: 11,
            enabled: true,
            default_enabled: false,
            platform_disabled: false,
            settings: {},
            schema: { fields: [] },
          },
        ]),
      ),
      http.post('*/api/v1/guilds/g1/sr/slowmode/apply', async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json({
          applied: [
            { channel_id: 'c1', before: 5, seconds: 5, revert_at: null },
            { channel_id: 'c2', before: 0, seconds: 5, revert_at: null },
          ],
        });
      }),
    );
    const { FeaturesSection } = await import('../../settings/server/sections/Features');
    render(<FeaturesSection guildId="g1" viewer={{} as never} />);
    const panel = await screen.findByRole('region', { name: 'Apply a preset now' });
    const apply = await within(panel).findByRole('button', { name: 'Apply preset' });
    expect(apply).toBeDisabled();
    await userEvent.click(within(panel).getByRole('checkbox', { name: '#general' }));
    await userEvent.click(within(panel).getByRole('checkbox', { name: '#random' }));
    await userEvent.click(apply);
    await waitFor(() =>
      expect(bodies).toEqual([{ preset_id: 'light', channel_ids: ['c1', 'c2'] }]),
    );
    expect(
      await within(panel).findByText('#general now has slowmode 5 seconds.'),
    ).toBeInTheDocument();
  });
});
