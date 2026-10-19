import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { diffProfile, type ProfileDraft } from '../sections/Profile';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, PROFILE, resetPrefs, signedIn } from './helpers';

beforeEach(() => {
  resetPrefs();
  signedIn();
});

const base: ProfileDraft = {
  displayName: '',
  pronouns: '',
  bio: '',
  accent: null,
  status: '',
};

describe('profile decoration and theme (LNG-USET-009)', () => {
  it('diffs the decoration slug and the two theme colours', () => {
    expect(diffProfile(base, { ...base, decoration: 'halo' })).toEqual({
      avatar_decoration: 'halo',
    });
    expect(diffProfile({ ...base, decoration: 'halo' }, { ...base, decoration: '' })).toEqual({
      avatar_decoration: null,
    });
    expect(diffProfile(base, { ...base, themeColors: ['#102030', '#a0b0c0'] })).toEqual({
      theme_colors: [0x102030, 0xa0b0c0],
    });
    expect(diffProfile({ ...base, themeColors: ['#102030', '#a0b0c0'] }, base)).toEqual({
      theme_colors: null,
    });
    expect(diffProfile(base, base)).toEqual({});
  });

  it('offers the approved decorations and saves the chosen one', async () => {
    let body: Record<string, unknown> | undefined;
    server.use(
      http.get('*/api/v1/users/@me/profile', () => HttpResponse.json(PROFILE)),
      http.get('*/api/v1/users/@me/presence', () =>
        HttpResponse.json({ status: 'online', custom_status: null }),
      ),
      http.get('*/api/v1/sr/profile-catalog', () =>
        HttpResponse.json({
          effects: [],
          decorations: [{ slug: 'halo', label_key: 'x' }],
        }),
      ),
      http.patch('*/api/v1/users/@me', async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({});
      }),
    );
    const user = userEvent.setup();
    openSettings('profile');
    const select = await screen.findByRole('combobox', { name: 'Avatar decoration' });
    await user.click(select);
    await user.click(await screen.findByRole('option', { name: 'Halo' }));
    await user.click(screen.getByRole('button', { name: /save changes/i }));
    await waitFor(() => expect(body).toEqual({ avatar_decoration: 'halo' }));
  });
});

describe('empty decoration catalogue (D-290)', () => {
  it('hides the decoration picker while the catalogue has none', async () => {
    server.use(
      http.get('*/api/v1/users/@me/profile', () => HttpResponse.json(PROFILE)),
      http.get('*/api/v1/users/@me/presence', () =>
        HttpResponse.json({ status: 'online', custom_status: null }),
      ),
      http.get('*/api/v1/sr/profile-catalog', () =>
        HttpResponse.json({ effects: [], decorations: [] }),
      ),
    );
    openSettings('profile');
    await screen.findByLabelText(/display name/i);
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByRole('combobox', { name: 'Avatar decoration' })).toBeNull();
  });
});

describe('TTS speed pref (LNG-USET-042)', () => {
  it('clamps the speed and defaults to normal', async () => {
    const { sanitizePrefs } = await import('../prefs');
    expect(sanitizePrefs({}).ttsRate).toBe(100);
    expect(sanitizePrefs({ ttsRate: 500 }).ttsRate).toBe(200);
    expect(sanitizePrefs({ ttsRate: 10 }).ttsRate).toBe(50);
    expect(sanitizePrefs({ ttsRate: 'fast' }).ttsRate).toBe(100);
  });
});
