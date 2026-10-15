import { http, HttpResponse, server } from '../test/server';
import { DEFAULT_PREFS, usePrefsStore } from '../settings/user/prefs';
import { adoptAccountLocale, chooseLanguage } from './accountLocale';
import { setLocale } from './runtime';

beforeEach(() => {
  localStorage.clear();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
});
afterEach(async () => {
  await setLocale('en');
  localStorage.clear();
});

describe('G-17 one locale', () => {
  it('writes the account locale and mirrors it into the settings blob', async () => {
    let body: unknown;
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.patch('*/api/v1/users/@me', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({});
      }),
    );
    await chooseLanguage('en');
    expect(body).toEqual({ locale: 'en' });
    expect(usePrefsStore.getState().prefs.locale).toBe('en');
  });

  it('still switches and mirrors when the backend rejects the account write', async () => {
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.patch('*/api/v1/users/@me', () => HttpResponse.json({}, { status: 400 })),
    );
    await chooseLanguage('en');
    expect(usePrefsStore.getState().prefs.locale).toBe('en');
  });

  it('never sends a pseudo-locale to the account', async () => {
    let calls = 0;
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.patch('*/api/v1/users/@me', () => {
        calls++;
        return HttpResponse.json({});
      }),
    );
    await chooseLanguage('en-XA', true);
    expect(calls).toBe(0);
    expect(usePrefsStore.getState().prefs.locale).toBe(DEFAULT_PREFS.locale);
  });

  it('adopts the account locale only on a device with no saved language', async () => {
    expect(await adoptAccountLocale(undefined)).toBeNull();
    expect(await adoptAccountLocale('en-US')).toBe('en');
    // adopting saved it, so a second boot leaves the device choice alone
    expect(await adoptAccountLocale('de')).toBeNull();
  });
});
