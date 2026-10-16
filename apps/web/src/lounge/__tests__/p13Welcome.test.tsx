import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ThemeProvider } from '@sideroom/ui';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { http, HttpResponse, server } from '../../test/server';
import { isNewAccount, Welcome } from '../shell/Welcome';

const openSettings = vi.fn();
vi.mock('../../settings/user/nav', async (orig) => ({
  ...(await orig<typeof import('../../settings/user/nav')>()),
  useOpenUserSettings: () => openSettings,
}));

const EPOCH = 1_767_225_600_000;
const idAt = (ms: number) => String((BigInt(ms - EPOCH) << 22n) | 5n);

beforeEach(() => {
  openSettings.mockClear();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
});

describe('isNewAccount', () => {
  it('is true for a snowflake minted in the last two weeks only', () => {
    const now = EPOCH + 40 * 86_400_000;
    expect(isNewAccount(idAt(now - 86_400_000), now)).toBe(true);
    expect(isNewAccount(idAt(now - 30 * 86_400_000), now)).toBe(false);
    expect(isNewAccount('abc', now)).toBe(false);
    expect(isNewAccount(undefined, now)).toBe(false);
  });
});

const mount = () =>
  render(
    <ThemeProvider section="lounge">
      <Welcome />
    </ThemeProvider>,
  );

describe('Welcome (ENT-UI-022)', () => {
  it('greets a new account once, nudges for two-step sign-in and remembers it was closed', async () => {
    server.use(
      http.get('*/api/v1/users/@me', () =>
        HttpResponse.json({ id: idAt(Date.now() - 1000), mfa_enabled: false }),
      ),
    );
    const user = userEvent.setup();
    mount();
    expect(
      await screen.findByRole('dialog', { name: 'Welcome to SIDE ROOM' }, { timeout: 4000 }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Onyx' }));
    expect(usePrefsStore.getState().prefs.theme).toBe('lounge-onyx');
    await user.click(screen.getByRole('button', { name: 'Set up two-step sign-in' }));
    expect(openSettings).toHaveBeenCalledWith('account');
    expect(usePrefsStore.getState().prefs.welcomeDone).toBe(true);
  });

  it('stays away for an older account and when it was already done', async () => {
    server.use(
      http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', mfa_enabled: true })),
    );
    mount();
    await new Promise((r) => setTimeout(r, 1800));
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
