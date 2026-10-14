import { render } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../../router';
import { resetStore } from '../../../store';
import { http, HttpResponse, server } from '../../../test/server';
import { DEFAULT_PREFS, usePrefsStore } from '../prefs';

export const ME = {
  id: '1',
  username: 'alice',
  global_name: 'Alice',
  display_name: 'Alice',
  email: 'alice@example.com',
  verified: true,
  mfa_enabled: false,
  locale: 'en-US',
};

export const PROFILE = {
  user: { id: '1', username: 'alice', global_name: 'Alice', avatar: null },
  user_profile: { bio: 'Hello there', pronouns: 'she/her', banner: null, accent_color: 0x336699 },
  connected_accounts: [] as { type: string; id: string; name: string; verified: boolean }[],
};

export const fieldError = (field: string, message: string) => ({
  code: 50035,
  message: 'Invalid Form Body',
  errors: { [field]: { _errors: [{ code: 'invalid', message }] } },
});

/** Signed-in baseline: /users/@me and the CSRF token. Tests add the endpoints a section calls. */
export function signedIn(me: Partial<typeof ME> = {}) {
  let out = false;
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    http.get('*/api/v1/users/@me', () =>
      out
        ? HttpResponse.json({ message: 'Unauthorized' }, { status: 401 })
        : HttpResponse.json({ ...ME, ...me }),
    ),
  );
  return {
    /** Subsequent /users/@me calls answer 401, like after a real logout. */
    endSession: () => {
      out = true;
    },
  };
}

export function resetPrefs() {
  // The account copy of the preferences lives in the store: a previous test must not leak into this one.
  resetStore();
  localStorage.clear();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
}

export function openSettings(section: string) {
  const router = createAppRouter(
    createMemoryHistory({ initialEntries: [`/settings/user/${section}`] }),
  );
  render(<RouterProvider router={router} />);
  return router;
}
