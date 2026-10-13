import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { fieldError, openSettings, PROFILE, resetPrefs, signedIn } from './helpers';

function profileEndpoints(patch?: (body: unknown) => Response | undefined) {
  server.use(
    http.get('*/api/v1/users/@me/profile', () => HttpResponse.json(PROFILE)),
    http.get('*/api/v1/users/@me/presence', () =>
      HttpResponse.json({
        status: 'online',
        custom_status: { text: 'brb', emoji_name: null, emoji_id: null, expires_at: null },
      }),
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

  it('disables avatar, banner and display name edits with an explanation', async () => {
    signedIn();
    profileEndpoints();
    openSettings('profile');
    await screen.findByLabelText('About me');
    expect(screen.getByRole('button', { name: 'Change avatar' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Change banner' })).toBeDisabled();
    expect(screen.getByLabelText('Display name')).toBeDisabled();
    expect(screen.getByText(/uploads are coming soon/)).toBeInTheDocument();
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
