import { waitFor } from '@testing-library/react';
import { http, HttpResponse, server } from '../../../test/server';
import { resetStore } from '../../../store';
import { useLoungeUi } from '../../store';
import type { RailEntry } from '../../data';
import { railToSettings, startUserSettingsSync, UNSYNCED_KEY } from '../userSettingsSync';

const accountRail: RailEntry[] = [
  { kind: 'guild', guildId: 'a' },
  { kind: 'guild', guildId: 'b' },
];
const myRail: RailEntry[] = [
  { kind: 'guild', guildId: 'b' },
  { kind: 'guild', guildId: 'a' },
];

type Body = { settings: Record<string, unknown> };

/** A settings endpoint that fails its reads until `up.on` is set; collects the patches. */
function endpoint(up: { on: boolean }, account: Record<string, unknown>) {
  const patches: Body[] = [];
  let reads = 0;
  server.use(
    http.get('*/api/v1/users/@me/settings', () => {
      reads += 1;
      return up.on ? HttpResponse.json({ version: 4, settings: account }) : HttpResponse.error();
    }),
    http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
      const body = (await request.json()) as Body;
      patches.push(body);
      return HttpResponse.json({ version: 5, settings: { ...account, ...body.settings } });
    }),
  );
  return { patches, reads: () => reads };
}

describe('layout sync after a failed first read (SEC-AUD-134)', () => {
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

  it('a_failed_first_read_is_retried_on_online', async () => {
    const up = { on: false };
    const ep = endpoint(up, railToSettings(accountRail));
    const stop = startUserSettingsSync(10);
    await waitFor(() => expect(ep.reads()).toBe(1));
    await new Promise((r) => setTimeout(r, 20));
    up.on = true;
    window.dispatchEvent(new Event('online'));
    await waitFor(() => expect(ep.reads()).toBe(2));
    await waitFor(() => expect(useLoungeUi.getState().railLayout).toEqual(accountRail));
    stop();
  });

  it('a_change_made_before_the_first_read_is_pushed_not_overwritten', async () => {
    const up = { on: false };
    const ep = endpoint(up, railToSettings(accountRail));
    const stop = startUserSettingsSync(10);
    await waitFor(() => expect(ep.reads()).toBe(1));
    await new Promise((r) => setTimeout(r, 20));
    useLoungeUi.getState().setRailLayout(myRail);
    up.on = true;
    window.dispatchEvent(new Event('online'));
    await waitFor(() => expect(ep.patches).toHaveLength(1));
    expect(ep.patches[0]!.settings.guild_positions).toEqual(['b', 'a']);
    expect(useLoungeUi.getState().railLayout).toEqual(myRail);
    stop();
  });

  it('with_nothing_waiting_the_account_copy_still_wins', async () => {
    useLoungeUi.setState({ railLayout: myRail });
    const up = { on: true };
    const ep = endpoint(up, railToSettings(accountRail));
    const stop = startUserSettingsSync(10);
    await waitFor(() => expect(useLoungeUi.getState().railLayout).toEqual(accountRail));
    await new Promise((r) => setTimeout(r, 60));
    expect(ep.patches).toHaveLength(0);
    stop();
  });

  it('the_mark_survives_a_reload_and_is_cleared_after_the_write', async () => {
    const up = { on: false };
    const ep = endpoint(up, railToSettings(accountRail));
    const first = startUserSettingsSync(10);
    await waitFor(() => expect(ep.reads()).toBe(1));
    useLoungeUi.getState().setRailLayout(myRail);
    expect(JSON.parse(localStorage.getItem(UNSYNCED_KEY)!).keys).toContain('guild_positions');
    first();

    // A reload: a fresh page, the layout comes back from the browser copy, the account copy is older.
    resetStore();
    up.on = true;
    const second = startUserSettingsSync(10);
    await waitFor(() => expect(ep.patches).toHaveLength(1));
    expect(ep.patches[0]!.settings.guild_positions).toEqual(['b', 'a']);
    expect(useLoungeUi.getState().railLayout).toEqual(myRail);
    await waitFor(() => expect(localStorage.getItem(UNSYNCED_KEY)).toBeNull());
    second();
  });
});
