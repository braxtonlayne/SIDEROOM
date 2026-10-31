// BRG-UI-001 (wave 35, D-381): the Modules card, the conditional "Discord bridge" nav entry, the screen and the
// teardown dialog, against MSW handlers with the wire shapes of crates/sr-api/src/routes/guilds/bridge.rs
// (reconciled at the wave-35 integration: GET .../bridge/status, maps by map id, the test with no body).
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  capture,
  http,
  HttpResponse,
  MANAGE,
  openSettings,
  seed,
  server,
} from '../../__tests__/helpers';
import { useBridgeModule } from '../moduleStore';

vi.mock('../../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const bridgeModule = (over: object = {}) => ({
  id: 'bridge.enabled',
  name: 'Discord bridge',
  description: 'Mirror channels with a Discord server.',
  section: 'Integrations, more',
  phase: 35,
  enabled: false,
  default_enabled: false,
  platform_disabled: false,
  owner_only: true,
  settings: {},
  schema: { fields: [] },
  ...over,
});

/** `BridgeObject`: connected, MESSAGE_CONTENT (bit 15) granted. */
const wireBridge = (over: object = {}) => ({
  discord_guild_id: 'd1',
  discord_guild_name: 'Friends',
  bot_user_id: '1',
  bot_name: 'Relay',
  mode: 'selective',
  status: 'connected',
  intents_granted: (1 << 15) | 1,
  created_by: '1',
  created_at: 0,
  invite_url: 'https://discord.com/oauth2/authorize?client_id=1&scope=bot&permissions=536939584',
  ...over,
});

/** `MapObject`. */
const wireMap = (over: object = {}) => ({
  id: 'm1',
  channel_id: 'c1',
  discord_channel_id: 'dc1',
  discord_channel_name: 'general-chat',
  direction: 'both',
  has_webhook: true,
  queued: 0,
  dead: 0,
  created_at: 0,
  ...over,
});

/** `BridgeStatusObject`; `bridge: null` is a server whose owner has not connected a bot. */
const status = (over: { bridge?: object | null; maps?: object[] } = {}) => ({
  platform_on: true,
  module_on: true,
  connected: over.bridge !== null,
  ...(over.bridge === null ? {} : { bridge: wireBridge(over.bridge ?? {}) }),
  maps: over.maps ?? [wireMap()],
  queued: 0,
  dead_letters: 0,
  leftovers: [],
  is_owner: true,
});

const STATUS = '*/api/v1/guilds/g1/bridge/status';

const FEATURES = '*/api/v1/guilds/g1/sr/features';
const OWNER = { ownerId: '1' };

beforeEach(() => useBridgeModule.getState().reset());

describe('the Discord bridge card under Modules', () => {
  it('is off by default, in the "Integrations, more" group, with the privacy notice before the switch', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    server.use(http.get(FEATURES, () => HttpResponse.json([bridgeModule()])));
    await openSettings('features');
    const card = await screen.findByTestId('bridge-card');
    expect(screen.getByRole('heading', { name: 'Integrations, more' })).toBeInTheDocument();
    const sw = within(card).getByRole('switch', { name: 'Enable Discord bridge' });
    expect(sw).not.toBeChecked();
    const notice = within(card).getByText(/mirrors the channels you map to a Discord server/);
    expect(notice).toBeInTheDocument();
    expect(
      notice.compareDocumentPosition(sw) & Node.DOCUMENT_POSITION_FOLLOWING,
      'the notice precedes the switch',
    ).toBeTruthy();
    expect(within(card).getByText(/There is no per-member opt-out/)).toBeInTheDocument();
  });

  it('is greyed for a manager who is not the owner, with the reason', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(http.get(FEATURES, () => HttpResponse.json([bridgeModule()])));
    await openSettings('features');
    const card = await screen.findByTestId('bridge-card');
    expect(within(card).getByRole('switch', { name: 'Enable Discord bridge' })).toBeDisabled();
    expect(within(card).getByTestId('bridge-owner-only')).toHaveTextContent(
      'Only the server owner can switch this on.',
    );
  });

  it('lets the owner switch it on, and the nav entry appears', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    const cap = capture();
    server.use(
      http.get(FEATURES, () => HttpResponse.json([bridgeModule()])),
      http.patch(`${FEATURES}/bridge.enabled`, async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(bridgeModule({ enabled: true }));
      }),
    );
    await openSettings('features');
    await screen.findByTestId('bridge-card');
    expect(screen.queryByRole('button', { name: 'Discord bridge' })).toBeNull();
    await userEvent.click(screen.getByRole('switch', { name: 'Enable Discord bridge' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ enabled: true });
    expect(await screen.findByRole('button', { name: 'Discord bridge' })).toBeInTheDocument();
  });

  it('shows the 403 of a non-owner write as an error and leaves the module off', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    server.use(
      http.get(FEATURES, () => HttpResponse.json([bridgeModule()])),
      http.patch(`${FEATURES}/bridge.enabled`, () =>
        HttpResponse.json({ code: 50013, message: 'Missing Permissions' }, { status: 403 }),
      ),
    );
    await openSettings('features');
    await userEvent.click(await screen.findByRole('switch', { name: 'Enable Discord bridge' }));
    expect(await screen.findByText('You do not have permission to do that.')).toBeInTheDocument();
    expect(screen.getByRole('switch', { name: 'Enable Discord bridge' })).not.toBeChecked();
    expect(screen.queryByRole('button', { name: 'Discord bridge' })).toBeNull();
  });

  it('says the platform switched it off and keeps the switch locked', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    server.use(
      http.get(FEATURES, () => HttpResponse.json([bridgeModule({ platform_disabled: true })])),
    );
    await openSettings('features');
    const card = await screen.findByTestId('bridge-card');
    expect(within(card).getByText(/switched this feature off for every server/)).toBeVisible();
    expect(within(card).getByRole('switch', { name: 'Enable Discord bridge' })).toBeDisabled();
  });
});

describe('the "Discord bridge" nav entry', () => {
  it('is absent while the module is off (owner and manager)', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    server.use(http.get(FEATURES, () => HttpResponse.json([bridgeModule()])));
    await openSettings('overview');
    await screen.findByRole('button', { name: 'Features' });
    await waitFor(() => expect(useBridgeModule.getState().byGuild['g1']).toBe(false));
    expect(screen.queryByRole('button', { name: 'Discord bridge' })).toBeNull();
  });

  it('is present for a manager once the module is on', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(http.get(FEATURES, () => HttpResponse.json([bridgeModule({ enabled: true })])));
    await openSettings('overview');
    expect(await screen.findByRole('button', { name: 'Discord bridge' })).toBeInTheDocument();
  });

  it('is absent for a member without Manage Server even when the module is on', async () => {
    seed({ perms: [MANAGE.channels] });
    server.use(http.get(FEATURES, () => HttpResponse.json([bridgeModule({ enabled: true })])));
    await openSettings('channels');
    await screen.findByRole('button', { name: 'Channels' });
    expect(screen.queryByRole('button', { name: 'Discord bridge' })).toBeNull();
  });

  it('is absent when the operator switched the module off platform-wide', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    server.use(
      http.get(FEATURES, () =>
        HttpResponse.json([bridgeModule({ enabled: true, platform_disabled: true })]),
      ),
    );
    await openSettings('overview');
    await screen.findByRole('button', { name: 'Features' });
    await waitFor(() => expect(useBridgeModule.getState().byGuild['g1']).toBe(false));
    expect(screen.queryByRole('button', { name: 'Discord bridge' })).toBeNull();
  });
});

describe('the Discord bridge screen', () => {
  const on = () => http.get(FEATURES, () => HttpResponse.json([bridgeModule({ enabled: true })]));

  it('lets the owner paste the token once; it is sent, cleared and never shown again', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    const cap = capture();
    let stored = false;
    server.use(
      on(),
      http.get(STATUS, () =>
        HttpResponse.json(stored ? status({ maps: [] }) : status({ bridge: null, maps: [] })),
      ),
      http.get('*/api/v1/guilds/g1/bridge/discord-channels', () =>
        HttpResponse.json([{ id: 'dc1', name: 'general-chat' }]),
      ),
      http.put('*/api/v1/guilds/g1/bridge/token', async ({ request }) => {
        await cap.record(request);
        stored = true;
        return HttpResponse.json(wireBridge());
      }),
    );
    await openSettings('bridge');
    expect(await screen.findByText(/Every channel you map is mirrored/)).toBeInTheDocument();
    const field = await screen.findByLabelText('Bot token');
    expect(field).toHaveAttribute('type', 'password');
    await userEvent.type(field, 'FAKE.TOKEN.VALUE');
    await userEvent.click(screen.getByRole('button', { name: 'Connect the bot' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ token: 'FAKE.TOKEN.VALUE' });
    expect(await screen.findByText('Connected to the Discord server Friends.')).toBeVisible();
    expect(screen.queryByLabelText('Bot token')).toBeNull();
    expect(document.body.textContent).not.toContain('FAKE.TOKEN.VALUE');
    expect(
      screen.getByRole('link', { name: 'Invite the bot to your Discord server' }),
    ).toHaveAttribute('href', expect.stringContaining('discord.com/oauth2/authorize'));
  });

  it('does not offer a manager the token form or the switch-off', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      on(),
      http.get(STATUS, () => HttpResponse.json(status({ bridge: null, maps: [] }))),
    );
    await openSettings('bridge');
    expect(await screen.findByText('Only the server owner can paste the bot token.')).toBeVisible();
    expect(screen.queryByLabelText('Bot token')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Switch off the Discord bridge' })).toBeNull();
    expect(screen.getByText('Only the server owner can switch the bridge off.')).toBeVisible();
  });

  it('warns when the Message Content intent is off', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    server.use(
      on(),
      http.get(STATUS, () => HttpResponse.json(status({ bridge: { intents_granted: 1 } }))),
      http.get('*/api/v1/guilds/g1/bridge/discord-channels', () => HttpResponse.json([])),
    );
    await openSettings('bridge');
    expect(await screen.findByText('The Message Content intent is off')).toBeVisible();
  });

  it('maps a channel, tests it and removes it (a manager can)', async () => {
    seed({ perms: [MANAGE.guild] });
    const add = capture();
    const test = capture();
    const del = capture();
    let maps = status().maps;
    server.use(
      on(),
      http.get(STATUS, () => HttpResponse.json(status({ maps }))),
      http.get('*/api/v1/guilds/g1/bridge/discord-channels', () =>
        HttpResponse.json([
          { id: 'dc1', name: 'general-chat' },
          { id: 'dc2', name: 'off-topic' },
        ]),
      ),
      http.post('*/api/v1/guilds/g1/bridge/maps', async ({ request }) => {
        await add.record(request);
        const m = wireMap({
          id: 'm2',
          channel_id: 'c2',
          discord_channel_id: 'dc2',
          discord_channel_name: 'off-topic',
          direction: 'to_discord',
        });
        maps = [...maps, m];
        return HttpResponse.json(m, { status: 201 });
      }),
      http.post('*/api/v1/guilds/g1/bridge/maps/m1/test', async ({ request }) => {
        await test.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
      http.delete('*/api/v1/guilds/g1/bridge/maps/m1', async ({ request }) => {
        await del.record(request);
        maps = maps.filter((m) => (m as { id: string }).id !== 'm1');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('bridge');
    const table = await screen.findByRole('table', { name: 'Mirrored channels' });
    const row = within(table).getByRole('row', { name: /general/ });
    expect(within(row).getByText('#general')).toBeInTheDocument();
    expect(within(row).getByText('#general-chat')).toBeInTheDocument();
    expect(within(row).getByText('Both ways')).toBeInTheDocument();

    // Only unmapped channels can be picked here; the category and mapped channels are not offered.
    const ours = screen.getByLabelText('Channel here', { selector: 'select' });
    expect(within(ours).queryByRole('option', { name: '#general' })).toBeNull();
    expect(within(ours).queryByRole('option', { name: '#Text' })).toBeNull();
    await userEvent.selectOptions(ours, 'c2');
    await userEvent.selectOptions(
      screen.getByLabelText('Channel on Discord', { selector: 'select' }),
      'dc2',
    );
    await userEvent.selectOptions(
      screen.getByLabelText('Direction', { selector: 'select' }),
      'to_discord',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Mirror this channel' }));
    await waitFor(() => expect(add.calls).toHaveLength(1));
    expect(add.calls[0]!.body).toEqual({
      channel_id: 'c2',
      discord_channel_id: 'dc2',
      direction: 'to_discord',
    });
    expect(await within(table).findByText('#off-topic')).toBeInTheDocument();
    expect(within(table).getByText('From here to Discord')).toBeInTheDocument();

    await userEvent.click(
      screen.getByRole('button', { name: 'Post a hello in #general on Discord' }),
    );
    await waitFor(() => expect(test.calls).toHaveLength(1));
    // The test route takes no body: the map id in the path says which channel.
    expect(test.calls[0]!.body).toBeUndefined();
    expect(await screen.findByText('Posted a hello for #general on Discord.')).toBeVisible();

    await userEvent.click(screen.getByRole('button', { name: 'Stop mirroring #general' }));
    await waitFor(() => expect(del.calls).toHaveLength(1));
    await waitFor(() => expect(within(table).queryByText('#general-chat')).toBeNull());
  });

  it('shows the refusal of an encrypted channel', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    server.use(
      on(),
      http.get(STATUS, () => HttpResponse.json(status({ maps: [] }))),
      http.get('*/api/v1/guilds/g1/bridge/discord-channels', () =>
        HttpResponse.json([{ id: 'dc1', name: 'general-chat' }]),
      ),
      http.post('*/api/v1/guilds/g1/bridge/maps', () =>
        HttpResponse.json(
          { code: 50035, message: 'Encrypted channels cannot be mirrored.' },
          { status: 400 },
        ),
      ),
    );
    await openSettings('bridge');
    expect(
      await screen.findByText(/Encrypted channels cannot be mirrored: the server/),
    ).toBeVisible();
    await userEvent.selectOptions(
      await screen.findByLabelText('Channel here', { selector: 'select' }),
      'c1',
    );
    await userEvent.selectOptions(
      screen.getByLabelText('Channel on Discord', { selector: 'select' }),
      'dc1',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Mirror this channel' }));
    expect(await screen.findByText(/Encrypted channels cannot be mirrored\./)).toBeVisible();
  });
});

describe('switching the bridge off', () => {
  it('names what is removed and what stays, runs the teardown, then the entry is gone', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    const cap = capture();
    let enabled = true;
    server.use(
      http.get(FEATURES, () => HttpResponse.json([bridgeModule({ enabled })])),
      http.get(STATUS, () => HttpResponse.json(status())),
      http.get('*/api/v1/guilds/g1/bridge/discord-channels', () => HttpResponse.json([])),
      http.patch(`${FEATURES}/bridge.enabled`, async ({ request }) => {
        await cap.record(request);
        enabled = false;
        return HttpResponse.json({
          ...bridgeModule({ enabled: false }),
          teardown: { maps_removed: 1, failed_webhooks: ['wh-998'] },
        });
      }),
    );
    await openSettings('bridge');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Switch off the Discord bridge' }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Switch off the Discord bridge?' });
    expect(
      within(dialog).getByText(/webhooks the bridge created on Discord are deleted/),
    ).toBeVisible();
    expect(within(dialog).getByText(/Messages already mirrored stay where they are/)).toBeVisible();
    expect(within(dialog).getByText(/you remove it by hand under/)).toBeVisible();
    expect(cap.calls).toHaveLength(0);

    await userEvent.click(within(dialog).getByRole('button', { name: 'Switch off and remove' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ enabled: false });
    const done = await screen.findByRole('dialog', { name: 'The Discord bridge is off' });
    expect(within(done).getByText('1 channel mapping removed.')).toBeVisible();
    expect(within(done).getByText('wh-998')).toBeVisible();
    expect(within(done).getByText(/Remove them by hand under/)).toBeVisible();
    // The entry stays until the owner has read the result.
    expect(screen.getByRole('button', { name: 'Discord bridge' })).toBeInTheDocument();

    await userEvent.click(within(done).getByRole('button', { name: 'Done' }));
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Discord bridge' })).toBeNull(),
    );
  });

  it('says nothing is left on Discord when every webhook went', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    server.use(
      http.get(FEATURES, () => HttpResponse.json([bridgeModule({ enabled: true })])),
      http.patch(`${FEATURES}/bridge.enabled`, () =>
        HttpResponse.json({
          ...bridgeModule({ enabled: false }),
          teardown: { maps_removed: 0, failed_webhooks: [] },
        }),
      ),
    );
    await openSettings('features');
    const card = await screen.findByTestId('bridge-card');
    await userEvent.click(within(card).getByRole('switch', { name: 'Enable Discord bridge' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Switch off and remove' }));
    expect(await screen.findByText('Every webhook was deleted on Discord.')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Done' }));
    await waitFor(() =>
      expect(
        within(screen.getByTestId('bridge-card')).getByRole('switch', {
          name: 'Enable Discord bridge',
        }),
      ).not.toBeChecked(),
    );
  });

  it('keeps the module on when the owner cancels', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    const cap = capture();
    server.use(
      http.get(FEATURES, () => HttpResponse.json([bridgeModule({ enabled: true })])),
      http.patch(`${FEATURES}/bridge.enabled`, async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(bridgeModule());
      }),
    );
    await openSettings('features');
    const card = await screen.findByTestId('bridge-card');
    await userEvent.click(within(card).getByRole('switch', { name: 'Enable Discord bridge' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Keep it on' }));
    expect(cap.calls).toHaveLength(0);
    expect(within(card).getByRole('switch', { name: 'Enable Discord bridge' })).toBeChecked();
  });
});
