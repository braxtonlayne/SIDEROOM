import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { CREATION_OPTIONS, stubWebAuthn, unstubWebAuthn } from '../../../test/webauthnFake';
import { openSettings, PROFILE, resetPrefs, signedIn } from './helpers';

const key = (id: string, name: string, last: string | null = null) => ({
  id,
  name,
  created_at: '2026-01-01T00:00:00Z',
  last_used_at: last,
  backup_eligible: false,
});

function accountEndpoints(passkeys: unknown[]) {
  server.use(
    http.get('*/api/v1/users/@me/profile', () => HttpResponse.json(PROFILE)),
    http.get('*/api/v1/sr/auth/sessions', () => HttpResponse.json([])),
    http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json([])),
    http.get('*/api/v1/sr/auth/passkeys', () => HttpResponse.json(passkeys)),
  );
}

beforeEach(resetPrefs);
afterEach(unstubWebAuthn);

describe('Passkeys card', () => {
  it('lists passkeys, and says when there are none', async () => {
    signedIn();
    accountEndpoints([key('1', 'Work laptop', '2026-02-01T00:00:00Z'), key('2', 'Phone')]);
    openSettings('account');
    const list = await screen.findByRole('list', { name: 'Passkeys' }, { timeout: 5000 });
    expect(within(list).getByText('Work laptop', { selector: 'strong' })).toBeInTheDocument();
    expect(within(list).getByText(/never used/)).toBeInTheDocument();
  });

  it('shows the empty state and disables adding where WebAuthn is missing', async () => {
    signedIn();
    accountEndpoints([]);
    openSettings('account');
    expect(await screen.findByText('No passkeys yet.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add a passkey' })).toBeDisabled();
    expect(screen.getByText('Not supported here')).toBeInTheDocument();
  });

  it('adds a passkey: password, browser prompt, finish, then shows recovery codes', async () => {
    signedIn();
    accountEndpoints([]);
    const { create } = stubWebAuthn();
    let begin: unknown;
    let finish: unknown;
    server.use(
      http.post('*/api/v1/sr/auth/passkeys/register/begin', async ({ request }) => {
        begin = await request.json();
        return HttpResponse.json({ ceremony_id: 'r1', options: CREATION_OPTIONS });
      }),
      http.post('*/api/v1/sr/auth/passkeys/register/finish', async ({ request }) => {
        finish = await request.json();
        return HttpResponse.json({
          passkey: key('7', 'Desk key'),
          recovery_codes: ['AAAA-0001', 'AAAA-0002'],
        });
      }),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: 'Add a passkey' }));
    const dialog = await screen.findByRole('dialog', { name: 'Add a passkey' });
    await user.clear(within(dialog).getByLabelText('Name'));
    await user.type(within(dialog).getByLabelText('Name'), 'Desk key');
    await user.type(within(dialog).getByLabelText('Current password'), 'pw');
    await user.click(within(dialog).getByRole('button', { name: 'Add passkey' }));
    expect(await screen.findByText('AAAA-0001')).toBeInTheDocument();
    expect(create).toHaveBeenCalledOnce();
    expect(begin).toEqual({ password: 'pw' });
    expect(finish).toMatchObject({
      ceremony_id: 'r1',
      name: 'Desk key',
      credential: { rawId: 'AQID', response: { attestationObject: 'BgcI' } },
    });
    await user.click(screen.getByRole('button', { name: 'I have saved them' }));
    expect(await screen.findByText('Desk key', { selector: 'strong' })).toBeInTheDocument();
  });

  it('shows the server message for a wrong password and never opens the browser prompt', async () => {
    signedIn();
    accountEndpoints([]);
    const { create } = stubWebAuthn();
    server.use(
      http.post('*/api/v1/sr/auth/passkeys/register/begin', () =>
        HttpResponse.json({ message: "That password doesn't match." }, { status: 403 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: 'Add a passkey' }));
    const dialog = await screen.findByRole('dialog', { name: 'Add a passkey' });
    await user.type(within(dialog).getByLabelText('Current password'), 'nope');
    await user.click(within(dialog).getByRole('button', { name: 'Add passkey' }));
    expect(await within(dialog).findByText("That password doesn't match.")).toBeInTheDocument();
    expect(create).not.toHaveBeenCalled();
  });

  it('renames a passkey', async () => {
    signedIn();
    accountEndpoints([key('1', 'Old name')]);
    let body: unknown;
    server.use(
      http.patch('*/api/v1/sr/auth/passkeys/1', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(key('1', 'New name'));
      }),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: /Rename/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Rename passkey' });
    await user.clear(within(dialog).getByLabelText('Name'));
    await user.type(within(dialog).getByLabelText('Name'), 'New name');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(body).toEqual({ name: 'New name' }));
    expect(await screen.findByText('New name', { selector: 'strong' })).toBeInTheDocument();
    expect(screen.queryByText('Old name', { selector: 'strong' })).toBeNull();
  });

  it('removes a passkey with the password (DELETE with a body)', async () => {
    signedIn();
    accountEndpoints([key('1', 'Work laptop')]);
    let body: unknown;
    server.use(
      http.delete('*/api/v1/sr/auth/passkeys/1', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: /Remove/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Remove passkey' });
    await user.type(within(dialog).getByLabelText('Current password'), 'pw');
    await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(body).toEqual({ password: 'pw' }));
    expect(await screen.findByText('No passkeys yet.')).toBeInTheDocument();
  });

  it('adding a second passkey proves the first factor in the re-auth dialog first (D-573)', async () => {
    signedIn();
    accountEndpoints([key('1', 'Work laptop')]);
    const { create } = stubWebAuthn();
    const begins: unknown[] = [];
    let proof: unknown = null;
    server.use(
      http.post('*/api/v1/sr/auth/passkeys/register/begin', async ({ request }) => {
        begins.push(await request.json());
        return proof
          ? HttpResponse.json({ ceremony_id: 'r2', options: CREATION_OPTIONS })
          : HttpResponse.json(
              {
                code: 50035,
                message: 'Invalid Form Body',
                errors: { mfa: { _errors: [{ code: 'required', message: 'Second factor.' }] } },
              },
              { status: 400 },
            );
      }),
      http.post('*/api/v1/sr/auth/passkeys/register/finish', () =>
        HttpResponse.json({ passkey: key('2', 'Desk key'), recovery_codes: null }),
      ),
      http.get('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ code: 50013, message: 'Missing Access' }, { status: 403 }),
      ),
      http.post('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ code: 50013, message: 'Missing Access' }, { status: 403 }),
      ),
      http.get('*/api/v1/sr/auth/reauth', () =>
        HttpResponse.json({ fresh: false, enrolled: true, window_secs: 900 }),
      ),
      http.post('*/api/v1/sr/auth/reauth', async ({ request }) => {
        proof = await request.json();
        return HttpResponse.json({ fresh: true, enrolled: true, window_secs: 900 });
      }),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: 'Add a passkey' }));
    const dialog = await screen.findByRole('dialog', { name: 'Add a passkey' });
    await user.type(within(dialog).getByLabelText('Current password'), 'pw');
    await user.click(within(dialog).getByRole('button', { name: 'Add passkey' }));
    const reauth = await screen.findByRole('dialog', { name: 'Confirm it is you' });
    expect(create).not.toHaveBeenCalled();
    await user.type(within(reauth).getByLabelText('Code from your authenticator app'), '654321');
    await user.click(within(reauth).getByRole('button', { name: 'Confirm' }));
    expect(await screen.findByText('Desk key', { selector: 'strong' })).toBeInTheDocument();
    expect(proof).toEqual({ method: 'totp', code: '654321' });
    expect(begins).toEqual([{ password: 'pw' }, { password: 'pw' }]);
    expect(create).toHaveBeenCalledOnce();
  });
});
