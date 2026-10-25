// Platform first run (RDY-ONB-001, ENT-UI-022): one test per step, plus the guards that keep it from becoming a wall.
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { http, HttpResponse, server } from '../../test/server';
import { Welcome } from '../shell/Welcome';
import { reopenFirstRun, useFirstRunUi } from '../shell/firstRun';
import { useLoungeUi } from '../store';

const openSettings = vi.fn();
vi.mock('../../settings/user/nav', async (orig) => ({
  ...(await orig<typeof import('../../settings/user/nav')>()),
  useOpenUserSettings: () => openSettings,
}));

let saved: unknown[] = [];

type Handler = Parameters<typeof server.use>[number];

function signedIn(me: Record<string, unknown>, extra: Handler[] = []) {
  saved = [];
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '5', ...me })),
    http.put('*/api/v1/sr/auth/first-run', async ({ request }) => {
      saved.push(await request.json());
      return new HttpResponse(null, { status: 204 });
    }),
    ...extra,
    http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([])),
  );
}

beforeEach(() => {
  openSettings.mockClear();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  useFirstRunUi.getState().hide();
  useLoungeUi.getState().closeDialog();
  sessionStorage.clear();
});

const mount = () =>
  render(
    <ThemeProvider section="lounge">
      <ToastProvider>
        <Welcome />
      </ToastProvider>
    </ThemeProvider>,
  );

const dialog = () => screen.findByRole('dialog', { name: 'Welcome to SIDE ROOM' });
const next = (user: ReturnType<typeof userEvent.setup>) =>
  user.click(screen.getByRole('button', { name: 'Next' }));

describe('the first run shows only for an account that has not closed it', () => {
  it('opens for first_run_done false and tells what the three sections are', async () => {
    signedIn({ first_run_done: false, mfa_enabled: true });
    mount();
    expect(await dialog()).toBeInTheDocument();
    expect(screen.getByText(/The Lounge: chat, voice and video/)).toBeVisible();
    expect(screen.getByText(/The Library: manga, comics and books/)).toBeVisible();
    expect(screen.getByText(/The Cinema: films, shows, anime and Live TV/)).toBeVisible();
    expect(screen.getByText('Step 1 of 3')).toBeVisible();
  });

  it('stays away when it was already closed', async () => {
    signedIn({ first_run_done: true, mfa_enabled: false });
    mount();
    await new Promise((r) => setTimeout(r, 300));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('stays away in the end-to-end build unless the spec opts in', async () => {
    vi.stubEnv('VITE_SR_E2E', '1');
    try {
      signedIn({ first_run_done: false, mfa_enabled: true });
      const { unmount } = mount();
      await new Promise((r) => setTimeout(r, 300));
      expect(screen.queryByRole('dialog')).toBeNull();
      unmount();
      sessionStorage.setItem('sr-e2e-first-run', '1');
      mount();
      expect(await dialog()).toBeInTheDocument();
    } finally {
      vi.unstubAllEnvs();
    }
  });
});

describe('the steps', () => {
  it('appearance: picks a theme and sends to the profile picture settings', async () => {
    signedIn({ first_run_done: false, mfa_enabled: true });
    const user = userEvent.setup();
    mount();
    await dialog();
    await next(user);
    await user.click(screen.getByRole('button', { name: 'Onyx' }));
    expect(usePrefsStore.getState().prefs.theme).toBe('lounge-onyx');
    await user.click(screen.getByRole('button', { name: 'Open profile settings' }));
    expect(openSettings).toHaveBeenCalledWith('profile');
    await waitFor(() => expect(saved).toEqual([{ done: true }]));
  });

  it('two-step sign-in: set up in place, without leaving the Lounge', async () => {
    signedIn({ first_run_done: false, mfa_enabled: false }, [
      http.post('*/api/v1/sr/auth/totp/enroll', () =>
        HttpResponse.json({
          otpauth_uri: 'otpauth://totp/SIDE%20ROOM:alex?secret=JBSWY3DPEHPK3PXP',
          manual_entry_key: 'JBSWY3DPEHPK3PXP',
        }),
      ),
      http.post('*/api/v1/sr/auth/totp/verify', () =>
        HttpResponse.json({ recovery_codes: ['aaaa-bbbb', 'cccc-dddd'] }),
      ),
    ]);
    const user = userEvent.setup();
    mount();
    await dialog();
    await next(user);
    await next(user);
    expect(screen.getByRole('heading', { name: 'Protect your account' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Set it up here' }));
    expect(await screen.findByText('JBSWY3DPEHPK3PXP')).toBeVisible();
    await user.type(screen.getByLabelText('Authentication code'), '123456');
    await user.click(screen.getByRole('button', { name: /Verify/ }));
    expect(await screen.findByText('aaaa-bbbb')).toBeVisible();
    expect(openSettings).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: /saved them/i }));
    // Back in the welcome, which now says it is on.
    expect(await screen.findByText('Two-step sign-in is on. Thank you.')).toBeVisible();
  });

  it('install: offered only where the browser offers it', async () => {
    signedIn({ first_run_done: false, mfa_enabled: true });
    const user = userEvent.setup();
    mount();
    await dialog();
    // No install event yet: the step is not there.
    expect(screen.getByText('Step 1 of 3')).toBeVisible();
    const prompt = vi.fn().mockResolvedValue(undefined);
    await act(async () => {
      const e = new Event('beforeinstallprompt', { cancelable: true });
      Object.assign(e, { prompt, userChoice: Promise.resolve({ outcome: 'accepted' }) });
      window.dispatchEvent(e);
    });
    expect(screen.getByText('Step 1 of 4')).toBeVisible();
    await next(user);
    await next(user);
    await user.click(screen.getByRole('button', { name: 'Install SIDE ROOM' }));
    expect(prompt).toHaveBeenCalled();
  });

  it('notifications: no permission is asked here; the button only opens the settings', async () => {
    const request = vi.fn().mockResolvedValue('granted');
    vi.stubGlobal(
      'Notification',
      Object.assign(vi.fn(), { requestPermission: request, permission: 'default' }),
    );
    try {
      signedIn({ first_run_done: false, mfa_enabled: true });
      const user = userEvent.setup();
      mount();
      await dialog();
      await next(user);
      await next(user);
      expect(screen.getByRole('heading', { name: 'Notifications' })).toBeVisible();
      expect(request).not.toHaveBeenCalled();
      await user.click(screen.getByRole('button', { name: 'Open notification settings' }));
      expect(openSettings).toHaveBeenCalledWith('notifications');
      expect(request).not.toHaveBeenCalled();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("the home server's own guide: shown when its managers wrote one, and opens it", async () => {
    signedIn({ first_run_done: false, mfa_enabled: true }, [
      http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([{ id: '77', name: 'Home' }])),
      http.get('*/api/v1/guilds/77/sr/guide', () =>
        HttpResponse.json({
          guild_id: '77',
          enabled: true,
          welcome_message: 'Hello',
          todos: [],
          resources: [],
        }),
      ),
      http.get('*/api/v1/guilds/77/onboarding', () =>
        HttpResponse.json({ guild_id: '77', enabled: false, default_channel_ids: [], prompts: [] }),
      ),
    ]);
    const user = userEvent.setup();
    mount();
    await dialog();
    await waitFor(() => expect(screen.getByText('Step 1 of 4')).toBeVisible());
    for (let i = 0; i < 3; i++) await next(user);
    expect(screen.getByRole('heading', { name: 'About Home' })).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Choose channels and roles' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Open the server guide' }));
    expect(useLoungeUi.getState().dialog).toEqual({ kind: 'serverGuide', guildId: '77' });
    await waitFor(() => expect(saved).toEqual([{ done: true }]));
  });

  it('a server without a guide adds no step', async () => {
    signedIn({ first_run_done: false, mfa_enabled: true }, [
      http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([{ id: '77', name: 'Home' }])),
      http.get('*/api/v1/guilds/77/sr/guide', () =>
        HttpResponse.json({
          guild_id: '77',
          enabled: false,
          welcome_message: null,
          todos: [],
          resources: [],
        }),
      ),
      http.get('*/api/v1/guilds/77/onboarding', () =>
        HttpResponse.json({ guild_id: '77', enabled: false, default_channel_ids: [], prompts: [] }),
      ),
    ]);
    mount();
    await dialog();
    await new Promise((r) => setTimeout(r, 200));
    expect(screen.getByText('Step 1 of 3')).toBeVisible();
  });
});

describe('nothing in it blocks a person who closes it', () => {
  it('Skip for now closes it at once from the first step and stores that', async () => {
    signedIn({ first_run_done: false, mfa_enabled: false });
    const user = userEvent.setup();
    mount();
    await dialog();
    await user.click(screen.getByRole('button', { name: 'Skip for now' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    await waitFor(() => expect(saved).toEqual([{ done: true }]));
  });

  it('Escape closes it from a later step, and a failed save still lets the person on', async () => {
    signedIn({ first_run_done: false, mfa_enabled: true }, [
      http.put('*/api/v1/sr/auth/first-run', () => new HttpResponse(null, { status: 500 })),
    ]);
    const user = userEvent.setup();
    mount();
    await dialog();
    await next(user);
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('the last step finishes with one click, and Back works', async () => {
    signedIn({ first_run_done: false, mfa_enabled: true });
    const user = userEvent.setup();
    mount();
    await dialog();
    await next(user);
    await next(user);
    await user.click(screen.getByRole('button', { name: 'Back' }));
    expect(screen.getByText('Step 2 of 3')).toBeVisible();
    await next(user);
    await user.click(screen.getByRole('button', { name: 'Start exploring' }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});

describe('bringing it back', () => {
  it('reopenFirstRun stores not done and shows the welcome without a reload', async () => {
    signedIn({ first_run_done: true, mfa_enabled: true });
    mount();
    await new Promise((r) => setTimeout(r, 200));
    expect(screen.queryByRole('dialog')).toBeNull();
    await act(async () => {
      await reopenFirstRun();
    });
    expect(await dialog()).toBeInTheDocument();
    expect(saved).toEqual([{ done: false }]);
  });
});
