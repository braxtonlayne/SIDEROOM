/**
 * Wave-30 integration: the library panel starts the lazy tagger on its first listing (MUS-IDX-001) and offers
 * "Rescan tags" for the buckets whose tags the member may write.
 */
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setTagger, Tagger, type TaggerApi } from '../../index';
import type { LibraryTrack } from '../../queue/types';
import * as searchApi from '../../search/api';
import { LibraryManagerPanel } from '../LibraryManagerPanel';
import { __resetTaggerStart, editableMounts } from '../tagging';

vi.mock('../../search/api', () => ({
  fetchLibrary: vi.fn(),
  presignTrack: vi.fn(async (id: string) => `/${id}.mp3`),
}));
vi.mock('../../playlists/api', () => ({
  listOwnPlaylists: vi.fn(async () => []),
  listGuildPlaylists: vi.fn(async () => []),
  listFavorites: vi.fn(async () => ({ items: [], hidden: 0 })),
  listRecent: vi.fn(async () => ({ items: [], hidden: 0 })),
}));

const trk = (id: string, o: Partial<LibraryTrack> = {}): LibraryTrack => ({
  id,
  mountId: 'm1',
  title: `Song ${id}`,
  artist: 'Ann',
  album: 'Alpha',
  genres: [],
  ...o,
});

let api: { [K in keyof TaggerApi]: ReturnType<typeof vi.fn> };

beforeEach(() => {
  __resetTaggerStart();
  api = {
    listPending: vi.fn(async () => ({ items: [], total: 0 })),
    presign: vi.fn(),
    patch: vi.fn(),
    rescan: vi.fn(async () => ({ queued: 0 })),
    uploadCover: vi.fn(),
  };
  setTagger(
    new Tagger({
      api: api as unknown as TaggerApi,
      reader: async () => ({ readTags: vi.fn(), readTagsTail: vi.fn() }),
    }),
  );
});
afterEach(() => setTagger(null));

function mount() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <LibraryManagerPanel />
    </QueryClientProvider>,
  );
}

describe('tagging in the library panel', () => {
  it('starts the tagger once, on the first listing', async () => {
    vi.mocked(searchApi.fetchLibrary).mockResolvedValue({
      items: [trk('1'), trk('2')],
      total: 2,
    });
    const { unmount } = mount();
    await screen.findByText('Song 1');
    await waitFor(() => expect(api.listPending).toHaveBeenCalledTimes(1));
    unmount();
    mount();
    await screen.findByText('Song 1');
    expect(api.listPending).toHaveBeenCalledTimes(1);
  });

  it('does not start on an empty library', async () => {
    vi.mocked(searchApi.fetchLibrary).mockResolvedValue({ items: [], total: 0 });
    mount();
    await screen.findByText(/no music|nothing/i).catch(() => undefined);
    await new Promise((r) => setTimeout(r, 20));
    expect(api.listPending).not.toHaveBeenCalled();
  });

  it('offers Rescan tags for an editable bucket only, and it rescans that bucket', async () => {
    vi.mocked(searchApi.fetchLibrary).mockResolvedValue({
      items: [trk('1', { canEdit: true }), trk('2', { mountId: 'm2' })],
      total: 2,
    });
    mount();
    const btn = await screen.findByRole('button', { name: 'Rescan tags' });
    expect(screen.getAllByRole('button', { name: 'Rescan tags' })).toHaveLength(1);
    await userEvent.click(btn);
    await waitFor(() => expect(api.rescan).toHaveBeenCalledWith('m1'));
  });

  it('editableMounts keeps listing order and drops duplicates', () => {
    expect(
      editableMounts([
        trk('1', { mountId: 'b', canEdit: true }),
        trk('2', { mountId: 'a', canEdit: true }),
        trk('3', { mountId: 'b', canEdit: true }),
        trk('4', { mountId: 'c' }),
      ]),
    ).toEqual(['b', 'a']);
  });
});
