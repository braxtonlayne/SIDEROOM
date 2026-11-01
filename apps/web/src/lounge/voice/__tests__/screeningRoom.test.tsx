import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { http, HttpResponse, server } from '../../../test/server';
import { ScreeningRoom } from '../ScreeningRoom';

// STU-SHF-001: Watch Together and the Live TV guide sit under the voice channel's Screening Room.
describe('Screening Room', () => {
  it('offers Watch Together, the shelves and the Live TV guide', async () => {
    server.use(
      http.get('*/api/v1/channels/:id/party', () =>
        HttpResponse.json({ message: 'none' }, { status: 404 }),
      ),
    );
    const root = createRootRoute({ component: () => <ScreeningRoom channelId="42" /> });
    const tv = createRoute({
      getParentRoute: () => root,
      path: '/cinema/tv',
      component: () => <p>guide</p>,
    });
    const router = createRouter({
      routeTree: root.addChildren([tv]),
      history: createMemoryHistory({ initialEntries: ['/'] }),
    });
    render(<RouterProvider router={router} />);
    const room = await screen.findByText('Screening Room');
    const user = userEvent.setup();
    await user.click(room);
    const box = room.closest('details')!;
    expect(within(box).getByRole('button', { name: 'Watch Together' })).toBeInTheDocument();
    expect(within(box).getByRole('button', { name: 'Pick from shelves' })).toBeInTheDocument();
    await user.click(within(box).getByRole('button', { name: 'Live TV guide' }));
    expect(router.state.location.pathname).toBe('/cinema/tv');
  });
});
