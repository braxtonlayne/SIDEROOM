// G-79: the staff badge and platform-DM switches write nested keys of the synced settings blob.
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useStore } from '../../../store';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, PROFILE, resetPrefs, signedIn } from './helpers';

function blob(settings: Record<string, unknown>, onPatch?: (b: unknown) => void) {
  server.use(
    http.get('*/api/v1/users/@me/settings', () => HttpResponse.json({ version: 3, settings })),
    http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
      const body = (await request.json()) as { settings: Record<string, unknown> };
      onPatch?.(body);
      return HttpResponse.json({ version: 4, settings: { ...settings, ...body.settings } });
    }),
  );
}

function profile(badges: { id: string }[]) {
  server.use(
    http.get('*/api/v1/users/@me/profile', () => HttpResponse.json({ ...PROFILE, badges })),
  );
}

beforeEach(() => {
  resetPrefs();
  signedIn();
});

describe('platform announcements by DM (notifications.platform_dm)', () => {
  it('is on by default and saves an explicit false as a merge patch', async () => {
    let sent: unknown;
    blob({}, (b) => (sent = b));
    openSettings('notifications');
    const sw = await screen.findByRole('switch', {
      name: /Platform announcements by direct message/,
    });
    expect(sw).toBeChecked();
    await userEvent.setup().click(sw);
    await waitFor(() =>
      expect(sent).toEqual({ settings: { notifications: { platform_dm: false } } }),
    );
    await waitFor(() => expect(sw).not.toBeChecked());
  });

  it('follows SR_USER_SETTINGS_UPDATE from another session', async () => {
    blob({ notifications: { platform_dm: false } });
    openSettings('notifications');
    const sw = await screen.findByRole('switch', {
      name: /Platform announcements by direct message/,
    });
    await waitFor(() => expect(sw).not.toBeChecked());
    useStore
      .getState()
      .applyDispatch('SR_USER_SETTINGS_UPDATE', { version: 9, settings: { notifications: {} } });
    await waitFor(() => expect(sw).toBeChecked());
  });
});

describe('staff badge (staff.show_badge)', () => {
  it('is hidden for an account without the staff badge', async () => {
    blob({});
    profile([]);
    openSettings('profile');
    await screen.findByLabelText('About me');
    expect(screen.queryByRole('switch', { name: /Staff badge on my profile/ })).toBeNull();
  });

  it('shows for staff and turns the badge off', async () => {
    let sent: unknown;
    blob({}, (b) => (sent = b));
    profile([{ id: 'staff' }]);
    openSettings('profile');
    const sw = await screen.findByRole('switch', { name: /Staff badge on my profile/ });
    expect(sw).toBeChecked();
    await userEvent.setup().click(sw);
    await waitFor(() => expect(sent).toEqual({ settings: { staff: { show_badge: false } } }));
  });

  it('stays visible once hidden (the server then drops the badge) so it can be turned back on', async () => {
    blob({ staff: { show_badge: false } });
    profile([]);
    openSettings('profile');
    const sw = await screen.findByRole('switch', { name: /Staff badge on my profile/ });
    expect(sw).not.toBeChecked();
  });
});
