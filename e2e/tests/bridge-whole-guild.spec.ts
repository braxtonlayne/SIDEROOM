import type { Page, Route } from '@playwright/test';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { FAKE_BOT_TOKEN, fakeReset } from '../support/bridgeFake';

/**
 * BRG-SYN-004 (wave 35b): the whole-server mirror from the owner's point of view. The first half is real: the owner
 * switches the bridge on and connects the bot against the fake Discord on 8090 (as bridge.spec.ts). The 35b routes
 * (preflight, plan, mode, sync, conflicts, mirrors, teardown-preview) are answered by a small in-test stand-in via
 * `page.route`, because the server side of 35b is built by sync-core, sync-perms and sync-gateway; the integrator
 * replaces the stand-in with the real routes and the fake gateway's guild state (design section 16.4). The shapes are
 * those of apps/web/src/settings/server/bridge/api.ts.
 *
 * Covers: Only bridge these -> prepare plan (preflight, tabs, blockers) -> owner confirms with the plan hash -> sync
 * status -> a Discord admin's permission edit shows in the conflict log and "Apply on SIDE ROOM" opens the role editor
 * prefilled without saving -> back to Only bridge these -> switch off with the whole-server options.
 *
 * Skipped when the fake Discord is not up.
 */

interface Stand {
  mode: 'selective' | 'whole_guild';
  conflicts: object[];
  modeBody: unknown;
  offBody: unknown;
}

const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

async function installStandIn(page: Page, guildId: string, roleId: string, st: Stand) {
  const B = `**/api/v1/guilds/${guildId}/bridge`;
  const now = () => Math.floor(Date.now() / 1000);
  // The real status, with the mode of the stand-in.
  await page.route(`${B}/status`, async (route) => {
    const res = await route.fetch();
    const body = await res.json();
    if (body.bridge) body.bridge.mode = st.mode;
    await json(route, body);
  });
  await page.route(`${B}/preflight`, (r) =>
    json(r, {
      checks: [
        { id: 'bot_admin', ok: true, blocker: true },
        { id: 'bot_top_role', ok: true, blocker: true },
        { id: 'intents', ok: true, blocker: true },
        { id: 'caps', ok: true, blocker: true },
      ],
    }),
  );
  await page.route(`${B}/plan`, (r) => json(r, { plan_id: 'p1' }));
  await page.route(`${B}/plan/p1`, (r) =>
    json(r, {
      plan_id: 'p1',
      status: 'ready',
      plan_hash: 'hash-1',
      created_at: now(),
      expires_at: now() + 900,
      seed_structure: 'sr',
      blockers: [],
      roles: [
        { id: 'r1', kind: 'role', name: 'Manager', discord_name: 'Manager', action: 'match' },
        { id: 'r2', kind: 'role', name: 'Regulars', action: 'create_there' },
      ],
      categories: [],
      channels: [{ id: 'c1', kind: 'channel', name: 'general', discord_name: 'general', action: 'match' }],
      threads: [],
      discord_changes: [{ id: 'x1', target: 'role', name: 'Manager', text: 'Role Manager on Discord gets Manage Server' }],
      imports: [],
      not_mirrored: [],
      estimate: { discord_calls: 12, seconds: 30 },
    }),
  );
  await page.route(`${B}/mode`, async (route) => {
    st.modeBody = route.request().postDataJSON();
    st.mode = (st.modeBody as { mode: Stand['mode'] }).mode;
    await route.fulfill({ status: 204 });
  });
  await page.route(`${B}/sync`, (r) =>
    json(r, {
      mode: st.mode,
      sync_state: 'live',
      bot: { administrator: true, top_role: true, intents: true },
      last_reconcile_at: now(),
      reconcile_available_at: null,
      queue_depth: 0,
      dead_ops: 0,
      rate_limit_wait_secs: 0,
      counts: { mapped: 2, one_sided: 0, degraded: 0, frozen: 0 },
      seed: null,
    }),
  );
  await page.route(`${B}/conflicts**`, (r) => json(r, st.conflicts));
  await page.route(`${B}/conflicts/*/dismiss`, async (route) => {
    st.conflicts = [];
    await route.fulfill({ status: 204 });
  });
  await page.route(`${B}/mirrors`, (r) =>
    json(r, {
      mode: st.mode,
      roles: [{ role_id: roleId, discord_role_id: 'D1', discord_only: [], state: 'mapped' }],
      channels: [],
    }),
  );
  await page.route(`${B}/teardown-preview`, (r) =>
    json(r, {
      created: { roles: 1, categories: 0, channels: 2, threads: 0 },
      channels_with_discord_messages: 1,
      snapshot_available: true,
    }),
  );
  await page.route(`**/api/v1/guilds/${guildId}/sr/features/bridge.enabled`, async (route) => {
    if (route.request().method() !== 'PATCH') return route.continue();
    const body = route.request().postDataJSON() as { enabled: boolean; teardown?: unknown };
    if (body.enabled || !body.teardown) return route.continue();
    st.offBody = body;
    await json(route, {
      id: 'bridge.enabled',
      name: 'Discord bridge',
      description: '',
      section: 'Integrations, more',
      phase: 35,
      enabled: false,
      default_enabled: false,
      platform_disabled: false,
      owner_only: true,
      settings: {},
      schema: { fields: [] },
      teardown: { maps_removed: 0, failed_webhooks: [], residue: [] },
    });
  });
}

test.describe('the whole-server Discord mirror', () => {
  test('prepare, confirm, watch a revert in the conflict log, go back, switch off with options', async ({
    scenario,
    playwright,
  }) => {
    test.setTimeout(180_000);
    const control = await playwright.request.newContext();
    try {
      const up = await control
        .get(`${process.env.SR_FAKE_DISCORD_URL ?? 'http://127.0.0.1:8090'}/__fake/state`)
        .catch(() => null);
      test.skip(!up?.ok(), 'The fake Discord (port 8090) is not running.');
      await fakeReset(control);

      const { a, guildId } = await scenario.ownerAndMember();
      const owner = a.page;
      const role = await apiPost<{ id: string }>(a.context.request, `/guilds/${guildId}/roles`, {
        name: 'Manager',
        permissions: String(1 << 5),
      });
      await apiPatch(a.context.request, `/guilds/${guildId}/roles/${role.id}`, { hoist: true });

      const st: Stand = { mode: 'selective', conflicts: [], modeBody: null, offBody: null };
      await installStandIn(owner, guildId, role.id, st);

      // Real: switch on and connect the bot.
      await owner.goto(`/settings/server/${guildId}/features`);
      await owner.getByRole('switch', { name: 'Enable Discord bridge' }).click();
      await owner.getByRole('button', { name: 'Discord bridge', exact: true }).click();
      await owner.getByLabel('Bot token').fill(FAKE_BOT_TOKEN);
      await owner.getByRole('button', { name: 'Connect the bot' }).click();
      await expect(owner.getByText(/^Connected to the Discord server/)).toBeVisible();

      // Mode card and plan.
      const mode = owner.getByTestId('bridge-mode-card');
      await expect(mode.getByText('Mode: Only bridge these')).toBeVisible();
      await mode.getByRole('button', { name: 'Prepare the whole-server plan' }).click();
      await expect(owner.getByTestId('bridge-preflight').getByText('Ready').first()).toBeVisible();
      await owner.getByRole('button', { name: 'Build the plan' }).click();
      await expect(owner.getByRole('tab', { name: 'Roles (2)' })).toBeVisible();
      await owner.getByRole('tab', { name: 'Changes on Discord (1)' }).click();
      await expect(owner.getByText('Role Manager on Discord gets Manage Server')).toBeVisible();

      // Confirm: the dialog carries the privacy sentence; the plan hash goes back.
      await owner.getByRole('button', { name: 'Confirm the plan' }).click();
      const dialog = owner.getByRole('dialog', { name: 'Bridge the whole server?' });
      await expect(dialog).toContainText('decided by Discord');
      await dialog.getByRole('button', { name: 'Confirm and start' }).click();
      await expect(owner.getByTestId('bridge-sync-status').getByText('In sync')).toBeVisible();
      expect(st.modeBody).toEqual({ mode: 'whole_guild', plan_id: 'p1', plan_hash: 'hash-1' });
      await expect(owner.getByText(/Everything in this server except direct messages/)).toBeVisible();

      // A Discord admin edits a permission; the bridge undid it and logged it.
      st.conflicts = [
        {
          id: 'k1',
          kind: 'permission_reverted',
          object_kind: 'role',
          object_id: role.id,
          object_name: 'Manager',
          field: 'permissions',
          discord_value: 'added Ban Members',
          sr_value: 'no Ban Members',
          discord_permissions: ['MANAGE_GUILD', 'BAN_MEMBERS'],
          discord_actor: 'Rhea',
          bridge_did: 'Put it back',
          count: 1,
          first_at: Math.floor(Date.now() / 1000),
          last_at: Math.floor(Date.now() / 1000),
        },
      ];
      await owner.reload();
      const log = owner.getByTestId('bridge-conflicts');
      await expect(log.getByText('Permission change undone: Manager')).toBeVisible();
      await expect(log.getByText('By Rhea')).toBeVisible();
      await log.getByRole('button', { name: 'Apply on SIDE ROOM: Manager' }).click();
      await expect(owner.getByText('Prefilled from Discord’s change')).toBeVisible();
      // Nothing was saved by the click: the role still has only Manage Server.
      const saved = await a.context.request.get(`/api/v1/guilds/${guildId}/roles`);
      const roles = (await saved.json()) as { id: string; permissions: string }[];
      expect(roles.find((r) => r.id === role.id)?.permissions).toBe(String(1 << 5));

      // Back to Only bridge these (nothing deleted), then switch off with the options.
      await owner.goto(`/settings/server/${guildId}/bridge`);
      await owner.getByRole('button', { name: 'Go back to Only bridge these' }).click();
      await owner.getByRole('button', { name: 'Go back', exact: true }).click();
      await expect(owner.getByText('Mode: Only bridge these')).toBeVisible();
      expect(st.modeBody).toEqual({ mode: 'selective' });

      // (whole mode again so the teardown dialog offers the options)
      st.mode = 'whole_guild';
      await owner.reload();
      await owner.getByRole('button', { name: 'Switch off the Discord bridge' }).click();
      const off = owner.getByRole('dialog', { name: 'Switch off the whole-server bridge?' });
      await off.getByRole('checkbox', { name: /Also remove what the bridge created/ }).check();
      await expect(off).toContainText('1 of those channels has messages from Discord members');
      await off.getByRole('button', { name: 'Switch off and remove' }).click();
      await expect(owner.getByText('The Discord bridge is off')).toBeVisible();
      expect(st.offBody).toEqual({
        enabled: false,
        teardown: { remove_created: true, restore_permissions: false },
      });
    } finally {
      await control.dispose();
    }
  });
});
