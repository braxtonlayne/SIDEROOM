import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { renderAt } from '../../../cinema/__tests__/harness';
import { http, HttpResponse, server } from '../../../test/server';
import { playableKind, type ShelfDetail, type ShelfItem, type ShelfList } from '../api';

const OWN = { mount_id: '501', label: 'Home', scope: 'own' as const, owner_id: '1' };
const FRIEND = { mount_id: '502', label: 'Alex', scope: 'friend' as const, owner_id: '2' };
const GUILD = { mount_id: '503', label: 'Server', scope: 'guild' as const, guild_id: '9' };

const item = (id: string, o: Partial<ShelfItem> = {}): ShelfItem => ({
  id,
  section: 'cinema',
  kind: 'movie',
  title: `Film ${id}`,
  subtitle: '',
  year: 1979,
  source: OWN,
  tags: [],
  genres: [],
  synopsis: '',
  external_ids: {},
  overridden: [],
  parts: 0,
  object_key: `Movies/film-${id}.mp4`,
  container: 'mp4',
  web_native: true,
  ...o,
});

interface Seen {
  lists: string[];
  presigns: { mount: string; key: string }[];
  patches: unknown[];
  partPatches: unknown[];
  indexed: string[];
}

/** Three cards on three shelves; the mount query narrows it like the server does. */
const everything = (u: URL): ShelfList => {
  const all = [item('1'), item('2', { source: FRIEND }), item('3', { source: GUILD })];
  const mount = u.searchParams.get('mount');
  const items = mount ? all.filter((i) => i.source.mount_id === mount) : all;
  return { items, total: items.length, sources: [OWN, FRIEND, GUILD] };
};

function useShelfApi(
  o: {
    list?: (u: URL) => ShelfList;
    detail?: ShelfDetail;
    providers?: boolean;
  } = {},
) {
  const seen: Seen = { lists: [], presigns: [], patches: [], partPatches: [], indexed: [] };
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/shelves/cinema', ({ request }) => {
      const u = new URL(request.url);
      seen.lists.push(u.search);
      return HttpResponse.json(o.list?.(u) ?? everything(u));
    }),
    http.get('*/api/v1/shelves/cinema/:id', () =>
      o.detail
        ? HttpResponse.json(o.detail)
        : HttpResponse.json({ message: 'Not found' }, { status: 404 }),
    ),
    http.patch('*/api/v1/shelves/cinema/:id', async ({ request }) => {
      seen.patches.push(await request.json());
      return HttpResponse.json(o.detail);
    }),
    http.patch('*/api/v1/shelves/cinema/:id/parts/:part', async ({ request }) => {
      seen.partPatches.push(await request.json());
      return HttpResponse.json(o.detail);
    }),
    http.post('*/api/v1/storage/mounts/:mount/presign', async ({ params, request }) => {
      const b = (await request.json()) as { key: string };
      seen.presigns.push({ mount: String(params.mount), key: b.key });
      return HttpResponse.json({
        url: `https://bucket.example/${b.key}?X-Amz-Signature=mock`,
        expires_at: new Date(Date.now() + 900_000).toISOString(),
      });
    }),
    http.get('*/api/v1/storage/mounts/:mount/index', () =>
      HttpResponse.json({
        queued: false,
        state: 'done',
        objects: 12,
        added: 3,
        vanished: 1,
        truncated: false,
      }),
    ),
    http.post('*/api/v1/storage/mounts/:mount/index', ({ params }) => {
      seen.indexed.push(String(params.mount));
      return HttpResponse.json(
        { queued: true, state: 'running', objects: 0, added: 0, vanished: 0, truncated: false },
        { status: 202 },
      );
    }),
    http.get('*/api/v1/submissions/lookup/providers', () =>
      HttpResponse.json({
        enabled: o.providers ?? false,
        providers: o.providers ? [{ id: 'tmdb', label: 'TMDB' }] : [],
      }),
    ),
    http.get('*/api/v1/submissions/lookup', () =>
      HttpResponse.json({
        candidates: [
          {
            provider: 'tmdb',
            external_id: '348',
            title: 'Alien',
            alt_titles: [],
            year: 1979,
            synopsis: 'In space.',
            authors: [],
            genres: ['horror'],
            url: null,
            cover: '',
          },
        ],
      }),
    ),
  );
  return seen;
}

const detail = (it: ShelfItem, o: Partial<ShelfDetail> = {}): ShelfDetail => ({
  item: it,
  parts: [],
  can_edit: false,
  ...o,
});

describe('pure helpers', () => {
  it('decides what opening a file does', () => {
    expect(playableKind('mp4', true)).toBe('video');
    expect(playableKind('webm', true)).toBe('video');
    expect(playableKind('flac', true)).toBe('audio');
    expect(playableKind('mkv', false)).toBe('none');
    expect(playableKind('wma', false)).toBe('none');
    expect(playableKind('cbz', false)).toBe('cbz');
    expect(playableKind('epub', false)).toBe('epub');
    expect(playableKind('pdf', false)).toBe('pdf');
  });
});

describe('Shelves list (BYOS-CAT-002, against MSW)', () => {
  it('shows every card with its source chip', async () => {
    useShelfApi();
    renderAt('/cinema/shelves');
    expect(await screen.findByRole('heading', { name: 'Shelves' })).toBeInTheDocument();
    const cards = await screen.findAllByRole('article');
    expect(cards).toHaveLength(3);
    expect(within(cards[0]!).getByText('Your shelf')).toBeInTheDocument();
    expect(within(cards[1]!).getByText("A friend's shelf")).toBeInTheDocument();
    expect(within(cards[2]!).getByText('Server shelf')).toBeInTheDocument();
    expect(screen.getByText('3 of 3')).toBeInTheDocument();
  });

  it('filters by shelf: one chip per reachable mount, sent as the mount query', async () => {
    const user = userEvent.setup();
    const seen = useShelfApi();
    renderAt('/cinema/shelves');
    const group = await screen.findByRole('group', { name: 'Shelves' });
    await user.click(within(group).getByRole('button', { name: "A friend's shelf" }));
    await waitFor(() => expect(seen.lists.some((q) => q.includes('mount=502'))).toBe(true));
    await waitFor(() => expect(screen.getAllByRole('article')).toHaveLength(1));
    // Clicking again clears it.
    await user.click(within(group).getByRole('button', { name: "A friend's shelf" }));
    await waitFor(() => expect(screen.getAllByRole('article')).toHaveLength(3));
  });

  it('has no shelf filter when only one mount is reachable, and asks the server for a kind', async () => {
    const user = userEvent.setup();
    const seen = useShelfApi({ list: () => ({ items: [item('1')], total: 1, sources: [OWN] }) });
    renderAt('/cinema/shelves');
    await screen.findByRole('article');
    expect(screen.queryByRole('group', { name: 'Shelves' })).toBeNull();
    await user.click(
      within(screen.getByRole('group', { name: 'Kind' })).getByRole('button', { name: 'Anime' }),
    );
    await waitFor(() => expect(seen.lists.some((q) => q.includes('kind=anime'))).toBe(true));
  });

  it('explains an empty shelf and a search with no match', async () => {
    const user = userEvent.setup();
    useShelfApi({ list: () => ({ items: [], total: 0, sources: [] }) });
    renderAt('/cinema/shelves');
    expect(await screen.findByText('Nothing on your shelves yet')).toBeInTheDocument();
    await user.type(screen.getByRole('searchbox', { name: 'Search shelves' }), 'zzz');
    expect(await screen.findByText('Nothing matches that search.')).toBeInTheDocument();
  });

  it('is reachable from the Cinema nav and passes axe', async () => {
    useShelfApi();
    const { container } = renderAt('/cinema/shelves');
    const nav = await screen.findByRole('navigation', { name: 'Cinema' });
    expect(within(nav).getByRole('link', { name: 'Shelves' })).toBeInTheDocument();
    await screen.findAllByRole('article');
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe('Shelf item page', () => {
  it('plays a web-native film from a presigned URL', async () => {
    const user = userEvent.setup();
    const seen = useShelfApi({ detail: detail(item('1')) });
    renderAt('/cinema/shelves/1');
    expect(await screen.findByRole('heading', { name: 'Film 1' })).toBeInTheDocument();
    expect(screen.getByText('Your shelf')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Play' }));
    const video = await waitFor(() => {
      const v = document.querySelector('video');
      expect(v).not.toBeNull();
      return v!;
    });
    expect(video.getAttribute('src')).toContain('https://bucket.example/Movies/film-1.mp4');
    expect(seen.presigns).toEqual([{ mount: '501', key: 'Movies/film-1.mp4' }]);
  });

  it('greys out a file a browser cannot play', async () => {
    useShelfApi({
      detail: detail(
        item('1', { container: 'mkv', web_native: false, object_key: 'Movies/a.mkv' }),
      ),
    });
    renderAt('/cinema/shelves/1');
    const btn = await screen.findByRole('button', { name: 'Convert to play' });
    expect(btn).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Play' })).toBeNull();
  });

  it('lists the episodes of a series and opens one', async () => {
    const user = userEvent.setup();
    const seen = useShelfApi({
      detail: detail(
        item('1', { kind: 'series', title: 'Show', object_key: undefined, parts: 2 }),
        {
          parts: [
            {
              id: 'e1',
              season: 1,
              number: 1,
              title: 'Pilot',
              object_key: 'Series/Show/e1.mp4',
              container: 'mp4',
              web_native: true,
              size: 5,
            },
            {
              id: 'e2',
              season: 1,
              number: 2,
              title: 'Two',
              object_key: 'Series/Show/e2.mkv',
              container: 'mkv',
              web_native: false,
              size: 5,
            },
          ],
        },
      ),
    });
    renderAt('/cinema/shelves/1');
    expect(await screen.findByText(/S1E1 Pilot/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Convert to play' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Play' }));
    await waitFor(() =>
      expect(seen.presigns).toEqual([{ mount: '501', key: 'Series/Show/e1.mp4' }]),
    );
  });

  it('shows the provider attribution and that the member edited it', async () => {
    useShelfApi({
      detail: detail(
        item('1', {
          attribution: 'This product uses the TMDB API but is not endorsed or certified by TMDB.',
          overridden: ['title'],
        }),
      ),
    });
    renderAt('/cinema/shelves/1');
    expect(await screen.findByText(/not endorsed or certified by TMDB/)).toBeInTheDocument();
    expect(screen.getByText('Edited by you')).toBeInTheDocument();
  });

  it('offers no editor to a friend, and says so for an item that is not on a shelf they see', async () => {
    useShelfApi({ detail: detail(item('1', { source: FRIEND })) });
    renderAt('/cinema/shelves/1');
    expect(await screen.findByText("A friend's shelf")).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Edit details' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Re-index' })).toBeNull();
  });

  it('answers a missing item with an explanation', async () => {
    useShelfApi();
    renderAt('/cinema/shelves/99');
    expect(await screen.findByText('That item is not on a shelf you can see.')).toBeInTheDocument();
  });

  it('queues a re-index for the owner and shows the last run', async () => {
    const user = userEvent.setup();
    const seen = useShelfApi({ detail: detail(item('1'), { can_edit: true }) });
    renderAt('/cinema/shelves/1');
    expect(
      await screen.findByText(/Last run: 12 files, 3 new or changed, 1 gone/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Re-index' }));
    expect(await screen.findByText(/Indexing queued/)).toBeInTheDocument();
    expect(seen.indexed).toEqual(['501']);
  });

  it('edits the details: only changed fields are sent', async () => {
    const user = userEvent.setup();
    const seen = useShelfApi({
      detail: detail(item('1'), { can_edit: true }),
    });
    renderAt('/cinema/shelves/1');
    await user.click(await screen.findByRole('button', { name: 'Edit details' }));
    const title = await screen.findByRole('textbox', { name: 'Title' });
    await user.clear(title);
    await user.type(title, 'Alien (Director’s Cut)');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText(/Saved\./)).toBeInTheDocument();
    expect(seen.patches).toEqual([{ title: 'Alien (Director’s Cut)' }]);
  });

  it('fetches details from the lookup provider and sends the provider id, not an attribution text', async () => {
    const user = userEvent.setup();
    const seen = useShelfApi({ detail: detail(item('1'), { can_edit: true }), providers: true });
    renderAt('/cinema/shelves/1');
    await user.click(await screen.findByRole('button', { name: 'Edit details' }));
    const fetchBtn = await screen.findByRole('button', { name: 'Fetch details' });
    await waitFor(() => expect(fetchBtn).toBeEnabled());
    await user.click(fetchBtn);
    await user.click(await screen.findByRole('button', { name: 'Use these details' }));
    expect(screen.getByRole('textbox', { name: 'Synopsis' })).toHaveValue('In space.');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(seen.patches).toHaveLength(1));
    const body = seen.patches[0] as Record<string, unknown>;
    expect(body.details).toEqual({ provider: 'tmdb', external_id: '348' });
    expect(body).not.toHaveProperty('attribution');
    expect(body.title).toBe('Alien');
  });

  it('says lookup is off when no provider is on, and passes axe with the editor open', async () => {
    const user = userEvent.setup();
    useShelfApi({ detail: detail(item('1'), { can_edit: true }) });
    const { container } = renderAt('/cinema/shelves/1');
    await user.click(await screen.findByRole('button', { name: 'Edit details' }));
    expect(
      await screen.findByText('Lookup is not switched on for this server.'),
    ).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
  });

  it('edits an episode: number, title and season', async () => {
    const user = userEvent.setup();
    const seen = useShelfApi({
      detail: detail(item('1', { kind: 'series', object_key: undefined, parts: 1 }), {
        can_edit: true,
        parts: [
          {
            id: 'e1',
            season: 1,
            number: 1,
            title: 'Pilot',
            object_key: 'Series/S/e1.mp4',
            container: 'mp4',
            web_native: true,
            size: 5,
          },
        ],
      }),
    });
    renderAt('/cinema/shelves/1');
    await user.click(await screen.findByRole('button', { name: 'Edit details' }));
    const num = await screen.findByRole('textbox', { name: 'Number of S1E1' });
    await user.clear(num);
    await user.type(num, '7');
    await user.click(screen.getByRole('button', { name: 'Save S1E1' }));
    await waitFor(() =>
      expect(seen.partPatches).toEqual([{ title: 'Pilot', number: 7, season: 1 }]),
    );
  });
});
