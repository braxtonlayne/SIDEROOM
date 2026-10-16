import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { resetStore, useStore } from '../../../store';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, resetPrefs, signedIn } from './helpers';

const PRIVACY = {
  friend_requests_everyone: true,
  friend_requests_friends_of_friends: true,
  friend_requests_server_members: true,
  allow_server_member_dms: true,
  message_requests: true,
  sensitive_media: { friends: 'show', others: 'blur', servers: 'show' },
  dm_spam_filter: 'non_friends',
  server_dm_overrides: {},
};

beforeEach(() => {
  resetStore();
  resetPrefs();
  signedIn();
  server.use(
    http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
    http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
    http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([])),
  );
});

describe('Safety settings (LNG-USET-012/013/015)', () => {
  it('saves a sensitive-media choice per audience as a partial patch', async () => {
    const sent: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/privacy', async ({ request }) => {
        const body = (await request.json()) as { sensitive_media: Record<string, string> };
        sent.push(body);
        return HttpResponse.json({
          ...PRIVACY,
          sensitive_media: { ...PRIVACY.sensitive_media, ...body.sensitive_media },
        });
      }),
    );
    const user = userEvent.setup();
    openSettings('privacy');
    const others = await screen.findByRole('radiogroup', {
      name: 'Direct messages from everyone else',
    });
    expect(within(others).getByRole('radio', { name: 'Blur' })).toBeChecked();
    await user.click(within(others).getByRole('radio', { name: 'Block' }));
    await waitFor(() => expect(sent).toEqual([{ sensitive_media: { others: 'block' } }]));
    await waitFor(() => expect(within(others).getByRole('radio', { name: 'Block' })).toBeChecked());
  });

  it('changes the DM spam filter and a per-server override', async () => {
    useStore.setState({
      guilds: { '9': { id: '9', name: 'Garden' } },
      guildOrder: ['9'],
    });
    const sent: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/privacy', async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        sent.push(body);
        return HttpResponse.json({
          ...PRIVACY,
          ...body,
          server_dm_overrides: body.server_dm_overrides ? { '9': false } : {},
        });
      }),
    );
    const user = userEvent.setup();
    openSettings('privacy');
    const filter = await screen.findByRole('radiogroup', { name: 'Spam filter' });
    await user.click(within(filter).getByRole('radio', { name: 'Everyone' }));
    await waitFor(() => expect(sent[0]).toEqual({ dm_spam_filter: 'all' }));
    await user.click(screen.getByRole('combobox', { name: 'Garden' }));
    await user.click(await screen.findByRole('option', { name: 'Do not allow' }));
    await waitFor(() => expect(sent[1]).toEqual({ server_dm_overrides: { '9': false } }));
  });

  it('shows the age confirmation, re-confirms it and resets it with the password', async () => {
    let confirmedAt: string | null = '2026-03-04T10:00:00Z';
    let posted = 0;
    let resetBody: unknown = null;
    server.use(
      http.get('*/api/v1/users/@me/age-confirmation', () =>
        HttpResponse.json({
          confirmed_at: confirmedAt,
          method: confirmedAt ? 'self-declared' : null,
        }),
      ),
      http.post('*/api/v1/sr/age-gate', () => {
        posted += 1;
        return new HttpResponse(null, { status: 204 });
      }),
      http.delete('*/api/v1/users/@me/age-confirmation', async ({ request }) => {
        resetBody = await request.json();
        confirmedAt = null;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openSettings('privacy');
    expect(await screen.findByText('Confirmed 18 or older')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Confirm again' }));
    await waitFor(() => expect(posted).toBe(1));
    await user.click(screen.getByRole('button', { name: 'Reset my answer' }));
    await user.type(await screen.findByLabelText('Current password'), 'hunter2hunter2');
    await user.click(screen.getByRole('button', { name: 'Reset' }));
    await waitFor(() => expect(resetBody).toEqual({ current_password: 'hunter2hunter2' }));
    expect(await screen.findByText('Not confirmed')).toBeInTheDocument();
  });

  it('shows the wrong-password error from the reset', async () => {
    server.use(
      http.get('*/api/v1/users/@me/age-confirmation', () =>
        HttpResponse.json({ confirmed_at: '2026-03-04T10:00:00Z', method: 'self-declared' }),
      ),
      http.delete('*/api/v1/users/@me/age-confirmation', () =>
        HttpResponse.json({ message: 'Wrong password.' }, { status: 403 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('privacy');
    await user.click(await screen.findByRole('button', { name: 'Reset my answer' }));
    await user.type(await screen.findByLabelText('Current password'), 'nope');
    await user.click(screen.getByRole('button', { name: 'Reset' }));
    expect(await screen.findByText('Wrong password.')).toBeInTheDocument();
  });
});

describe('Notification keywords', () => {
  it('adds and removes keywords with PUT and shows the cleaned list', async () => {
    const puts: string[][] = [];
    server.use(
      http.get('*/api/v1/users/@me/notification-keywords', () =>
        HttpResponse.json({ keywords: ['alpha'] }),
      ),
      http.put('*/api/v1/users/@me/notification-keywords', async ({ request }) => {
        const { keywords } = (await request.json()) as { keywords: string[] };
        puts.push(keywords);
        return HttpResponse.json({ keywords });
      }),
    );
    const user = userEvent.setup();
    openSettings('notifications');
    expect((await screen.findAllByText('alpha')).length).toBeGreaterThan(0);
    await user.type(screen.getByLabelText('New keyword'), 'beta  gamma');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    await waitFor(() => expect(puts).toEqual([['alpha', 'beta gamma']]));
    expect((await screen.findAllByText('beta gamma')).length).toBeGreaterThan(0);
    await user.click(screen.getByRole('button', { name: /Remove\s+alpha/ }));
    await waitFor(() => expect(puts[1]).toEqual(['beta gamma']));
    expect(useStore.getState().notificationKeywords).toEqual(['beta gamma']);
  });

  it('refuses a duplicate before calling the server', async () => {
    server.use(
      http.get('*/api/v1/users/@me/notification-keywords', () =>
        HttpResponse.json({ keywords: ['Alpha'] }),
      ),
    );
    const user = userEvent.setup();
    openSettings('notifications');
    await screen.findAllByText('Alpha');
    await user.type(screen.getByLabelText('New keyword'), 'alpha');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByText('That keyword is already in the list.')).toBeInTheDocument();
  });
});
