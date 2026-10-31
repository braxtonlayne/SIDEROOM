import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { useStore } from '../../store/store';

const open = () => {
  useStore.setState({
    currentUserId: '1',
    users: { '1': { id: '1', username: 'me' } as never },
  });
  const router = createAppRouter(createMemoryHistory({ initialEntries: ['/games'] }));
  render(<RouterProvider router={router} />);
  return router;
};

const rom = (over: Record<string, unknown>) => ({
  id: '1',
  mount_id: '7',
  key: 'emu/roms/a.nes',
  size: 40976,
  title: 'a',
  platform: 'nes',
  core: 'fceumm',
  scope: 'own',
  mount_label: 'Mine',
  ...over,
});

const handlers = (roms: unknown[]) => [
  http.get('*/api/v1/users/@me/games/roms', () => HttpResponse.json({ roms })),
  http.get('*/api/v1/users/@me/storage/mounts', () => HttpResponse.json([])),
  http.post('*/api/v1/storage/mounts/:id/presign', () =>
    HttpResponse.json({ url: 'https://bucket.example/rom', expires_at: '2099-01-01T00:00:00Z' }),
  ),
  http.get('https://bucket.example/rom', () => new HttpResponse(new Uint8Array(64))),
];

describe('the emulator page (ACT-EMU-001)', () => {
  it('lists the ROMs of the member and of a server shelf, with where each comes from', async () => {
    server.use(
      ...handlers([
        rom({ id: '1', title: 'Alpha Quest' }),
        rom({
          id: '2',
          key: 'emu/roms/b.gba',
          title: 'Beta Run',
          platform: 'gba',
          core: 'mgba',
          scope: 'guild',
          mount_label: 'Club shelf',
          guild_id: '99',
        }),
      ]),
    );
    open();
    expect(await screen.findByRole('heading', { name: 'Games', level: 1 })).toBeInTheDocument();
    expect(await screen.findByText('Alpha Quest')).toBeInTheDocument();
    expect(screen.getByText('Beta Run')).toBeInTheDocument();
    expect(screen.getByText(/NES - FCEUmm/)).toBeInTheDocument();
    expect(screen.getByText(/Game Boy Advance - mGBA/)).toBeInTheDocument();
    expect(screen.getByText(/your bucket/)).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: 'Play' })).toHaveLength(2);
    // The BIOS panel belongs to PlayStation games only.
    expect(screen.queryByRole('heading', { name: 'PlayStation BIOS' })).toBeNull();
  });

  it('says what to do when there are no games', async () => {
    server.use(...handlers([]));
    open();
    expect(await screen.findByText('No games yet')).toBeInTheDocument();
    expect(screen.getByText(/emu\/roms folder of your bucket/)).toBeInTheDocument();
  });

  it('filters the list by title', async () => {
    const user = userEvent.setup();
    server.use(
      ...handlers([rom({ id: '1', title: 'Alpha Quest' }), rom({ id: '2', title: 'Beta Run' })]),
    );
    open();
    await screen.findByText('Alpha Quest');
    await user.type(screen.getByRole('searchbox', { name: 'Search games' }), 'beta');
    expect(screen.queryByText('Alpha Quest')).toBeNull();
    expect(screen.getByText('Beta Run')).toBeInTheDocument();
  });

  it('explains why a PlayStation game needs your own BIOS, and will not start without one', async () => {
    const user = userEvent.setup();
    server.use(
      ...handlers([
        rom({
          id: '3',
          key: 'emu/roms/disc.pbp',
          title: 'Disc',
          platform: 'ps1',
          core: 'pcsx_rearmed',
        }),
      ]),
    );
    open();
    expect(await screen.findByRole('heading', { name: 'PlayStation BIOS' })).toBeInTheDocument();
    expect(screen.getByText(/never provides, stores or keeps one/)).toBeInTheDocument();
    expect(screen.getByText(/emu\/bios folder of your own bucket/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Play' }));
    expect(await screen.findByText('A PlayStation BIOS is needed')).toBeInTheDocument();
    await waitFor(() =>
      expect(document.querySelector('canvas[data-testid="emu-canvas"]')).toHaveAttribute(
        'data-phase',
        'error',
      ),
    );
  });

  it('names a core that is not installed', async () => {
    const user = userEvent.setup();
    server.use(
      ...handlers([
        rom({
          id: '4',
          key: 'emu/roms/s.sfc',
          title: 'Snes Game',
          platform: 'snes',
          core: 'supafaust',
        }),
      ]),
      http.get('*/emu/cores/supafaust/*', () => new HttpResponse(null, { status: 404 })),
    );
    open();
    await user.click(await screen.findByRole('button', { name: 'Play' }));
    expect(await screen.findByText('This emulator core is not installed here')).toBeInTheDocument();
    expect(screen.getByText(/Beetle Supafaust core is not on this instance/)).toBeInTheDocument();
  });
});
