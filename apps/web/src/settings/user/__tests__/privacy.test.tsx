import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, resetPrefs, signedIn } from './helpers';

const rel = (id: string, type: number, username: string) => ({
  id,
  type,
  nickname: null,
  since: '2026-01-01T00:00:00Z',
  user: { id, username, global_name: null, avatar: null },
});

beforeEach(() => {
  resetPrefs();
  signedIn();
});

describe('Privacy and Safety', () => {
  it('lists only blocked users and unblocks one', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json([rel('2', 2, 'bob'), rel('3', 1, 'friendly')]),
      ),
      http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
      http.delete(
        '*/api/v1/users/@me/relationships/2',
        () => new HttpResponse(null, { status: 204 }),
      ),
    );
    openSettings('privacy');
    expect((await screen.findAllByText('bob')).length).toBeGreaterThan(0);
    expect(screen.queryByText('friendly')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: /Unblock\s+bob/ }));
    expect(await screen.findByText("You haven't blocked anyone.")).toBeInTheDocument();
  });

  it('unignores a user and shows server errors', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/ignores', () =>
        HttpResponse.json([
          { id: '4', username: 'carl' },
          { id: '6', username: 'dana' },
        ]),
      ),
      http.delete('*/api/v1/users/@me/ignores/4', () => new HttpResponse(null, { status: 204 })),
      http.delete('*/api/v1/users/@me/ignores/6', () =>
        HttpResponse.json({ message: 'Try later.' }, { status: 500 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('privacy');
    await user.click(await screen.findByRole('button', { name: /Unignore\s+carl/ }));
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: /Unignore\s+carl/ })).toBeNull(),
    );
    await user.click(screen.getByRole('button', { name: /Unignore\s+dana/ }));
    expect(await screen.findByText('Try later.')).toBeInTheDocument();
  });

  it('shows empty states and says DM/friend privacy has no endpoint', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
    );
    openSettings('privacy');
    expect(await screen.findByText("You haven't ignored anyone.")).toBeInTheDocument();
    expect(screen.getByText(/no endpoint for DM and friend-request privacy/)).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: /direct messages/ })).toBeDisabled();
  });

  it('reports load failures', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json({ message: 'x' }, { status: 500 }),
      ),
      http.get('*/api/v1/users/@me/ignores', () =>
        HttpResponse.json({ message: 'y' }, { status: 500 }),
      ),
    );
    openSettings('privacy');
    expect(await screen.findByText("Couldn't load blocked users")).toBeInTheDocument();
    expect(await screen.findByText("Couldn't load ignored users")).toBeInTheDocument();
  });
});
