import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { useLoungeUi } from '../../../lounge/store';
import { matchCustomShortcut } from '../../../lounge/shortcuts';
import { useStore } from '../../../store';
import { http, HttpResponse, server } from '../../../test/server';
import { DEFAULT_PREFS, usePrefsStore } from '../prefs';
import { searchSettings } from '../searchIndex';
import { openSettings, PROFILE, resetPrefs, signedIn } from './helpers';

const themeRoot = () => document.querySelector<HTMLElement>('[data-theme]')!;
const saved = () => usePrefsStore.getState().prefs;

beforeEach(() => {
  resetPrefs();
  signedIn();
});

describe('Appearance extras', () => {
  it('gives a section its own theme and removes it again', async () => {
    const user = userEvent.setup();
    openSettings('appearance');
    await user.click(await screen.findByRole('combobox', { name: 'Library' }));
    await user.click(await screen.findByRole('option', { name: 'Lounge Onyx' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('unsaved changes');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved().sectionThemes).toEqual({ library: 'lounge-onyx' });
    await user.click(screen.getByRole('combobox', { name: 'Library' }));
    await user.click(await screen.findByRole('option', { name: 'Default for this section' }));
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved().sectionThemes).toEqual({});
  });

  it('previews a Lounge-only theme and the Side Room theme', async () => {
    const user = userEvent.setup();
    openSettings('appearance');
    await user.click(await screen.findByRole('button', { name: /Side Room/ }));
    expect(themeRoot()).toHaveAttribute('data-theme', 'sideroom');
    await user.click(screen.getByRole('combobox', { name: 'Lounge' }));
    await user.click(await screen.findByRole('option', { name: 'Lounge Light' }));
    expect(themeRoot()).toHaveAttribute('data-theme', 'lounge-light');
  });

  it('saves server theming and the space between message groups', async () => {
    const user = userEvent.setup();
    openSettings('appearance');
    await user.click(await screen.findByRole('switch', { name: /Allow server theming/ }));
    fireEvent.change(screen.getByRole('slider', { name: 'Space between message groups' }), {
      target: { value: '4' },
    });
    expect(themeRoot().style.getPropertyValue('--lng-message-gap')).toBe('4px');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved()).toMatchObject({ allowServerTheming: false, messageGap: 4 });
  });
});

describe('Account card and section bar', () => {
  it('shows the avatar and banner at the top of the account card', async () => {
    signedIn({ avatar: 'abc123', banner: 'def456' } as never);
    server.use(
      http.get('*/api/v1/users/@me/profile', () => HttpResponse.json(PROFILE)),
      http.get('*/api/v1/sr/auth/sessions', () => HttpResponse.json([])),
      http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json([])),
    );
    openSettings('account');
    const banner = await screen.findByTestId('account-banner');
    expect(banner.style.backgroundImage).toContain('/cdn/avatars/1/def456.png');
    expect(document.querySelector('img[src*="/cdn/avatars/1/abc123.png"]')).not.toBeNull();
  });

  it('toggles the Lounge section bar from Appearance', async () => {
    useLoungeUi.setState({ showSectionBar: true });
    const user = userEvent.setup();
    openSettings('appearance');
    await user.click(await screen.findByRole('switch', { name: /Show the section bar/ }));
    expect(useLoungeUi.getState().showSectionBar).toBe(false);
  });
});

describe('Accessibility', () => {
  it('keeps reduced motion here and applies contrast, link and font choices as you go', async () => {
    const user = userEvent.setup();
    openSettings('accessibility');
    await user.click(await screen.findByRole('radio', { name: 'On' }));
    expect(themeRoot()).toHaveAttribute('data-reduced-motion', 'true');
    await user.click(screen.getByRole('switch', { name: /High contrast/ }));
    expect(themeRoot()).toHaveAttribute('data-contrast', 'high');
    await user.click(screen.getByRole('switch', { name: /Always underline links/ }));
    expect(themeRoot()).toHaveAttribute('data-underline-links', 'true');
    await user.click(screen.getByRole('switch', { name: /Dyslexia-friendly text/ }));
    expect(themeRoot()).toHaveAttribute('data-dyslexia', 'true');
    await user.click(screen.getByRole('radio', { name: 'As a dot' }));
    await user.click(screen.getByRole('switch', { name: /Show the send-message button/ }));
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved()).toMatchObject({
      reducedMotion: 'on',
      highContrast: true,
      underlineLinks: true,
      dyslexiaFont: true,
      roleColors: 'dot',
      showSendButton: false,
    });
  });

  it('sets sticker and emoji animation separately', async () => {
    const user = userEvent.setup();
    openSettings('accessibility');
    const stickers = await screen.findByRole('radiogroup', { name: 'Play animated stickers' });
    await user.click(within(stickers).getByRole('radio', { name: 'Never' }));
    const emoji = screen.getByRole('radiogroup', { name: 'Play animated emoji' });
    await user.click(within(emoji).getByRole('radio', { name: 'On hover or focus' }));
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved()).toMatchObject({ stickerAnimation: 'never', emojiAnimation: 'hover' });
  });
});

describe('Text and Images', () => {
  it('saves the link preview, embed, attachment, reaction, emoticon and spoiler choices', async () => {
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 1, settings: {} }),
      ),
    );
    const user = userEvent.setup();
    openSettings('text');
    await user.click(await screen.findByRole('switch', { name: /Show website previews/ }));
    await user.click(screen.getByRole('switch', { name: /Show embeds from apps/ }));
    await user.click(screen.getByRole('switch', { name: /Show attachments directly/ }));
    await user.click(screen.getByRole('switch', { name: /Show emoji reactions/ }));
    await user.click(screen.getByRole('switch', { name: /Convert emoticons/ }));
    await user.click(screen.getByRole('radio', { name: 'Always show' }));
    expect(
      screen.getByRole('switch', { name: /Load remote images automatically/ }),
    ).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved()).toMatchObject({
      linkPreviews: false,
      showEmbeds: false,
      inlineAttachments: false,
      showReactions: false,
      convertEmoticons: false,
      spoilers: 'always',
    });
  });
});

describe('Activity Privacy', () => {
  it('writes the sharing choices to the account settings at once (G-58)', async () => {
    const patches: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        patches.push(body.settings);
        return HttpResponse.json({ version: patches.length, settings: body.settings });
      }),
    );
    const user = userEvent.setup();
    openSettings('activity');
    const watching = await screen.findByRole('switch', { name: /Share what I'm watching/ });
    expect(watching).toBeEnabled();
    expect(watching).toBeChecked();
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(watching);
    await user.click(screen.getByRole('switch', { name: /Share the live TV I'm watching/ }));
    await user.click(screen.getByRole('switch', { name: /Hide age-restricted titles/ }));
    await user.click(screen.getByRole('switch', { name: /Pause my watch and reading history/ }));
    await waitFor(() => expect(patches).toHaveLength(4));
    expect(patches).toEqual([
      { activity: { share_cinema: false } },
      { activity: { share_tv: false } },
      { activity: { hide_age_restricted: false } },
      { activity: { paused: true } },
    ]);
    expect(screen.queryByRole('button', { name: 'Save changes' })).not.toBeInTheDocument();
  });
});

describe('Authorized Apps and Connections', () => {
  it('says why there are no authorized apps', async () => {
    server.use(
      http.get('*/api/v1/oauth2/tokens', () =>
        HttpResponse.json({ message: 'Not Found' }, { status: 404 }),
      ),
    );
    openSettings('apps');
    expect(await screen.findByText(/cannot authorize third-party apps yet/)).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });
});

describe('Devices', () => {
  const session = (id: string, current = false) => ({
    id,
    device_name: `Device ${id}`,
    created_at: '2026-01-01T00:00:00Z',
    last_seen_at: '2026-01-02T00:00:00Z',
    expires_at: '2026-02-01T00:00:00Z',
    current,
  });

  it('lists this device first and logs out every other device after confirming', async () => {
    const revoked: string[] = [];
    server.use(
      http.get('*/api/v1/sr/auth/sessions', () =>
        HttpResponse.json([session('11'), session('10', true), session('12')]),
      ),
      http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json([])),
      http.delete('*/api/v1/sr/auth/sessions/:id', ({ params }) => {
        revoked.push(String(params.id));
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openSettings('devices');
    const first = (await screen.findAllByText(/^Device 1\d$/))[0]!;
    expect(first).toHaveTextContent('Device 10');
    await user.click(screen.getByRole('button', { name: 'Log out other devices' }));
    const dialog = await screen.findByRole('dialog', { name: 'Log out your other devices?' });
    await user.click(within(dialog).getByRole('button', { name: 'Log out other devices' }));
    await waitFor(() => expect(revoked.sort()).toEqual(['11', '12']));
    await waitFor(() => expect(screen.queryByText('Device 11')).toBeNull());
    expect(screen.getByText('Device 10')).toBeInTheDocument();
  });
});

describe('Sign-in alerts (ENT-AUTH-015)', () => {
  it('shows the switch from the server and saves a change', async () => {
    let sent: unknown;
    server.use(
      http.get('*/api/v1/sr/auth/sessions', () => HttpResponse.json([])),
      http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json([])),
      http.get('*/api/v1/sr/auth/login-alerts', () => HttpResponse.json({ enabled: true })),
      http.put('*/api/v1/sr/auth/login-alerts', async ({ request }) => {
        sent = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openSettings('devices');
    const sw = await screen.findByRole('switch', {
      name: 'Email me about sign-ins from new devices',
    });
    expect(sw).toBeChecked();
    await user.click(sw);
    await waitFor(() => expect(sent).toEqual({ enabled: false }));
    await waitFor(() => expect(sw).not.toBeChecked());
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('says so when the change is refused and leaves the switch where it was', async () => {
    server.use(
      http.get('*/api/v1/sr/auth/sessions', () => HttpResponse.json([])),
      http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json([])),
      http.put('*/api/v1/sr/auth/login-alerts', () =>
        HttpResponse.json({ message: 'Nope.' }, { status: 500 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('devices');
    const sw = await screen.findByRole('switch', {
      name: 'Email me about sign-ins from new devices',
    });
    await user.click(sw);
    expect(await screen.findByText('Could not change the setting')).toBeInTheDocument();
    expect(sw).toBeChecked();
  });
});

describe('Language', () => {
  it('stores the region and the 12 or 24-hour clock and shows an example', async () => {
    const user = userEvent.setup();
    openSettings('language');
    await user.click(await screen.findByRole('radio', { name: '24-hour' }));
    expect(screen.getByText(/Example:/)).toHaveTextContent(/15:05/);
    await user.click(screen.getByRole('combobox', { name: 'Region and date format' }));
    await user.click(await screen.findByRole('option', { name: 'Deutsch' }));
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved()).toMatchObject({ locale: 'de', timeFormat: '24h' });
  });
});

describe('Advanced', () => {
  it('resets the layout state locally and in the account after confirming', async () => {
    useLoungeUi.setState({
      collapsedCategories: { c: true },
      rightPanelWidth: 400,
      railLayout: [{ kind: 'guild', guildId: 'g1' }],
    });
    let sent: unknown;
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        sent = ((await request.json()) as { settings: unknown }).settings;
        return HttpResponse.json({ version: 9, settings: {} });
      }),
    );
    const user = userEvent.setup();
    openSettings('advanced');
    await user.click(await screen.findByRole('button', { name: 'Reset UI state' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reset UI state?' });
    await user.click(within(dialog).getByRole('button', { name: 'Reset' }));
    await waitFor(() => expect(useLoungeUi.getState().rightPanelWidth).toBe(240));
    expect(useLoungeUi.getState().collapsedCategories).toEqual({});
    expect(useLoungeUi.getState().railLayout).toBeNull();
    expect(sent).toEqual({
      guild_positions: null,
      guild_folders: null,
      collapsed_categories: null,
      last_channel_by_guild: null,
      right_panel_width: null,
    });
  });

  it('keeps the layout when the account cannot be reached', async () => {
    useLoungeUi.setState({ rightPanelWidth: 400 });
    server.use(
      http.patch('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ message: 'Down.' }, { status: 500 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('advanced');
    await user.click(await screen.findByRole('button', { name: 'Reset UI state' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reset UI state?' });
    await user.click(within(dialog).getByRole('button', { name: 'Reset' }));
    expect(await within(dialog).findByText('Down.')).toBeInTheDocument();
    expect(useLoungeUi.getState().rightPanelWidth).toBe(400);
  });
});

describe('Keybinds', () => {
  const recorder = (name: RegExp) => screen.findByRole('button', { name });

  it('records a new combo, shows it, and the Lounge then answers to it', async () => {
    const user = userEvent.setup();
    openSettings('keybinds');
    await user.click(await screen.findByRole('button', { name: /Change\s+Previous server/ }));
    const capture = await recorder(/Press the new keys for Previous server/);
    expect(capture).toHaveFocus();
    await user.keyboard('{Control>}{Shift>}J{/Shift}{/Control}');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved().keybinds).toEqual({ prevGuild: 'Mod+Shift+J' });
    const key = (e: Partial<KeyboardEventInit>) =>
      matchCustomShortcut(
        { key: 'j', ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...e },
        saved().keybinds,
      );
    expect(key({ key: 'J', ctrlKey: true, shiftKey: true })).toBe('prevGuild');
    // The old default no longer does it.
    expect(key({ key: 'ArrowUp', ctrlKey: true, altKey: true })).toBeNull();
    // Fixed keys stay put.
    expect(key({ key: 'Escape' })).toBe('markRead');
  });

  it('refuses a combo that is taken, one without a modifier, and cancels with Escape', async () => {
    const user = userEvent.setup();
    openSettings('keybinds');
    await user.click(await screen.findByRole('button', { name: /Change\s+Previous server/ }));
    await recorder(/Press the new keys/);
    await user.keyboard('{Control>}k{/Control}');
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Already used for "Find or start a conversation"',
    );
    await user.keyboard('x');
    expect(await screen.findByRole('alert')).toHaveTextContent('Hold Ctrl');
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('button', { name: /Press the new keys/ })).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(saved().keybinds).toEqual({});
  });

  it('unbinds, resets one, and resets all', async () => {
    const user = userEvent.setup();
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, keybinds: { quickSwitcher: 'Mod+J' } } });
    openSettings('keybinds');
    const unbind = await screen.findByRole('button', { name: /Unbind\s+Find or start/ });
    // The voice keybinds on this page also say "Not bound" when empty: count against that baseline.
    const before = screen.queryAllByText('Not bound').length;
    await user.click(unbind);
    await waitFor(() => expect(screen.queryAllByText('Not bound')).toHaveLength(before + 1));
    await user.click(screen.getByRole('button', { name: /Reset\s+Find or start/ }));
    await waitFor(() => expect(screen.queryAllByText('Not bound')).toHaveLength(before));
    await user.click(screen.getByRole('button', { name: /Unbind\s+Find or start/ }));
    await user.click(screen.getByRole('button', { name: 'Reset all keybinds to the defaults' }));
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(saved().keybinds).toEqual({});
  });
});

describe('Settings search', () => {
  it('finds settings by name or by the words people use, and opens the section', async () => {
    expect(searchSettings('motion').map((e) => e.section)).toContain('accessibility');
    expect(searchSettings('time format 24')[0]?.section).toBe('language');
    expect(searchSettings('   ')).toEqual([]);
    const user = userEvent.setup();
    const router = openSettings('account');
    const box = await screen.findByRole('searchbox', { name: 'Search settings' });
    await user.type(box, 'reduced mot');
    const results = await screen.findByRole('list', { name: 'Search results' });
    await user.click(within(results).getByRole('button', { name: /Reduced motion/ }));
    await waitFor(() =>
      expect(router.state.location.pathname).toBe('/settings/user/accessibility'),
    );
    expect(box).toHaveValue('');
  });

  it('opens the first hit on Enter, says when nothing matches, and Escape clears before closing', async () => {
    const user = userEvent.setup();
    const router = openSettings('account');
    const box = await screen.findByRole('searchbox', { name: 'Search settings' });
    await user.type(box, 'zzzz');
    expect(await screen.findByText(/No settings match/)).toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(box).toHaveValue('');
    expect(router.state.location.pathname).toBe('/settings/user/account');
    await user.type(box, 'underline{Enter}');
    await waitFor(() =>
      expect(router.state.location.pathname).toBe('/settings/user/accessibility'),
    );
  });
});

describe('What is new and the footer', () => {
  it("shows the version and a What's New dialog", async () => {
    const user = userEvent.setup();
    openSettings('advanced');
    expect(await screen.findByText(/^SIDE ROOM \d/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: "What's New" }));
    const dialog = await screen.findByRole('dialog', { name: "What's New" });
    expect(within(dialog).getByText('Settings, filled in')).toBeInTheDocument();
  });
});

describe('Preferences follow the account', () => {
  it('writes saved preferences into the settings blob under user_prefs', async () => {
    const patches: Record<string, unknown>[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        patches.push(body.settings);
        return HttpResponse.json({ version: patches.length, settings: body.settings });
      }),
    );
    const user = userEvent.setup();
    openSettings('accessibility');
    await user.click(await screen.findByRole('switch', { name: /High contrast/ }));
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(patches).toHaveLength(1));
    const sent = patches[0]!.user_prefs as Record<string, unknown>;
    expect(sent).toMatchObject({ highContrast: true, theme: 'lounge-dark', sectionThemes: {} });
  });

  it('adopts the account copy when it arrives, including a theme chosen on another device', async () => {
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({
          version: 4,
          settings: { user_prefs: { theme: 'lounge-ash', highContrast: true, zoom: 9999 } },
        }),
      ),
    );
    openSettings('appearance');
    await waitFor(() => expect(themeRoot()).toHaveAttribute('data-theme', 'lounge-ash'));
    expect(saved()).toMatchObject({ theme: 'lounge-ash', highContrast: true, zoom: 200 });
    // Adopting does not write it straight back.
    expect(useStore.getState().userSettings?.version).toBe(4);
  });

  it('removes a dropped per-section theme and keybind with explicit nulls', async () => {
    const patches: Record<string, unknown>[] = [];
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({
          version: 2,
          settings: {
            user_prefs: {
              ...DEFAULT_PREFS,
              sectionThemes: { library: 'lounge-onyx' },
              keybinds: { quickSwitcher: 'Mod+J' },
            },
          },
        }),
      ),
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        patches.push(body.settings);
        return HttpResponse.json({ version: 3, settings: body.settings });
      }),
    );
    const user = userEvent.setup();
    openSettings('keybinds');
    await user.click(await screen.findByRole('button', { name: /Reset\s+Find or start/ }));
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(patches).toHaveLength(1));
    const sent = patches[0]!.user_prefs as Record<string, Record<string, unknown>>;
    expect(sent.keybinds).toEqual({ quickSwitcher: null });
    expect(sent.sectionThemes).toEqual({ library: 'lounge-onyx' });
  });

  it("pushes this browser's choices once when the account has none yet", async () => {
    const patches: Record<string, unknown>[] = [];
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, theme: 'lounge-light' } });
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        patches.push(body.settings);
        return HttpResponse.json({ version: 1, settings: body.settings });
      }),
    );
    openSettings('appearance');
    await waitFor(() => expect(patches).toHaveLength(1));
    expect((patches[0]!.user_prefs as { theme: string }).theme).toBe('lounge-light');
    expect(themeRoot()).toHaveAttribute('data-theme', 'lounge-light');
  });
});
