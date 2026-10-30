import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactElement } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { EQ_PRESETS } from '../../eq/presets';
import * as playlistsApi from '../../playlists/api';
import { setPlayerPort, type PlayerPort, type PortState } from '../../queue/port';
import type { LibraryTrack } from '../../queue/types';
import * as searchApi from '../../search/api';
import { LibraryManagerPanel } from '../LibraryManagerPanel';

vi.mock('../../search/api', () => ({
  fetchLibrary: vi.fn(),
  presignTrack: vi.fn(async (id: string) => `https://bucket.example/${id}.mp3?sig=1`),
}));
vi.mock('../../playlists/api', () => ({
  listOwnPlaylists: vi.fn(),
  listGuildPlaylists: vi.fn(),
  listFavorites: vi.fn(),
  listRecent: vi.fn(),
  clearRecent: vi.fn(),
  addFavorite: vi.fn(async () => undefined),
  removeFavorite: vi.fn(async () => undefined),
  createPlaylist: vi.fn(),
  getPlaylist: vi.fn(),
  renamePlaylist: vi.fn(),
  deletePlaylist: vi.fn(),
  addToPlaylist: vi.fn(async () => ({})),
  removeFromPlaylist: vi.fn(),
  moveInPlaylist: vi.fn(),
}));

const trk = (id: string, o: Partial<LibraryTrack> = {}): LibraryTrack => ({
  id,
  mountId: 'm1',
  title: `Song ${id}`,
  artist: 'Ann',
  album: 'Alpha',
  genres: ['Rock'],
  ...o,
});
const T1 = trk('1', { durationMs: 185_000 });
const T2 = trk('2');

function fakePort(over: Partial<PortState> = {}) {
  let state: PortState = {
    queueIds: [],
    index: -1,
    eq: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    eqEnabled: true,
    crossfadeSec: 0,
    ...over,
  };
  const listeners = new Set<() => void>();
  const port = {
    play: vi.fn(),
    enqueue: vi.fn(),
    setEq: vi.fn((b: readonly number[]) => {
      state = { ...state, eq: [...b] };
      listeners.forEach((l) => l());
    }),
    setEqEnabled: vi.fn(),
    setCrossfade: vi.fn((s: number) => {
      state = { ...state, crossfadeSec: s };
      listeners.forEach((l) => l());
    }),
    getState: () => state,
    subscribe: (l: () => void) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
  } satisfies PlayerPort;
  return port;
}

function wrap(ui: ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

let off: () => void;
let port: ReturnType<typeof fakePort>;

beforeEach(() => {
  vi.mocked(searchApi.fetchLibrary).mockResolvedValue({ items: [T1, T2], total: 2 });
  vi.mocked(playlistsApi.listFavorites).mockResolvedValue({ items: [], hidden: 0 });
  vi.mocked(playlistsApi.listOwnPlaylists).mockResolvedValue([]);
  vi.mocked(playlistsApi.listRecent).mockResolvedValue({ items: [], hidden: 0 });
  port = fakePort();
  off = setPlayerPort(port);
});
afterEach(() => {
  off();
  vi.clearAllMocks();
});

describe('LibraryManagerPanel (MUS-LIB-001)', () => {
  it('searches the library and plays a result with a lazily presigned URL', async () => {
    const user = userEvent.setup();
    wrap(<LibraryManagerPanel />);
    expect(await screen.findByRole('button', { name: 'Play Song 1' })).toBeInTheDocument();
    expect(screen.getByText('3:05')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Play Song 2' }));
    expect(port.play).toHaveBeenCalledTimes(1);
    const [tracks, startAt] = port.play.mock.calls[0]!;
    expect(startAt).toBe(1);
    expect(tracks.map((t: { id: string }) => t.id)).toEqual(['1', '2']);
    // The queue holds no URL: it is resolved when the track is about to start.
    expect(tracks[1].src).toBe('');
    await expect(tracks[1].resolveSrc(tracks[1])).resolves.toContain('/2.mp3');
    expect(searchApi.presignTrack).toHaveBeenCalledWith('2');
  });

  it('sends the typed text to the server search after a short pause', async () => {
    const user = userEvent.setup();
    wrap(<LibraryManagerPanel />);
    await screen.findByRole('button', { name: 'Play Song 1' });
    await user.type(screen.getByRole('searchbox', { name: 'Search your music' }), 'alp');
    await waitFor(() =>
      expect(searchApi.fetchLibrary).toHaveBeenLastCalledWith({ q: 'alp', limit: 50 }),
    );
  });

  it('says so when nothing is reachable', async () => {
    vi.mocked(searchApi.fetchLibrary).mockResolvedValue({ items: [], total: 0 });
    wrap(<LibraryManagerPanel />);
    expect(await screen.findByText(/No tracks are reachable yet/)).toBeInTheDocument();
  });

  it('queues a track next and at the end', async () => {
    const user = userEvent.setup();
    wrap(<LibraryManagerPanel />);
    await user.click(await screen.findByRole('button', { name: 'More for Song 1' }));
    const group = screen.getByRole('group', { name: 'Actions for Song 1' });
    await user.click(within(group).getByRole('button', { name: 'Play next' }));
    await user.click(within(group).getByRole('button', { name: 'Add to queue' }));
    expect(port.enqueue.mock.calls.map((c) => c[1])).toEqual([{ next: true }, { next: false }]);
  });

  it('builds the album queue from the server and starts at the chosen track', async () => {
    const user = userEvent.setup();
    const a1 = trk('1', { trackNo: 1 });
    const a2 = trk('2', { trackNo: 2 });
    vi.mocked(searchApi.fetchLibrary).mockResolvedValue({ items: [a2, a1], total: 2 });
    wrap(<LibraryManagerPanel />);
    await user.click(await screen.findByRole('button', { name: 'More for Song 2' }));
    await user.click(screen.getByRole('button', { name: 'Play album' }));
    await waitFor(() => expect(port.play).toHaveBeenCalled());
    const [tracks, startAt] = port.play.mock.calls[0]!;
    expect(tracks.map((t: { id: string }) => t.id)).toEqual(['1', '2']);
    expect(startAt).toBe(1);
    expect(searchApi.fetchLibrary).toHaveBeenCalledWith(
      expect.objectContaining({ album: 'Alpha', artist: 'Ann', mount: 'm1' }),
    );
  });

  it('shuffle opens the queue with the chosen track and keeps every track once', async () => {
    const user = userEvent.setup();
    const tracks = [1, 2, 3, 4, 5].map((n) => trk(String(n), { trackNo: n }));
    vi.mocked(searchApi.fetchLibrary).mockResolvedValue({ items: tracks, total: 5 });
    wrap(<LibraryManagerPanel />);
    await user.click(await screen.findByRole('button', { name: 'Shuffle' }));
    expect(screen.getByRole('button', { name: 'Shuffle' })).toHaveAttribute('aria-pressed', 'true');
    await user.click(await screen.findByRole('button', { name: 'More for Song 3' }));
    await user.click(screen.getByRole('button', { name: 'Play album' }));
    await waitFor(() => expect(port.play).toHaveBeenCalled());
    const [queue, startAt] = port.play.mock.calls[0]!;
    expect(startAt).toBe(0);
    expect(queue[0].id).toBe('3');
    expect(queue.map((t: { id: string }) => t.id).sort()).toEqual(['1', '2', '3', '4', '5']);
  });

  it('favorites a track with the star', async () => {
    const user = userEvent.setup();
    wrap(<LibraryManagerPanel />);
    const star = await screen.findByRole('button', { name: 'Favorite Song 1' });
    expect(star).toHaveAttribute('aria-pressed', 'false');
    await user.click(star);
    await waitFor(() => expect(playlistsApi.addFavorite).toHaveBeenCalledWith('1'));
  });

  it('adds a track to one of the member’s playlists', async () => {
    const user = userEvent.setup();
    vi.mocked(playlistsApi.listOwnPlaylists).mockResolvedValue([
      {
        id: '77',
        owner_type: 'user',
        owner_id: '5',
        name: 'Road trip',
        description: '',
        item_count: 0,
        created_ms: 1,
        updated_ms: 1,
        can_edit: true,
      },
    ]);
    wrap(<LibraryManagerPanel />);
    await user.click(await screen.findByRole('button', { name: 'More for Song 1' }));
    const pick = await screen.findByRole('combobox', { name: 'Add Song 1 to a playlist' });
    await user.selectOptions(pick, '77');
    await waitFor(() => expect(playlistsApi.addToPlaylist).toHaveBeenCalledWith('77', ['1']));
  });

  it('creates a playlist from the Playlists tab', async () => {
    const user = userEvent.setup();
    vi.mocked(playlistsApi.createPlaylist).mockResolvedValue({
      id: '9',
      owner_type: 'user',
      owner_id: '5',
      name: 'New one',
      description: '',
      item_count: 0,
      created_ms: 1,
      updated_ms: 1,
      can_edit: true,
    });
    vi.mocked(playlistsApi.getPlaylist).mockResolvedValue({
      playlist: {
        id: '9',
        owner_type: 'user',
        owner_id: '5',
        name: 'New one',
        description: '',
        item_count: 0,
        created_ms: 1,
        updated_ms: 1,
        can_edit: true,
      },
      items: [],
      hidden: 0,
    });
    wrap(<LibraryManagerPanel />);
    await user.click(screen.getByRole('tab', { name: 'Playlists' }));
    await user.type(await screen.findByRole('textbox', { name: 'New playlist name' }), 'New one');
    await user.click(screen.getByRole('button', { name: 'Create playlist' }));
    await waitFor(() =>
      expect(playlistsApi.createPlaylist).toHaveBeenCalledWith('New one', undefined),
    );
    // It opens the new, empty playlist.
    expect(await screen.findByText(/This playlist is empty/)).toBeInTheDocument();
  });

  it('shows a playlist read-only when the member cannot edit it, and counts hidden tracks', async () => {
    const user = userEvent.setup();
    const pl = {
      id: '3',
      owner_type: 'guild' as const,
      owner_id: '50',
      name: 'Server mix',
      description: '',
      item_count: 3,
      created_ms: 1,
      updated_ms: 1,
      can_edit: false,
    };
    vi.mocked(playlistsApi.listOwnPlaylists).mockResolvedValue([pl]);
    vi.mocked(playlistsApi.getPlaylist).mockResolvedValue({
      playlist: pl,
      items: [{ itemId: '10', position: 0, track: T1 }],
      hidden: 2,
    });
    wrap(<LibraryManagerPanel />);
    await user.click(screen.getByRole('tab', { name: 'Playlists' }));
    await user.click(await screen.findByRole('button', { name: /Server mix/ }));
    expect(await screen.findByText(/2 tracks are hidden/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Delete playlist' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Remove Song 1 from the playlist' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Play all' })).toBeEnabled();
  });

  it('moves and removes items when the member may edit', async () => {
    const user = userEvent.setup();
    const pl = {
      id: '3',
      owner_type: 'user' as const,
      owner_id: '5',
      name: 'Mine',
      description: '',
      item_count: 2,
      created_ms: 1,
      updated_ms: 1,
      can_edit: true,
    };
    vi.mocked(playlistsApi.listOwnPlaylists).mockResolvedValue([pl]);
    vi.mocked(playlistsApi.getPlaylist).mockResolvedValue({
      playlist: pl,
      items: [
        { itemId: '10', position: 0, track: T1 },
        { itemId: '11', position: 1, track: T2 },
      ],
      hidden: 0,
    });
    vi.mocked(playlistsApi.moveInPlaylist).mockResolvedValue(undefined);
    vi.mocked(playlistsApi.removeFromPlaylist).mockResolvedValue(undefined);
    wrap(<LibraryManagerPanel />);
    await user.click(screen.getByRole('tab', { name: 'Playlists' }));
    await user.click(await screen.findByRole('button', { name: /Mine/ }));
    await user.click(await screen.findByRole('button', { name: 'Move Song 2 up' }));
    expect(playlistsApi.moveInPlaylist).toHaveBeenCalledWith('3', '11', 0);
    await user.click(screen.getByRole('button', { name: 'Remove Song 1 from the playlist' }));
    expect(playlistsApi.removeFromPlaylist).toHaveBeenCalledWith('3', '10');
    expect(screen.getByRole('button', { name: 'Move Song 1 up' })).toBeDisabled();
  });

  it('lists favorites and recent, and clears the history', async () => {
    const user = userEvent.setup();
    vi.mocked(playlistsApi.listFavorites).mockResolvedValue({
      items: [{ track: T2, atMs: 1 }],
      hidden: 0,
    });
    vi.mocked(playlistsApi.listRecent).mockResolvedValue({
      items: [{ track: T1, atMs: 1 }],
      hidden: 0,
    });
    vi.mocked(playlistsApi.clearRecent).mockResolvedValue(undefined);
    wrap(<LibraryManagerPanel />);
    await user.click(screen.getByRole('tab', { name: 'Favorites' }));
    expect(await screen.findByRole('button', { name: 'Play Song 2' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Recent' }));
    expect(await screen.findByRole('button', { name: 'Play Song 1' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Clear history' }));
    await waitFor(() => expect(playlistsApi.clearRecent).toHaveBeenCalled());
  });

  it('drives the EQ and the crossfade through the player port', async () => {
    const user = userEvent.setup();
    wrap(<LibraryManagerPanel />);
    await user.click(screen.getByRole('tab', { name: 'Equalizer' }));
    await user.selectOptions(screen.getByRole('combobox', { name: 'Preset' }), 'bassBoost');
    expect(port.setEq).toHaveBeenLastCalledWith(EQ_PRESETS.bassBoost);
    // The select now reads the preset back from the port, and a hand-moved slider makes it custom.
    expect(screen.getByRole('combobox', { name: 'Preset' })).toHaveValue('bassBoost');
    const slider = screen.getByRole('slider', { name: '1 kHz band' });
    fireEvent.change(slider, { target: { value: '5' } });
    expect(port.setEq).toHaveBeenCalledTimes(2);
    expect(screen.getByRole('combobox', { name: 'Preset' })).toHaveValue('custom');
    const fade = screen.getByRole('slider', { name: 'Crossfade' });
    fireEvent.change(fade, { target: { value: '3' } });
    expect(port.setCrossfade).toHaveBeenLastCalledWith(3);
    await user.click(screen.getByRole('checkbox', { name: 'Equalizer on' }));
    expect(port.setEqEnabled).toHaveBeenCalledWith(false);
  });

  it('moves between sections with the arrow keys', async () => {
    const user = userEvent.setup();
    wrap(<LibraryManagerPanel />);
    const first = screen.getByRole('tab', { name: 'Library' });
    first.focus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Playlists' })).toHaveAttribute('aria-selected', 'true');
  });

  it('has no accessibility violations (search, playlists and equalizer)', async () => {
    const user = userEvent.setup();
    const { container } = wrap(<LibraryManagerPanel />);
    await screen.findByRole('button', { name: 'Play Song 1' });
    expect(await axeViolations(container)).toEqual([]);
    await user.click(screen.getByRole('tab', { name: 'Playlists' }));
    await screen.findByRole('textbox', { name: 'New playlist name' });
    expect(await axeViolations(container)).toEqual([]);
    await user.click(screen.getByRole('tab', { name: 'Equalizer' }));
    expect(await axeViolations(container)).toEqual([]);
  });
});
