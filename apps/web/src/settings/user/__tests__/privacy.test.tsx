import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { resetStore, useStore } from '../../../store';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
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
  resetStore();
  resetPrefs();
  signedIn();
  server.use(
    http.get('*/api/v1/users/@me/data-export', () => HttpResponse.json({ state: 'none' })),
  );
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

  it('shows the empty states, the data request and where the media filters stand', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/data-export', () => HttpResponse.json({ state: 'none' })),
    );
    openSettings('privacy');
    expect(await screen.findByText("You haven't ignored anyone.")).toBeInTheDocument();
    expect(await screen.findByText('You have not requested your data yet.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Request my data' })).toBeEnabled();
    expect(screen.getByText(/no media-scanning/)).toBeInTheDocument();
  });

  it('loads who can reach you and saves each change straight away', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/privacy', () =>
        HttpResponse.json({
          friend_requests_everyone: false,
          friend_requests_friends_of_friends: true,
          friend_requests_server_members: true,
          allow_server_member_dms: true,
          message_requests: true,
        }),
      ),
    );
    const sent: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/privacy', async ({ request }) => {
        const body = (await request.json()) as Record<string, boolean>;
        sent.push(body);
        return HttpResponse.json({
          friend_requests_everyone: true,
          friend_requests_friends_of_friends: true,
          friend_requests_server_members: true,
          allow_server_member_dms: true,
          message_requests: true,
          ...body,
        });
      }),
    );
    const user = userEvent.setup();
    openSettings('privacy');
    const everyone = await screen.findByRole('switch', { name: /Everyone/ });
    expect(everyone).not.toBeChecked();
    expect(screen.getByRole('switch', { name: /Friends of friends/ })).toBeChecked();
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(everyone);
    await waitFor(() => expect(sent).toEqual([{ friend_requests_everyone: true }]));
    await waitFor(() => expect(everyone).toBeChecked());
    await user.click(screen.getByRole('switch', { name: /direct messages from server members/ }));
    await waitFor(() => expect(sent[1]).toEqual({ allow_server_member_dms: false }));
    await user.click(screen.getByRole('switch', { name: /^Message requests/ }));
    await waitFor(() => expect(sent[2]).toEqual({ message_requests: false }));
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('puts the switch back and shows the reason when a privacy change is refused', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
      http.patch('*/api/v1/users/@me/privacy', () =>
        HttpResponse.json({ message: 'Try again later.' }, { status: 500 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('privacy');
    const sw = await screen.findByRole('switch', { name: /^Message requests/ });
    expect(sw).toBeChecked();
    await user.click(sw);
    expect(await screen.findByText('Try again later.')).toBeInTheDocument();
    expect(sw).toBeChecked();
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

describe('Load remote images automatically', () => {
  const quietLists = () =>
    server.use(
      http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
    );

  it('is off by default, saves into the settings blob and shows the new state', async () => {
    quietLists();
    const patches: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        patches.push(body.settings);
        return HttpResponse.json({ version: 2, settings: body.settings });
      }),
    );
    openSettings('privacy');
    const sw = await screen.findByRole('switch', { name: /Load remote images automatically/ });
    expect(sw).not.toBeChecked();
    expect(await axeViolations(document.body)).toEqual([]);
    await userEvent.click(sw);
    await waitFor(() => expect(sw).toBeChecked());
    expect(patches).toEqual([{ load_remote_images: true }]);
    expect(useStore.getState().userSettings?.settings.load_remote_images).toBe(true);
  });

  it('reflects a saved on state and turns it off', async () => {
    quietLists();
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 3, settings: { load_remote_images: true } }),
      ),
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        return HttpResponse.json({
          version: 4,
          settings: { load_remote_images: body.settings.load_remote_images },
        });
      }),
    );
    openSettings('privacy');
    const sw = await screen.findByRole('switch', { name: /Load remote images automatically/ });
    await waitFor(() => expect(sw).toBeChecked());
    await userEvent.click(sw);
    await waitFor(() => expect(sw).not.toBeChecked());
  });
});
