import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { afterAll, beforeAll } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../../router';
import { http, HttpResponse, server } from '../../../test/server';
import { useLoungeUi } from '../../../lounge/store';
import { usePrefsStore } from '../prefs';
import { openSettings, PROFILE, resetPrefs, signedIn } from './helpers';
import { warmLoungeRoute } from '../../../test/warmRoutes';

// The Lounge runs against the deterministic mock seed here (ESC returns to it).
beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

beforeEach(() => {
  resetPrefs();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
});

// After the env stubs above, so the warmed modules see the same env as the tests.
warmLoungeRoute();

describe('User Settings shell', () => {
  it('lists the sections in two groups with a roving tab stop and navigates by URL', async () => {
    signedIn();
    server.use(
      http.get('*/api/v1/users/@me/profile', () => HttpResponse.json(PROFILE)),
      http.get('*/api/v1/sr/auth/sessions', () => HttpResponse.json([])),
      http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json([])),
    );
    const user = userEvent.setup();
    const router = openSettings('account');
    const nav = await screen.findByRole('navigation', { name: 'User Settings' });
    expect(within(nav).getByText('App Settings')).toBeInTheDocument();
    const buttons = within(nav).getAllByRole('button');
    expect(buttons.filter((b) => b.tabIndex === 0)).toHaveLength(3); // active section + What's New + Log Out
    expect(within(nav).getByRole('button', { name: 'My Account' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    await user.click(within(nav).getByRole('button', { name: 'Keybinds' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/settings/user/keybinds'));
    expect(await screen.findByRole('heading', { name: 'Keybinds' })).toBeInTheDocument();
  });

  it('redirects /settings/user and unknown sections to My Account', async () => {
    signedIn();
    server.use(
      http.get('*/api/v1/users/@me/profile', () => HttpResponse.json(PROFILE)),
      http.get('*/api/v1/sr/auth/sessions', () => HttpResponse.json([])),
      http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json([])),
    );
    const router = createAppRouter(createMemoryHistory({ initialEntries: ['/settings/user'] }));
    render(<RouterProvider router={router} />);
    await waitFor(() => expect(router.state.location.pathname).toBe('/settings/user/account'));
    await router.navigate({ to: '/settings/user/$section', params: { section: 'bogus' } });
    await waitFor(() => expect(router.state.location.pathname).toBe('/settings/user/account'));
  });

  it('shows the keybind table with Change buttons and the voice keybind recorders', async () => {
    signedIn();
    openSettings('keybinds');
    expect((await screen.findAllByText('Open user settings')).length).toBeGreaterThan(0);
    expect(screen.getAllByRole('img', { name: /plus ,|Ctrl plus K/ }).length).toBeGreaterThan(0);
    expect(screen.getByRole('button', { name: /Change Open user settings/ })).toBeInTheDocument();
    // Voice keybinds are recorded on this page too.
    expect(screen.getByRole('button', { name: 'Record Push to Talk keybind' })).toBeInTheDocument();
  });

  it('lists the language and region choices', async () => {
    signedIn();
    openSettings('language');
    const select = await screen.findByRole('combobox', { name: 'Region and date format' });
    expect(select).not.toBeDisabled();
    expect(select).toHaveValue('Same as my browser');
    expect(screen.getByText(/English only for now/)).toBeInTheDocument();
  });

  it('holds ESC while there are unsaved changes, Reset clears them, Save persists and ESC then leaves', async () => {
    signedIn();
    const user = userEvent.setup();
    const router = openSettings('advanced');
    const dev = await screen.findByRole('switch', { name: /Developer mode/ });
    await user.click(dev);
    expect(await screen.findByRole('alert')).toHaveTextContent('unsaved changes');

    await user.keyboard('{Escape}');
    expect(router.state.location.pathname).toBe('/settings/user/advanced');
    expect(screen.getByRole('alert')).toHaveAttribute('data-nudge');

    await user.click(screen.getByRole('button', { name: 'Reset' }));
    expect(screen.queryByRole('alert')).toBeNull();
    expect(dev).not.toBeChecked();

    await user.click(dev);
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(usePrefsStore.getState().prefs.developerMode).toBe(true);
    await user.keyboard('{Escape}');
    // Deep link with no previous location falls back to the Lounge.
    await waitFor(() => expect(router.state.location.pathname.startsWith('/channels')).toBe(true));
  });

  it('keeps unsaved changes when switching sections', async () => {
    signedIn();
    const user = userEvent.setup();
    openSettings('advanced');
    await user.click(await screen.findByRole('switch', { name: /Developer mode/ }));
    const nav = screen.getByRole('navigation', { name: 'User Settings' });
    await user.click(within(nav).getByRole('button', { name: 'Language' }));
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    await user.click(within(nav).getByRole('button', { name: 'Advanced' }));
    expect(screen.getByRole('switch', { name: /Developer mode/ })).toBeChecked();
  });

  it('opens from the Lounge gear and Ctrl+, and ESC returns to the previous location', async () => {
    signedIn();
    server.use(
      http.get('*/api/v1/users/@me/profile', () => HttpResponse.json(PROFILE)),
      http.get('*/api/v1/sr/auth/sessions', () => HttpResponse.json([])),
      http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json([])),
    );
    const user = userEvent.setup();
    const router = createAppRouter(
      createMemoryHistory({ initialEntries: ['/channels/1001/2001'] }),
    );
    render(<RouterProvider router={router} />);
    await user.click(await screen.findByRole('button', { name: 'User Settings' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/settings/user/account'));
    await screen.findByRole('heading', { name: 'My Account' });
    await user.keyboard('{Escape}');
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/1001/2001'));

    await screen.findByRole('navigation', { name: 'Servers' });
    await user.keyboard('{Control>},{/Control}');
    await waitFor(() => expect(router.state.location.pathname).toBe('/settings/user/account'));
  });

  it('Log Out asks first, cancel keeps you here, confirm signs out', async () => {
    const s = signedIn();
    server.use(
      http.post('*/api/v1/sr/auth/logout', () => {
        s.endSession();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    const router = openSettings('language');
    const nav = await screen.findByRole('navigation', { name: 'User Settings' });
    await user.click(within(nav).getByRole('button', { name: 'Log Out' }));
    let dialog = await screen.findByRole('dialog', { name: 'Log out?' });
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Log out?' })).toBeNull());
    expect(router.state.location.pathname).toBe('/settings/user/language');

    await user.click(within(nav).getByRole('button', { name: 'Log Out' }));
    dialog = await screen.findByRole('dialog', { name: 'Log out?' });
    await user.click(within(dialog).getByRole('button', { name: 'Log Out' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/login'));
  });

  it('shows the error when logging out fails', async () => {
    signedIn();
    server.use(
      http.post('*/api/v1/sr/auth/logout', () =>
        HttpResponse.json({ message: 'Server unavailable.' }, { status: 503 }),
      ),
    );
    const user = userEvent.setup();
    const router = openSettings('language');
    const nav = await screen.findByRole('navigation', { name: 'User Settings' });
    await user.click(within(nav).getByRole('button', { name: 'Log Out' }));
    const dialog = await screen.findByRole('dialog', { name: 'Log out?' });
    await user.click(within(dialog).getByRole('button', { name: 'Log Out' }));
    expect(await screen.findByText('Server unavailable.')).toBeInTheDocument();
    expect(router.state.location.pathname).toBe('/settings/user/language');
  });

  it('renders no Nitro, boost, billing, gift or quest sections (LNG-USET-057)', async () => {
    signedIn();
    openSettings('language');
    const nav = await screen.findByRole('navigation', { name: 'User Settings' });
    expect(nav.textContent).not.toMatch(
      /nitro|boost|subscription|gift|billing|family|clips|quest/i,
    );
  });
});

describe('User Settings accessibility (axe)', () => {
  const API = [
    http.get('*/api/v1/users/@me/profile', () => HttpResponse.json(PROFILE)),
    http.get('*/api/v1/users/@me/presence', () =>
      HttpResponse.json({ status: 'online', custom_status: null }),
    ),
    http.get('*/api/v1/sr/auth/sessions', () =>
      HttpResponse.json([
        {
          id: '1',
          device_name: 'Here',
          created_at: '2026-01-01T00:00:00Z',
          last_seen_at: '2026-01-01T00:00:00Z',
          expires_at: '2026-02-01T00:00:00Z',
          current: true,
        },
      ]),
    ),
    http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json([])),
    http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([])),
    http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
    http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
  ];
  it.each([
    ['account', 'Two-factor authentication'],
    ['profile', 'Main profile'],
    ['privacy', 'Blocked users'],
    ['appearance', 'Theme'],
    ['notifications', 'Default notification level'],
    ['keybinds', 'Navigation'],
    ['language', 'Language'],
    ['advanced', 'Developer mode'],
  ])('%s has no violations', async (section, marker) => {
    signedIn();
    server.use(...API);
    openSettings(section);
    await screen.findAllByText(marker);
    await waitFor(() => expect(screen.queryByText(/^Loading/)).toBeNull());
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
