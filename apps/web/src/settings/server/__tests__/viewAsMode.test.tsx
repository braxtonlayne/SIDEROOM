import { act, render, renderHook, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import type { Channel } from '../../../lounge/data';
import { ViewAsBanner, useViewAs, useViewAsChannels } from '../viewAsMode';

const ch = (id: string, type: Channel['type'] = 'text'): Channel =>
  ({
    id,
    guildId: 'g1',
    type,
    name: id,
    position: 0,
    unread: false,
    mentionCount: 0,
    canSend: true,
  }) as Channel;
const all = [ch('c1'), ch('c2'), ch('cat', 'category')];

afterEach(() => act(() => useViewAs.getState().exit()));

describe('View Server As Role, Lounge mode (RDY-PAR-005)', () => {
  it('leaves the list alone without a preview and narrows only the previewed server', () => {
    const { result, rerender } = renderHook(({ g }) => useViewAsChannels(g, all), {
      initialProps: { g: 'g1' },
    });
    expect(result.current).toBe(all);
    act(() =>
      useViewAs
        .getState()
        .start({
          guildId: 'g1',
          guildName: 'Club',
          roleNames: ['Mod'],
          visibleIds: new Set(['c1', 'cat']),
        }),
    );
    expect(result.current.map((c) => c.id)).toEqual(['c1', 'cat']);
    rerender({ g: 'g2' });
    expect(result.current).toBe(all);
  });

  it('shows the exit banner with the roles and ends the preview', async () => {
    const root = createRootRoute({ component: ViewAsBanner });
    const router = createRouter({
      routeTree: root,
      history: createMemoryHistory({ initialEntries: ['/'] }),
    });
    render(<RouterProvider router={router} />);
    expect(screen.queryByTestId('view-as-banner')).toBeNull();
    act(() =>
      useViewAs
        .getState()
        .start({
          guildId: 'g1',
          guildName: 'Club',
          roleNames: ['Mod', 'Helper'],
          visibleIds: new Set(),
        }),
    );
    expect(await screen.findByTestId('view-as-banner')).toHaveTextContent(
      'Previewing Club as Mod, Helper.',
    );
    expect(screen.getByRole('region', { name: 'Server preview' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Exit preview' }));
    expect(useViewAs.getState().session).toBeNull();
    expect(screen.queryByTestId('view-as-banner')).toBeNull();
  });
});
