import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ThemeProvider } from '@sideroom/ui';
import { useSocial } from '../../../lounge/shell/social';
import { http, HttpResponse, server } from '../../../test/server';
import { FriendShelfShare } from '../storage/FriendShelfShare';
import { byosMockHandlers, mockMount } from '../storage/mocks';

const friendRow = (id: string, type = 1) => ({
  id,
  type,
  since: '2026-01-01T00:00:00Z',
  user: { id, username: `u${id}` },
});
const setFriends = (rows: ReturnType<typeof friendRow>[]) =>
  // The component only reads `relationships`; the load status keeps it from refetching.
  useSocial.setState({ relationships: rows as never, status: 'ready' });

const view = (id: string) =>
  render(
    <ThemeProvider section="lounge">
      <FriendShelfShare userId={id} />
    </ThemeProvider>,
  );

describe('Friend Shelf share toggles (BYOS-UI-001, against MSW mocks)', () => {
  it('renders nothing for a non-friend or without a mount', async () => {
    server.use(...byosMockHandlers({ mounts: [mockMount()] }));
    setFriends([friendRow('7', 3)]);
    view('7');
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByText('Share my Media Shelf')).toBeNull();
  });

  it('loads the current flags and saves each kind, deleting the share when all are off', async () => {
    const user = userEvent.setup();
    const calls: string[] = [];
    server.use(
      http.put('*/api/v1/users/@me/storage/mounts/:id/shares/:f', async ({ request, params }) => {
        calls.push(`PUT ${params.f} ${JSON.stringify(await request.json())}`);
        return HttpResponse.json({ friend_id: params.f });
      }),
      http.delete('*/api/v1/users/@me/storage/mounts/:id/shares/:f', ({ params }) => {
        calls.push(`DELETE ${params.f}`);
        return new HttpResponse(null, { status: 204 });
      }),
      ...byosMockHandlers({
        mounts: [mockMount()],
        shares: { '501': [{ friend_id: '7', cinema: true, library: false, music: false }] },
      }),
    );
    setFriends([friendRow('7')]);
    view('7');
    const cinema = await screen.findByRole('switch', { name: 'Cinema' });
    expect(cinema).toBeChecked();
    expect(screen.getByRole('switch', { name: 'Music' })).not.toBeChecked();
    await user.click(screen.getByRole('switch', { name: 'Music' }));
    await waitFor(() =>
      expect(calls).toContain('PUT 7 {"cinema":true,"library":false,"music":true}'),
    );
    await user.click(screen.getByRole('switch', { name: 'Cinema' }));
    await user.click(screen.getByRole('switch', { name: 'Music' }));
    await waitFor(() => expect(calls).toContain('DELETE 7'));
  });
});
