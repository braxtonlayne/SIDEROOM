import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import { fieldError, openSettings, PROFILE, resetPrefs, signedIn } from './helpers';

const CROPPED = 'data:image/png;base64,AAAA';
// jsdom cannot decode or draw images: stand in for the browser's decoder and canvas.
vi.mock('../imageCrop', async (orig) => ({
  ...(await orig<typeof import('../imageCrop')>()),
  loadImage: async () => ({
    width: 1000,
    height: 600,
    url: 'blob:test',
    release: () => undefined,
    render: () => CROPPED,
  }),
}));

function profileEndpoints(patch?: (body: unknown) => Response | undefined) {
  server.use(
    http.get('*/api/v1/users/@me/profile', () => HttpResponse.json(PROFILE)),
    http.get('*/api/v1/users/@me/presence', () =>
      HttpResponse.json({
        status: 'online',
        custom_status: { text: 'brb', emoji_name: null, emoji_id: null, expires_at: null },
      }),
    ),
    // PRF-PRO-001: the "Who can see my profile" card loads next to the profile form.
    http.get('*/api/v1/users/@me/profile-privacy', () =>
      HttpResponse.json({
        preset: 'private',
        stored: {
          page_tier: 'only_me',
          links_tier: 'only_me',
          top8_tier: 'only_me',
          now_playing_tier: 'only_me',
          bio_tier: 'only_me',
          guestbook_tier: 'only_me',
          media_tier: 'only_me',
          ring_tier: 'only_me',
          guestbook_accepts_public: false,
        },
        effective: {},
        public_possible: false,
      }),
    ),
    http.get(
      '*/api/v1/users/@me/profile-page/preview',
      () => new HttpResponse(null, { status: 404 }),
    ),
    http.patch('*/api/v1/users/@me', async ({ request }) => {
      const body = await request.json();
      return patch?.(body) ?? HttpResponse.json({});
    }),
  );
}

beforeEach(resetPrefs);

describe('Profile', () => {
  it('loads the profile, shows a live preview and a character counter', async () => {
    signedIn();
    profileEndpoints();
    openSettings('profile');
    const bio = await screen.findByLabelText('About me');
    expect(bio).toHaveValue('Hello there');
    expect(screen.getByText('11/190')).toBeInTheDocument();
    const preview = screen.getByRole('complementary', { name: 'Profile preview' });
    expect(within(preview).getByText('Hello there')).toBeInTheDocument();
    expect(within(preview).getByText('brb')).toBeInTheDocument();
    fireEvent.change(bio, { target: { value: 'New bio' } });
    expect(within(preview).getByText('New bio')).toBeInTheDocument();
    expect(screen.getByText('7/190')).toBeInTheDocument();
  });

  it('edits the display name and saves it as global_name', async () => {
    signedIn();
    let sent: unknown;
    profileEndpoints((b) => {
      sent = b;
      return undefined;
    });
    const user = userEvent.setup();
    openSettings('profile');
    const name = await screen.findByLabelText('Display name');
    expect(name).toHaveValue('Alice');
    await user.clear(name);
    await user.type(name, 'Ali C');
    const preview = screen.getByRole('complementary', { name: 'Profile preview' });
    expect(within(preview).getByText('Ali C')).toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(sent).toEqual({ global_name: 'Ali C' }));
  });

  it('rejects a display name over 32 characters', async () => {
    signedIn();
    profileEndpoints();
    openSettings('profile');
    fireEvent.change(await screen.findByLabelText('Display name'), {
      target: { value: 'x'.repeat(33) },
    });
    expect((await screen.findAllByText(/at most 32 characters/)).length).toBeGreaterThan(0);
  });

  it('crops a new avatar, previews it and sends it as a data URI on save', async () => {
    signedIn();
    let sent: unknown;
    profileEndpoints((b) => {
      sent = b;
      return undefined;
    });
    const user = userEvent.setup();
    openSettings('profile');
    await screen.findByLabelText('About me');
    expect(await axeViolations(document.body)).toEqual([]);
    await user.upload(
      screen.getByLabelText('Avatar file'),
      new File(['x'], 'me.png', { type: 'image/png' }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Crop your avatar' });
    await within(dialog).findByRole('group', { name: 'Crop area' });
    expect(await axeViolations(dialog)).toEqual([]);
    fireEvent.change(within(dialog).getByRole('slider', { name: 'Zoom' }), {
      target: { value: '2' },
    });
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('unsaved changes');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(sent).toEqual({ avatar: CROPPED }));
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
  });

  it('moves the crop with the arrow keys and cancelling changes nothing', async () => {
    signedIn();
    profileEndpoints();
    const user = userEvent.setup();
    openSettings('profile');
    await screen.findByLabelText('About me');
    await user.upload(
      screen.getByLabelText('Banner file'),
      new File(['x'], 'wide.jpg', { type: 'image/jpeg' }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Crop your banner' });
    const area = await within(dialog).findByRole('group', { name: 'Crop area' });
    const img = area.querySelector('img')!;
    const before = img.style.top;
    area.focus();
    await user.keyboard('{ArrowDown}{ArrowDown}');
    expect(img.style.top).not.toBe(before);
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('sends a GIF as it is (no flattening) and refuses files that are not images', async () => {
    signedIn();
    let sent: unknown;
    profileEndpoints((b) => {
      sent = b;
      return undefined;
    });
    const user = userEvent.setup({ applyAccept: false });
    openSettings('profile');
    await screen.findByLabelText('About me');
    await user.upload(
      screen.getByLabelText('Avatar file'),
      new File(['notes'], 'notes.txt', { type: 'text/plain' }),
    );
    expect(await screen.findByText('Choose a PNG, JPEG, GIF or WebP image.')).toBeInTheDocument();
    await user.upload(
      screen.getByLabelText('Avatar file'),
      new File(['GIF89a'], 'spin.gif', { type: 'image/gif' }),
    );
    await user.click(await screen.findByRole('button', { name: 'Save changes' }));
    await waitFor(() =>
      expect((sent as { avatar: string }).avatar).toMatch(/^data:image\/gif;base64,/),
    );
  });

  it('removes the avatar and banner when they exist', async () => {
    signedIn();
    server.use(
      http.get('*/api/v1/users/@me/profile', () =>
        HttpResponse.json({
          ...PROFILE,
          user: { ...PROFILE.user, avatar: 'abc123', banner: 'def456' },
        }),
      ),
    );
    let sent: unknown;
    profileEndpoints((b) => {
      sent = b;
      return undefined;
    });
    server.use(
      http.get('*/api/v1/users/@me/profile', () =>
        HttpResponse.json({
          ...PROFILE,
          user: { ...PROFILE.user, avatar: 'abc123', banner: 'def456' },
        }),
      ),
    );
    const user = userEvent.setup();
    openSettings('profile');
    await user.click(await screen.findByRole('button', { name: 'Remove avatar' }));
    await user.click(screen.getByRole('button', { name: 'Remove banner' }));
    await user.click(await screen.findByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(sent).toEqual({ avatar: null, banner: null }));
  });

  it('sets a nickname for one server on its own button', async () => {
    signedIn();
    profileEndpoints();
    const sent: unknown[] = [];
    server.use(
      http.get('*/api/v1/users/@me/guilds', () =>
        HttpResponse.json([
          { id: '5', name: 'Book Club' },
          { id: '6', name: 'Chess' },
        ]),
      ),
      http.get('*/api/v1/guilds/5/members/1', () => HttpResponse.json({ nick: 'Ally' })),
      http.patch('*/api/v1/guilds/5/members/1', async ({ request }) => {
        const body = (await request.json()) as { nick: string | null };
        sent.push(body);
        return HttpResponse.json({ nick: body.nick });
      }),
    );
    const user = userEvent.setup();
    openSettings('profile');
    await user.click(await screen.findByRole('radio', { name: 'Per-server profiles' }));
    const nick = await screen.findByLabelText('Server nickname');
    await waitFor(() => expect(nick).toHaveValue('Ally'));
    expect(screen.getByRole('button', { name: 'Save nickname' })).toBeDisabled();
    await user.clear(nick);
    await user.click(screen.getByRole('button', { name: 'Save nickname' }));
    await waitFor(() => expect(sent).toEqual([{ nick: null }]));
    expect(screen.getByText(/Only the nickname is stored per server/)).toBeInTheDocument();
  });

  it('shows why a nickname was refused', async () => {
    signedIn();
    profileEndpoints();
    server.use(
      http.get('*/api/v1/users/@me/guilds', () =>
        HttpResponse.json([{ id: '5', name: 'Book Club' }]),
      ),
      http.get('*/api/v1/guilds/5/members/1', () => HttpResponse.json({ nick: null })),
      http.patch('*/api/v1/guilds/5/members/1', () =>
        HttpResponse.json({ message: 'Missing Permissions' }, { status: 403 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('profile');
    await user.click(await screen.findByRole('radio', { name: 'Per-server profiles' }));
    await user.type(await screen.findByLabelText('Server nickname'), 'Al');
    await user.click(screen.getByRole('button', { name: 'Save nickname' }));
    expect(await screen.findByText('Missing Permissions')).toBeInTheDocument();
  });

  it('saves only the changed fields through the unsaved-changes bar', async () => {
    signedIn();
    let sent: unknown;
    profileEndpoints((b) => {
      sent = b;
      return undefined;
    });
    const user = userEvent.setup();
    openSettings('profile');
    const bio = await screen.findByLabelText('About me');
    expect(screen.queryByRole('alert')).toBeNull();
    await user.type(bio, '!');
    expect(await screen.findByRole('alert')).toHaveTextContent('unsaved changes');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(sent).toEqual({ bio: 'Hello there!' }));
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
  });

  it('sends null to clear a field', async () => {
    signedIn();
    let sent: unknown;
    profileEndpoints((b) => {
      sent = b;
      return undefined;
    });
    const user = userEvent.setup();
    openSettings('profile');
    await user.clear(await screen.findByLabelText('Pronouns'));
    await user.clear(screen.getByLabelText('Custom status'));
    await user.click(screen.getByRole('button', { name: 'Clear accent color' }));
    await user.click(await screen.findByRole('button', { name: 'Save changes' }));
    await waitFor(() =>
      expect(sent).toEqual({ pronouns: null, accent_color: null, custom_status: null }),
    );
  });

  it('rejects an over-long bio before sending anything', async () => {
    signedIn();
    let calls = 0;
    profileEndpoints(() => {
      calls += 1;
      return undefined;
    });
    const user = userEvent.setup();
    openSettings('profile');
    const bio = await screen.findByLabelText('About me');
    fireEvent.change(bio, { target: { value: 'x'.repeat(191) } });
    expect(await screen.findAllByText('About me can be at most 190 characters.')).not.toHaveLength(
      0,
    );
    await user.click(await screen.findByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(screen.getAllByText(/at most 190/).length).toBeGreaterThan(1));
    expect(calls).toBe(0);
  });

  it('shows a server error and keeps the edits', async () => {
    signedIn();
    profileEndpoints(
      () =>
        HttpResponse.json(fieldError('bio', 'Bio contains blocked words.'), {
          status: 400,
        }) as unknown as Response,
    );
    const user = userEvent.setup();
    openSettings('profile');
    await user.type(await screen.findByLabelText('About me'), '!');
    await user.click(await screen.findByRole('button', { name: 'Save changes' }));
    expect(await screen.findByText('Bio contains blocked words.')).toBeInTheDocument();
    expect(screen.getByLabelText('About me')).toHaveValue('Hello there!');
    expect(screen.getAllByRole('alert').length).toBeGreaterThan(0);
  });

  it('Reset discards edits', async () => {
    signedIn();
    profileEndpoints();
    const user = userEvent.setup();
    openSettings('profile');
    await user.type(await screen.findByLabelText('About me'), '!!');
    await user.click(await screen.findByRole('button', { name: 'Reset' }));
    expect(screen.getByLabelText('About me')).toHaveValue('Hello there');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('shows an error when the profile cannot be loaded', async () => {
    signedIn();
    server.use(
      http.get('*/api/v1/users/@me/profile', () =>
        HttpResponse.json({ message: 'Down.' }, { status: 500 }),
      ),
      http.get('*/api/v1/users/@me/presence', () =>
        HttpResponse.json({ status: 'online', custom_status: null }),
      ),
    );
    openSettings('profile');
    expect(await screen.findByText("Couldn't load your profile")).toBeInTheDocument();
  });
});
