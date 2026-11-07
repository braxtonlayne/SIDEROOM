// SEC-AUD-037 (wave 54): a failed first read of the settings blob never lets the older account copy replace a
// change made in this browser, and the Activity Privacy switches stay locked until the real values are known.
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { resetStore, useStore } from '../../../store';
import { http, HttpResponse, server } from '../../../test/server';
import { DEFAULT_PREFS, usePrefsStore } from '../prefs';
import { PREFS_KEY, startPrefsSync, UNSYNCED_KEY } from '../prefsSync';
import { ActivitySection } from '../sections/Activity';

const changed = !DEFAULT_PREFS.dmToasts;
const fail500 = () => HttpResponse.json({ message: 'boom' }, { status: 500 });

let patches: unknown[];
beforeEach(() => {
  resetStore();
  localStorage.clear();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  patches = [];
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
      const body = (await request.json()) as { settings: Record<string, unknown> };
      patches.push(body.settings);
      return HttpResponse.json({ version: 9, settings: body.settings });
    }),
  );
});

describe('preferences sync after a failed first read', () => {
  it('reads again when the network returns and pushes the change made meanwhile', async () => {
    let up = false;
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        up
          ? HttpResponse.json({ version: 3, settings: { [PREFS_KEY]: { ...DEFAULT_PREFS } } })
          : fail500(),
      ),
    );
    const stop = startPrefsSync();
    usePrefsStore.getState().save({ ...DEFAULT_PREFS, dmToasts: changed });
    expect(localStorage.getItem(UNSYNCED_KEY)).toBe('1');
    await new Promise((r) => setTimeout(r, 20));
    expect(patches).toEqual([]);

    up = true;
    window.dispatchEvent(new Event('online'));
    await waitFor(() => expect(patches).toHaveLength(1));
    expect((patches[0] as Record<string, { dmToasts: boolean }>)[PREFS_KEY]!.dmToasts).toBe(
      changed,
    );
    // The older account copy did not win.
    expect(usePrefsStore.getState().prefs.dmToasts).toBe(changed);
    await waitFor(() => expect(localStorage.getItem(UNSYNCED_KEY)).toBeNull());
    stop();
  });

  it('a change still waiting from an earlier page is pushed, not overwritten, on the next load', async () => {
    localStorage.setItem(UNSYNCED_KEY, '1');
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, dmToasts: changed } });
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 3, settings: { [PREFS_KEY]: { ...DEFAULT_PREFS } } }),
      ),
    );
    const stop = startPrefsSync();
    await waitFor(() => expect(patches).toHaveLength(1));
    expect(usePrefsStore.getState().prefs.dmToasts).toBe(changed);
    stop();
  });

  it("another account's waiting change on a shared browser is dropped, and this account's copy wins", async () => {
    localStorage.setItem(UNSYNCED_KEY, '111');
    useStore.setState({ currentUserId: '222' });
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, dmToasts: changed } });
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 3, settings: { [PREFS_KEY]: { ...DEFAULT_PREFS } } }),
      ),
    );
    const stop = startPrefsSync();
    await waitFor(() =>
      expect(usePrefsStore.getState().prefs.dmToasts).toBe(DEFAULT_PREFS.dmToasts),
    );
    expect(patches).toEqual([]);
    expect(localStorage.getItem(UNSYNCED_KEY)).toBeNull();
    stop();
  });

  it("this account's own waiting change is pushed, and the mark names the account", async () => {
    useStore.setState({ currentUserId: '222' });
    localStorage.setItem(UNSYNCED_KEY, '222');
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, dmToasts: changed } });
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 3, settings: { [PREFS_KEY]: { ...DEFAULT_PREFS } } }),
      ),
    );
    const stop = startPrefsSync();
    await waitFor(() => expect(patches).toHaveLength(1));
    expect(usePrefsStore.getState().prefs.dmToasts).toBe(changed);
    stop();
  });

  it('with nothing waiting, the account copy wins as before', async () => {
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, dmToasts: changed } });
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 3, settings: { [PREFS_KEY]: { ...DEFAULT_PREFS } } }),
      ),
    );
    const stop = startPrefsSync();
    await waitFor(() =>
      expect(usePrefsStore.getState().prefs.dmToasts).toBe(DEFAULT_PREFS.dmToasts),
    );
    expect(patches).toEqual([]);
    stop();
  });
});

describe('Activity Privacy switches', () => {
  it('stay locked with an error and Try again while the account copy cannot be read', async () => {
    let fails = true;
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        fails
          ? fail500()
          : HttpResponse.json({ version: 2, settings: { activity: { share_cinema: false } } }),
      ),
    );
    render(<ActivitySection prefs={DEFAULT_PREFS} setPrefs={() => undefined} />);
    const notice = (await screen.findByText(/sharing choices could not be loaded/)).closest(
      '[role="alert"]',
    ) as HTMLElement;
    const watching = screen.getByRole('switch', { name: /^Share what I.m watching/ });
    expect(watching).toBeDisabled();
    fails = false;
    await userEvent.click(within(notice).getByRole('button', { name: 'Try again' }));
    await waitFor(() =>
      expect(screen.getByRole('switch', { name: /^Share what I.m watching/ })).toBeEnabled(),
    );
    expect(screen.getByRole('switch', { name: /^Share what I.m watching/ })).not.toBeChecked();
    expect(screen.queryByText(/sharing choices could not be loaded/)).toBeNull();
    expect(patches).toEqual([]);
  });
});
