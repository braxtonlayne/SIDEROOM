// BRG-SYN-004 (wave 35b): the whole-server mirror screens against MSW handlers that answer the WIRE shapes of the
// real routes (crates/sr-api/src/routes/guilds/bridge_sync.rs and bridge_conflicts.rs); settings/server/bridge/api.ts
// turns them into what the screens draw (reconciled at the wave-35b integration).
import { configure, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useStore } from '../../../../store';
import {
  capture,
  http,
  HttpResponse,
  MANAGE,
  openSettings,
  restRoles,
  seed,
  server,
} from '../../__tests__/helpers';
import { buildMemberRows } from '../../../../lounge/memberGroups';
import type { Role } from '../../../../lounge/data';
import type { Preflight, WireConflict, WireMirrors, WirePlan, WireSyncStatus } from '../api';
import { useMirrorStore, useRolePrefill } from '../mirrors';
import { useBridgeModule } from '../moduleStore';

vi.mock('../../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const B = '*/api/v1/guilds/g1/bridge';
const FEATURES = '*/api/v1/guilds/g1/sr/features';
const OWNER = { ownerId: '1' };

const moduleOn = () =>
  http.get(FEATURES, () =>
    HttpResponse.json([
      {
        id: 'bridge.enabled',
        name: 'Discord bridge',
        description: 'd',
        section: 'Integrations, more',
        phase: 35,
        enabled: true,
        default_enabled: false,
        platform_disabled: false,
        owner_only: true,
        settings: {},
        schema: { fields: [] },
      },
    ]),
  );

const wireStatus = (mode: 'selective' | 'whole_guild') => ({
  platform_on: true,
  module_on: true,
  connected: true,
  bridge: {
    discord_guild_id: 'd1',
    discord_guild_name: 'Friends',
    bot_user_id: '1',
    bot_name: 'Relay',
    mode,
    status: 'connected',
    intents_granted: (1 << 15) | 1,
    created_by: '1',
    created_at: 0,
    invite_url: 'https://discord.com/oauth2/authorize?client_id=1',
  },
  maps: [],
  queued: 0,
  dead_letters: 0,
  leftovers: [],
  is_owner: true,
});

const preflight = (over: Partial<Preflight> = {}): Preflight => ({
  checks: [
    { id: 'bot_admin', ok: true, blocker: true },
    { id: 'bot_top_role', ok: true, blocker: true },
    { id: 'intents', ok: true, blocker: true },
    { id: 'caps', ok: true, blocker: true },
  ],
  ...over,
});

// Permission words: SEND_MESSAGES = 2048, BAN_MEMBERS = 4.
const item = (over: Partial<WirePlan['items'][number]>): WirePlan['items'][number] => ({
  kind: 'role',
  action: 'match',
  sr_id: null,
  discord_id: null,
  name: 'x',
  channel_type: null,
  sr_permissions: '0',
  discord_permissions: '0',
  dangerous_bits: '0',
  import: false,
  degraded: null,
  ...over,
});

const plan = (over: Partial<WirePlan> = {}): WirePlan => ({
  id: 'p1',
  status: 'ready',
  plan_hash: 'h1',
  expires_at: Date.now() + 900_000,
  seed_structure: 'sr',
  blockers: [],
  items: [
    item({
      name: 'Mods',
      sr_id: 'mods',
      discord_id: 'D1',
      sr_permissions: '2048',
      discord_permissions: '2052',
    }),
    item({ name: 'Fans', sr_id: 'fans', action: 'create_on_discord' }),
    item({
      name: 'Boosters',
      discord_id: 'D3',
      action: 'create_here',
      discord_permissions: '2052',
      dangerous_bits: '4',
      import: true,
    }),
    item({ kind: 'channel', name: 'general', sr_id: 'c1', discord_id: 'D4', channel_type: 0 }),
  ],
  not_mirrored: [
    { kind: 'channel', sr_id: 'n1', name: 'dm-ish', reason: 'End-to-end encrypted', code: 'e2ee' },
  ],
  estimated_writes: 40,
  ...over,
});

const syncStatus = (over: Partial<WireSyncStatus> = {}): WireSyncStatus => ({
  mode: 'whole_guild',
  sync_state: 'live',
  last_reconcile_at: null,
  queue_pending: 2,
  queue_running: 1,
  dead_ops: 0,
  role_maps: 5,
  channel_maps: 7,
  degraded: 0,
  frozen: 0,
  held_deletions: 1,
  intents_ok: true,
  reconcile_available_at: null,
  ...over,
});

const conflict = (over: Partial<WireConflict> = {}): WireConflict => ({
  id: 'k1',
  kind: 'permission_reverted',
  object_kind: 'role',
  sr_object_id: 'mod',
  discord_object_id: 'D1',
  object_name: 'Mod',
  field: 'permissions',
  discord_value: '4',
  sr_value: '0',
  discord_actor_name: 'Rin',
  count: 2,
  status: 'open',
  first_at: 1_700_000_000_000,
  last_at: 1_700_000_100_000,
  ...over,
});

const conflicts = (rows: WireConflict[]) => ({ conflicts: rows, open: rows.length });

beforeAll(() => configure({ asyncUtilTimeout: 5000 }));

beforeEach(() => {
  useBridgeModule.getState().reset();
  useMirrorStore.getState().reset();
  useRolePrefill.getState().set(null);
});

describe('mode card', () => {
  it('lets a manager prepare a plan but not change the mode, and says who can', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      moduleOn(),
      http.get(`${B}/status`, () => HttpResponse.json(wireStatus('selective'))),
      http.get(`${B}/discord-channels`, () => HttpResponse.json([])),
    );
    await openSettings('bridge');
    const card = await screen.findByTestId('bridge-mode-card');
    expect(within(card).getByText('Mode: Only bridge these')).toBeVisible();
    expect(within(card).getByTestId('bridge-mode-owner-only')).toHaveTextContent(
      'Only the server owner can change this.',
    );
    expect(
      within(card).getByRole('button', { name: 'Prepare the whole-server plan' }),
    ).toBeEnabled();
    expect(within(card).getByText('The bot needs Administrator on Discord')).toBeVisible();
  });

  it('whole-server mode: the way back is the owner’s, greyed for a manager', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      moduleOn(),
      http.get(`${B}/status`, () => HttpResponse.json(wireStatus('whole_guild'))),
      http.get(`${B}/sync`, () => HttpResponse.json(syncStatus())),
      http.get(`${B}/conflicts`, () => HttpResponse.json(conflicts([]))),
    );
    await openSettings('bridge');
    const card = await screen.findByTestId('bridge-mode-card');
    expect(within(card).getByText('Mode: Bridge the whole server')).toBeVisible();
    expect(
      within(card).getByRole('button', { name: 'Go back to Only bridge these' }),
    ).toBeDisabled();
    expect(
      await screen.findByText(/Everything in this server except direct messages and voice/),
    ).toBeVisible();
  });

  it('the owner goes back to selective with no plan', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    const cap = capture();
    let mode: 'selective' | 'whole_guild' = 'whole_guild';
    server.use(
      moduleOn(),
      http.get(`${B}/status`, () => HttpResponse.json(wireStatus(mode))),
      http.get(`${B}/sync`, () => HttpResponse.json(syncStatus())),
      http.get(`${B}/conflicts`, () => HttpResponse.json(conflicts([]))),
      http.get(`${B}/discord-channels`, () => HttpResponse.json([])),
      http.post(`${B}/mode`, async ({ request }) => {
        await cap.record(request);
        mode = 'selective';
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('bridge');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Go back to Only bridge these' }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Go back' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ mode: 'selective' });
    expect(await screen.findByText('Mode: Only bridge these')).toBeVisible();
  });
});

describe('preflight and plan', () => {
  const setup = (opts: {
    pre?: Preflight;
    plans?: WirePlan[];
    mode?: (b: unknown) => Response;
  }) => {
    const mode = capture();
    let n = 0;
    server.use(
      moduleOn(),
      http.get(`${B}/status`, () => HttpResponse.json(wireStatus('selective'))),
      http.get(`${B}/discord-channels`, () => HttpResponse.json([])),
      http.get(`${B}/preflight`, () => HttpResponse.json(opts.pre ?? preflight())),
      http.post(`${B}/plan`, () => HttpResponse.json({ plan_id: 'p1' })),
      http.get(`${B}/plan/p1`, () => {
        const p = (opts.plans ?? [plan()])[Math.min(n, (opts.plans ?? [plan()]).length - 1)];
        n += 1;
        return HttpResponse.json(p);
      }),
      http.post(`${B}/mode`, async ({ request }) => {
        await mode.record(request);
        return opts.mode
          ? opts.mode(null)
          : (new HttpResponse(null, { status: 204 }) as unknown as Response);
      }),
    );
    return mode;
  };

  it('a failed blocker check keeps the plan from being built', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    setup({
      pre: preflight({
        checks: [
          { id: 'bot_admin', ok: false, blocker: true },
          {
            id: 'other_integration',
            ok: false,
            blocker: false,
            detail: 'Another bot edited roles.',
          },
        ],
      }),
    });
    await openSettings('bridge');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Prepare the whole-server plan' }),
    );
    const pre = await screen.findByTestId('bridge-preflight');
    expect(await within(pre).findByText('The bot is an Administrator on Discord')).toBeVisible();
    expect(within(pre).getByText('Fix this first')).toBeVisible();
    expect(within(pre).getByText('Warning')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Build the plan' })).toBeDisabled();
  });

  it('shows the plan tabs with counts, and the owner confirms with the plan hash', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    const mode = setup({});
    await openSettings('bridge');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Prepare the whole-server plan' }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Build the plan' }));
    expect(await screen.findByRole('tab', { name: 'Roles (3)' })).toBeVisible();
    expect(screen.getByRole('tab', { name: 'Channels (1)' })).toBeVisible();
    expect(screen.getByRole('tab', { name: 'Changes on Discord (1)' })).toBeVisible();
    await userEvent.click(screen.getByRole('tab', { name: 'Changes on Discord (1)' }));
    expect(screen.getByText('Role Mods on Discord loses Ban Members.')).toBeVisible();
    await userEvent.click(screen.getByRole('tab', { name: 'Imported from Discord (1)' }));
    expect(screen.getByText('Dangerous: Ban Members')).toBeVisible();
    await userEvent.click(screen.getByRole('tab', { name: 'Not mirrored (1)' }));
    expect(screen.getByText('End-to-end encrypted')).toBeVisible();

    await userEvent.click(screen.getByRole('button', { name: 'Confirm the plan' }));
    const dialog = await screen.findByRole('dialog', { name: 'Bridge the whole server?' });
    expect(within(dialog).getByText(/decided by Discord/)).toBeVisible();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm and start' }));
    await waitFor(() => expect(mode.calls).toHaveLength(1));
    expect(mode.calls[0]!.body).toEqual({ mode: 'whole_guild', plan_id: 'p1', plan_hash: 'h1' });
  });

  it('blockers disable Confirm; a manager can read the plan but not confirm', async () => {
    seed({ perms: [MANAGE.guild] });
    setup({ plans: [plan({ blockers: [{ code: 'bot_role_not_top', message: 'top role' }] })] });
    await openSettings('bridge');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Prepare the whole-server plan' }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Build the plan' }));
    expect(await screen.findByText('Fix these before confirming')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Confirm the plan' })).toBeDisabled();
    expect(screen.getByText(/Only the server owner can confirm/)).toBeVisible();
  });

  it('a stale plan (409) asks for a rebuild', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    setup({
      mode: () =>
        HttpResponse.json(
          { code: 40900, message: 'BRIDGE_PLAN_STALE' },
          { status: 409 },
        ) as unknown as Response,
    });
    await openSettings('bridge');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Prepare the whole-server plan' }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Build the plan' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Confirm the plan' }));
    await userEvent.click(
      await within(
        await screen.findByRole('dialog', { name: 'Bridge the whole server?' }),
      ).findByRole('button', { name: 'Confirm and start' }),
    );
    expect(await screen.findByText(/changed after the plan was built/)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Confirm the plan' })).toBeDisabled();
  });

  it('unlinks a matched pair with a PATCH', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    setup({});
    const patch = capture();
    server.use(
      http.patch(`${B}/plan/p1`, async ({ request }) => {
        await patch.record(request);
        return HttpResponse.json(plan());
      }),
    );
    await openSettings('bridge');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Prepare the whole-server plan' }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Build the plan' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Unlink Mods' }));
    await waitFor(() => expect(patch.calls).toHaveLength(1));
    expect(patch.calls[0]!.body).toEqual({ op: 'unlink', index: 0 });
  });
});

describe('sync status and conflict log', () => {
  const wholeHandlers = (status: WireSyncStatus, rows: WireConflict[]) =>
    server.use(
      moduleOn(),
      http.get(`${B}/status`, () => HttpResponse.json(wireStatus('whole_guild'))),
      http.get(`${B}/sync`, () => HttpResponse.json(status)),
      http.get(`${B}/conflicts`, () => HttpResponse.json(conflicts(rows))),
    );

  // The server reports neither seed progress nor the rate-limit wait yet (integration handoff): those lines stay
  // empty; the state, the bot checks, the queue and the counts come from `GET .../sync`.
  it('shows the state, the bot checks, the queue and the counts; pause posts', async () => {
    seed({ perms: [MANAGE.guild] });
    const pause = capture();
    wholeHandlers(syncStatus({ sync_state: 'seeding' }), []);
    server.use(
      http.post(`${B}/pause`, async ({ request }) => {
        await pause.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('bridge');
    const card = await screen.findByTestId('bridge-sync-status');
    expect(await within(card).findByText('Setting up')).toBeVisible();
    expect(within(card).getByText('3')).toBeVisible();
    expect(within(card).getByText('12')).toBeVisible();
    await userEvent.click(within(card).getByRole('button', { name: 'Pause sync' }));
    await waitFor(() => expect(pause.calls).toHaveLength(1));
  });

  it('a paused-for-access state explains itself and offers no reconcile', async () => {
    seed({ perms: [MANAGE.guild] });
    wholeHandlers(syncStatus({ sync_state: 'paused_discord_access' }), []);
    await openSettings('bridge');
    const card = await screen.findByTestId('bridge-sync-status');
    expect(await within(card).findByText('Paused: the bot lost access on Discord')).toBeVisible();
    expect(within(card).getByRole('button', { name: 'Reconcile now' })).toBeDisabled();
  });

  it('lists a reverted permission with who, what the bridge did and per-kind actions; dismiss posts', async () => {
    seed({ perms: [MANAGE.guild] });
    const dismiss = capture();
    wholeHandlers(syncStatus(), [
      conflict(),
      conflict({
        id: 'k2',
        kind: 'deletion_held',
        object_kind: 'channel',
        sr_object_id: 'c9',
        object_name: 'lore',
        field: 'deleted',
        discord_value: undefined,
        sr_value: { hours: 24 },
      }),
    ]);
    server.use(
      http.post(`${B}/conflicts/k1/dismiss`, async ({ request }) => {
        await dismiss.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('bridge');
    const log = await screen.findByTestId('bridge-conflicts');
    expect(await within(log).findByText('Permission change undone: Mod')).toBeVisible();
    expect(within(log).getAllByText('Discord: Ban Members').length).toBe(1);
    expect(within(log).getAllByText('By Rin').length).toBeGreaterThan(0);
    expect(
      within(log).getByText('The bridge: Put the permissions set here back on Discord.'),
    ).toBeVisible();
    expect(within(log).getByRole('button', { name: 'Apply on SIDE ROOM: Mod' })).toBeVisible();
    expect(within(log).getByRole('button', { name: 'Restore: lore' })).toBeVisible();
    expect(within(log).getByRole('button', { name: 'Delete now: lore' })).toBeVisible();
    await userEvent.click(within(log).getByRole('button', { name: 'Dismiss: Mod' }));
    await waitFor(() => expect(dismiss.calls).toHaveLength(1));
  });

  it('"Apply on SIDE ROOM" opens the role editor prefilled and saves nothing by itself', async () => {
    seed({ perms: [MANAGE.guild, MANAGE.roles], position: 20, ...OWNER });
    const patched = capture();
    wholeHandlers(syncStatus(), [conflict()]);
    server.use(
      http.get('*/api/v1/guilds/g1/roles', () => HttpResponse.json(restRoles())),
      http.patch('*/api/v1/guilds/g1/roles/mod', async ({ request }) => {
        await patched.record(request);
        return HttpResponse.json({});
      }),
    );
    await openSettings('bridge');
    await userEvent.click(await screen.findByRole('button', { name: 'Apply on SIDE ROOM: Mod' }));
    expect(await screen.findByText('Prefilled from Discord’s change')).toBeVisible();
    expect(patched.calls).toHaveLength(0);
    const ban = (await screen.findAllByRole('switch', { name: /Ban Members/ }))[0]!;
    expect(ban).toBeChecked();
  });
});

describe('badges and member grouping', () => {
  const mirrors = (over: Partial<WireMirrors> = {}): WireMirrors => ({
    mode: 'whole_guild',
    roles: [{ role_id: 'mod', discord_role_id: 'D1', discord_only_bits: [47], state: 'mapped' }],
    channels: [{ channel_id: 'c1', discord_channel_id: 'D2', state: 'mapped' }],
    ...over,
  });
  const bridgeChannel = () =>
    useStore.setState((s) => ({
      channels: {
        ...s.channels,
        c1: { ...s.channels['c1']!, sr_bridge: { direction: 'both', mode: 'whole_guild' } },
      },
    }));

  it('the role editor says mirrored, renames the SIDE ROOM-only group and lists Discord-only bits', async () => {
    seed({ perms: [MANAGE.roles], position: 20 });
    bridgeChannel();
    server.use(
      http.get('*/api/v1/guilds/g1/roles', () => HttpResponse.json(restRoles())),
      http.get(`${B}/mirrors`, () => HttpResponse.json(mirrors())),
      http.get(`${B}/members`, () => HttpResponse.json([])),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'Mod' }));
    await userEvent.click(screen.getByRole('tab', { name: 'Permissions' }));
    expect((await screen.findAllByText('Mirrored to Discord')).length).toBeGreaterThan(0);
    expect(screen.getByText('SIDE ROOM only, not sent to Discord')).toBeVisible();
    expect(screen.getByText('Set on Discord')).toBeVisible();
    expect(screen.getByText('Discord permission 47')).toBeVisible();
  });

  it('the channel permissions screen marks a member overwrite SIDE ROOM only', async () => {
    seed({ perms: [MANAGE.channels, MANAGE.roles] });
    bridgeChannel();
    useStore.setState((s) => ({
      channels: {
        ...s.channels,
        c1: {
          ...s.channels['c1']!,
          permission_overwrites: [{ id: '2', type: 1, allow: '0', deny: '2048' }],
        },
      },
    }));
    server.use(
      http.get(`${B}/mirrors`, () =>
        HttpResponse.json(
          mirrors({
            channels: [
              {
                channel_id: 'c1',
                discord_channel_id: 'D2',
                state: 'degraded',
                degraded_reason: 'Discord needs Community for announcements',
              },
            ],
          }),
        ),
      ),
      http.get(`${B}/members`, () => HttpResponse.json([])),
      http.get('*/api/v1/guilds/g1/channels', () =>
        HttpResponse.json([
          {
            id: 'c1',
            type: 0,
            guild_id: 'g1',
            name: 'general',
            position: 0,
            permission_overwrites: [{ id: '2', type: 1, allow: '0', deny: '2048' }],
          },
        ]),
      ),
    );
    await openSettings('channels');
    await userEvent.click(await screen.findByRole('button', { name: '# general' }));
    await userEvent.click(await screen.findByRole('tab', { name: 'Permissions' }));
    expect(await screen.findByTestId('channel-mirror')).toHaveTextContent('Mirrored to Discord');
    expect(screen.getByText(/Mirrored as a simpler type/)).toBeVisible();
    expect((await screen.findAllByText('SIDE ROOM only')).length).toBeGreaterThan(0);
  });
});

describe('member list grouping', () => {
  const role = (id: string, name: string, position: number, over: Partial<Role> = {}): Role => ({
    id,
    name,
    hoist: false,
    position,
    ...over,
  });
  const bm = (
    id: string,
    name: string,
    role_ids: string[],
    presence: 'online' | 'offline' | null,
  ) => ({
    id,
    display_name: name,
    avatar: null,
    role_name: null,
    role_color: null,
    bot: false,
    presence,
    role_ids,
  });

  it('puts Discord members under the hoisted SIDE ROOM roles their roles map to, coloured like them', () => {
    const roles = [
      role('g', '@everyone', 0),
      role('mods', 'Mods', 5, { hoist: true, color: '#ff0000' }),
    ];
    const rows = buildMemberRows(
      [],
      roles,
      {},
      {
        bridged: [
          bm('d1', 'Rin', ['mods'], 'online'),
          bm('d2', 'Sol', [], null),
          bm('d3', 'Off', ['mods'], 'offline'),
        ],
      },
    );
    const shape = rows.map((r) =>
      r.kind === 'header'
        ? `# ${r.label} ${r.count}`
        : r.kind === 'bridged'
          ? `${r.member.display_name}:${r.color}`
          : 'native',
    );
    expect(shape).toEqual([
      '# Mods 1',
      'Rin:#ff0000',
      '# Online 1',
      'Sol:null',
      '# Offline 1',
      'Off:#ff0000',
    ]);
  });

  it('adds nothing when there are no Discord members', () => {
    expect(buildMemberRows([], [role('g', '@everyone', 0)], {})).toEqual([]);
  });
});

describe('teardown options (whole server)', () => {
  it('leave is the default; the options send what the owner ticked', async () => {
    seed({ perms: [MANAGE.guild], ...OWNER });
    const off = capture();
    const teardown = capture();
    server.use(
      moduleOn(),
      http.get(`${B}/status`, () => HttpResponse.json(wireStatus('whole_guild'))),
      http.get(`${B}/sync`, () => HttpResponse.json(syncStatus())),
      http.get(`${B}/conflicts`, () => HttpResponse.json(conflicts([]))),
      http.get(`${B}/teardown-preview`, () =>
        HttpResponse.json({
          whole_guild: true,
          created_roles: 2,
          created_categories: 1,
          created_channels: 3,
          created_threads: 0,
          created_with_discord_messages: 1,
          restorable_roles: 4,
          restorable_channels: 2,
        }),
      ),
      http.post(`${B}/teardown`, async ({ request }) => {
        await teardown.record(request);
        return HttpResponse.json({
          maps_removed: 6,
          webhooks_deleted: 3,
          roles_removed: 2,
          channels_removed: 3,
          roles_restored: 0,
          channels_restored: 0,
          failed_ids: ['123'],
          leftover_webhooks: [],
        });
      }),
      http.patch(`${FEATURES}/bridge.enabled`, async ({ request }) => {
        await off.record(request);
        return HttpResponse.json({
          id: 'bridge.enabled',
          enabled: false,
          platform_disabled: false,
          teardown: { maps_removed: 0, failed_webhooks: [] },
        });
      }),
    );
    await openSettings('bridge');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Switch off the Discord bridge' }),
    );
    const dialog = await screen.findByRole('dialog', {
      name: 'Switch off the whole-server bridge?',
    });
    const remove = await within(dialog).findByRole('checkbox', {
      name: /Also remove what the bridge created/,
    });
    const restore = within(dialog).getByRole('checkbox', { name: /Also put Discord/ });
    expect(remove).not.toBeChecked();
    expect(restore).not.toBeChecked();
    expect(
      within(dialog).getByText(/never deletes anything on Discord that it did not create/),
    ).toBeVisible();
    await userEvent.click(remove);
    expect(
      within(dialog).getByText(/1 of those channels has messages from Discord members/),
    ).toBeVisible();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Switch off and remove' }));
    await waitFor(() => expect(off.calls).toHaveLength(1));
    expect(teardown.calls).toHaveLength(1);
    expect(teardown.calls[0]!.body).toEqual({ remove_created: true, restore_permissions: false });
    expect(off.calls[0]!.body).toEqual({ enabled: false });
    expect(await screen.findByText('123')).toBeVisible();
  });
});
