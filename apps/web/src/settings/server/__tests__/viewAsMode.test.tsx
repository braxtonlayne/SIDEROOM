import { act, render, renderHook, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import type { Channel } from '../../../lounge/data';
import {
  ViewAsBanner,
  useViewAs,
  useViewAsChannels,
  useViewAsMembers,
  useViewAsThreadParent,
} from '../viewAsMode';

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
      useViewAs.getState().start({
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
      useViewAs.getState().start({
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

describe('View Server As Role filters members and threads (PAR-FU-001)', () => {
  const member = (id: string, roleIds: string[]) => ({ user: { id }, roleIds });
  const members = [member('owner', []), member('staff', ['r-staff']), member('plain', [])];
  // A staff-only channel: only members holding r-staff can view it.
  const canSeeChannel = (channelId: string, roleIds: readonly string[]) =>
    channelId !== 'c-staff' || roleIds.includes('r-staff');
  const start = (visible: string[]) =>
    act(() =>
      useViewAs.getState().start({
        guildId: 'g1',
        guildName: 'Club',
        roleNames: ['Reader'],
        visibleIds: new Set(visible),
        canSeeChannel,
      }),
    );

  it('leaves the member list alone without a preview, in other servers and without a channel', () => {
    const { result, rerender } = renderHook(
      ({ g, c }) => useViewAsMembers(g, c, 'owner', members),
      { initialProps: { g: 'g1', c: 'c-staff' as string | undefined } },
    );
    expect(result.current).toBe(members);
    start(['c-staff']);
    expect(result.current.map((m) => m.user.id)).toEqual(['owner', 'staff']);
    rerender({ g: 'g2', c: 'c-staff' });
    expect(result.current).toBe(members);
    rerender({ g: 'g1', c: undefined });
    expect(result.current).toBe(members);
  });

  it('hides members who cannot view the open channel and empties it when the previewed roles cannot', () => {
    const { result, rerender } = renderHook(
      ({ c }) => useViewAsMembers('g1', c, 'owner', members),
      {
        initialProps: { c: 'c-open' },
      },
    );
    start(['c-open', 'c-staff']);
    expect(result.current.map((m) => m.user.id)).toEqual(['owner', 'staff', 'plain']);
    rerender({ c: 'c-staff' });
    expect(result.current.map((m) => m.user.id)).toEqual(['owner', 'staff']);
    rerender({ c: 'c-hidden' });
    expect(result.current).toEqual([]);
  });

  it('shows threads only of channels the previewed roles can read', () => {
    const { result } = renderHook(() => useViewAsThreadParent('g1'));
    expect(result.current('c-hidden')).toBe(true);
    start(['c-open']);
    expect(result.current('c-open')).toBe(true);
    expect(result.current('c-hidden')).toBe(false);
  });
});
