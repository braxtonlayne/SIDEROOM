import { waitFor } from '@testing-library/react';
import { http, HttpResponse, server } from '../../test/server';
import { resetStore, useStore } from '../../store';
import { useLoungeUi } from '../store';
import type { RailEntry } from '../data';
import {
  diffSettings,
  railFromSettings,
  railToSettings,
  startUserSettingsSync,
} from '../shell/userSettingsSync';

const rail: RailEntry[] = [
  { kind: 'guild', guildId: 'a' },
  { kind: 'folder', id: '1234', name: 'Games', color: '#5865f2', guildIds: ['b', 'c'] },
  { kind: 'guild', guildId: 'd' },
];

describe('rail <-> settings (Discord shapes)', () => {
  it('writes guild_positions flat and guild_folders with null ids for bare servers', () => {
    const s = railToSettings(rail);
    expect(s.guild_positions).toEqual(['a', 'b', 'c', 'd']);
    expect(s.guild_folders).toEqual([
      { id: null, name: null, color: null, guild_ids: ['a'] },
      { id: 1234, name: 'Games', color: 0x5865f2, guild_ids: ['b', 'c'] },
      { id: null, name: null, color: null, guild_ids: ['d'] },
    ]);
  });

  it('round-trips, and falls back to guild_positions alone', () => {
    expect(railFromSettings({ ...railToSettings(rail) })).toEqual(rail);
    expect(railFromSettings({ guild_positions: ['x', 'y'] })).toEqual([
      { kind: 'guild', guildId: 'x' },
      { kind: 'guild', guildId: 'y' },
    ]);
    expect(railFromSettings({})).toBeUndefined();
  });

  it('ignores malformed folder entries', () => {
    expect(
      railFromSettings({ guild_folders: [null, { id: 1, guild_ids: 'x' }, { id: 2 }] }),
    ).toEqual([]);
  });
});

describe('diffSettings', () => {
  it('replaces arrays and scalars, diffs records with null for removals', () => {
    const patch = diffSettings(
      {
        guild_positions: ['a', 'b'],
        right_panel_width: 300,
        collapsed_categories: { c1: true },
        last_channel_by_guild: { g: 'x' },
      },
      {
        guild_positions: ['b', 'a'],
        right_panel_width: 300,
        collapsed_categories: { c1: true, c2: true },
        last_channel_by_guild: { g: 'x' },
      },
    );
    expect(patch).toEqual({ guild_positions: ['a', 'b'], collapsed_categories: { c2: null } });
  });

  it('is empty when nothing changed', () => {
    expect(diffSettings({ a: 1 }, { a: 1 })).toEqual({});
  });
});

describe('startUserSettingsSync', () => {
  beforeEach(() => {
    server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
    resetStore();
    localStorage.clear();
    useLoungeUi.setState({
      railLayout: null,
      collapsedCategories: {},
      lastChannelByGuild: {},
      rightPanelWidth: 240,
    });
  });

  it('applies the server layout over the local cache on load', async () => {
    useLoungeUi.setState({ railLayout: [{ kind: 'guild', guildId: 'local' }] });
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 3, settings: railToSettings(rail) }),
      ),
    );
    const stop = startUserSettingsSync(10);
    await waitFor(() => expect(useLoungeUi.getState().railLayout).toEqual(rail));
    stop();
  });

  it('migrates a local-only layout up, debounced into one merge patch', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        bodies.push(body);
        return HttpResponse.json({ version: 1, settings: body.settings });
      }),
    );
    useLoungeUi.setState({ railLayout: rail, rightPanelWidth: 300 });
    const stop = startUserSettingsSync(10);
    await waitFor(() => expect(bodies).toHaveLength(1));
    const sent = (bodies[0] as { settings: Record<string, unknown> }).settings;
    expect(sent).toMatchObject({ guild_positions: ['a', 'b', 'c', 'd'], right_panel_width: 300 });
    // Untouched defaults are not written.
    expect(sent).not.toHaveProperty('collapsed_categories');
    // The echo of our own save must not produce a second patch.
    await new Promise((r) => setTimeout(r, 60));
    expect(bodies).toHaveLength(1);
    stop();
  });

  it('writes a drag as one patch and sends nulls for removed collapsed categories', async () => {
    const bodies: { settings: Record<string, unknown> }[] = [];
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({
          version: 2,
          settings: { ...railToSettings(rail), collapsed_categories: { k1: true } },
        }),
      ),
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        bodies.push(body);
        return HttpResponse.json({ version: 3, settings: {} });
      }),
    );
    const stop = startUserSettingsSync(10);
    await waitFor(() => expect(useLoungeUi.getState().railLayout).toEqual(rail));
    // Several quick changes coalesce.
    useLoungeUi.getState().setRailLayout([rail[2]!, rail[1]!, rail[0]!]);
    useLoungeUi.getState().toggleCategory('k1');
    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0]!.settings.guild_positions).toEqual(['d', 'b', 'c', 'a']);
    expect(bodies[0]!.settings.collapsed_categories).toEqual({ k1: null });
    stop();
  });

  it('applies SR_USER_SETTINGS_UPDATE from another session without writing it back', async () => {
    let patches = 0;
    server.use(
      http.patch('*/api/v1/users/@me/settings', () => {
        patches += 1;
        return HttpResponse.json({ version: 9, settings: {} });
      }),
    );
    const stop = startUserSettingsSync(10);
    await waitFor(() => expect(useStore.getState().userSettings).not.toBeNull());
    useStore
      .getState()
      .applyDispatch('SR_USER_SETTINGS_UPDATE', { version: 5, settings: railToSettings(rail) });
    await waitFor(() => expect(useLoungeUi.getState().railLayout).toEqual(rail));
    await new Promise((r) => setTimeout(r, 60));
    expect(patches).toBe(0);
    stop();
  });

  it('keeps working from localStorage when the blob cannot be loaded', async () => {
    server.use(
      http.get('*/api/v1/users/@me/settings', () => HttpResponse.error()),
      http.patch('*/api/v1/users/@me/settings', () => {
        throw new Error('must not write before a load succeeded');
      }),
    );
    useLoungeUi.setState({ railLayout: rail });
    const stop = startUserSettingsSync(10);
    useLoungeUi.getState().setRailLayout([rail[1]!, rail[0]!, rail[2]!]);
    await new Promise((r) => setTimeout(r, 60));
    expect(useLoungeUi.getState().railLayout?.[0]?.kind).toBe('folder');
    stop();
  });
});

describe('settings reducer', () => {
  beforeEach(() => resetStore());

  it('ignores an older version', () => {
    const { applyDispatch } = useStore.getState();
    applyDispatch('SR_USER_SETTINGS_UPDATE', { version: 4, settings: { a: 1 } });
    applyDispatch('SR_USER_SETTINGS_UPDATE', { version: 2, settings: { a: 2 } });
    expect(useStore.getState().userSettings).toEqual({ version: 4, settings: { a: 1 } });
  });
});
