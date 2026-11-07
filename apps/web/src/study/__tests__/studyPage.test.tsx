import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createHash } from 'node:crypto';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../router';
import { useStore } from '../../store/store';
import { http, HttpResponse, server } from '../../test/server';

const open = () => {
  const router = createAppRouter(createMemoryHistory({ initialEntries: ['/study'] }));
  render(<RouterProvider router={router} />);
  return router;
};

const FILES: Record<string, { key: string; size: number }[]> = {
  '501': [
    { key: 'Films/a.mp4', size: 10 },
    { key: 'Music/b.mp3', size: 20 },
    { key: 'Books/c.epub', size: 30 },
    { key: 'notes.txt', size: 5 },
  ],
  '502': [{ key: 'Shared/d.cbz', size: 40 }],
  '503': [{ key: 'Club/e.mkv', size: 50 }],
};

interface Seen {
  puts: unknown[];
  urls: string[];
}

const sha = (k: string) => createHash('sha256').update(k, 'utf8').digest('hex');

function useStudyApi(canEdit: boolean): Seen {
  const seen: Seen = { puts: [], urls: [] };
  const notes = new Map<string, { title: string; note: string; cover: string }>();
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/shelves/visible', () =>
      HttpResponse.json({
        shelves: [
          {
            shelf_id: '501',
            label: 'Home',
            via: 'own',
            owner_id: '1',
            object_count: 4,
            can_manage: true,
          },
          {
            shelf_id: '502',
            label: 'Alex stuff',
            via: 'friend',
            owner_id: '20',
            owner_handle: 'alex',
            object_count: 1,
            can_manage: false,
          },
          {
            shelf_id: '503',
            label: 'Club shelf',
            via: 'guild',
            guild_id: '70',
            guild_name: 'Club',
            object_count: 1,
            can_manage: false,
          },
        ],
      }),
    ),
    http.get('*/api/v1/storage/mounts/:mount/objects', ({ params }) => {
      const files = FILES[String(params.mount)] ?? [];
      return HttpResponse.json({
        folders: [],
        objects: files.map((f) => ({ ...f, last_modified: '2026-10-01T00:00:00Z' })),
        next_cursor: null,
        can_upload: false,
        can_manage: canEdit,
      });
    }),
    http.get('*/api/v1/shelves/:shelf/objects', ({ params }) => {
      const files = FILES[String(params.shelf)] ?? [];
      return HttpResponse.json({
        objects: files.map((f) => ({
          object_hash: sha(f.key),
          path: f.key,
          size: f.size,
          class: 'other',
          title: notes.get(f.key)?.title ?? '',
          cover: notes.get(f.key)?.cover ?? '',
          note: notes.get(f.key)?.note ?? '',
          updated_at: '2026-10-01T00:00:00Z',
        })),
        next_cursor: null,
        can_edit: canEdit,
      });
    }),
    http.get('*/api/v1/shelves/:shelf/objects/:hash/url', ({ params }) => {
      seen.urls.push(String(params.hash));
      return HttpResponse.json({
        url: 'https://bucket.example/x?sig=1',
        expires_at: new Date(Date.now() + 600_000).toISOString(),
      });
    }),
    // The PATCH records the body against the path its hash names (the catalog hash is sha256 of the path).
    http.patch('*/api/v1/shelves/:shelf/objects/:hash', async ({ params, request }) => {
      const b = (await request.json()) as { title: string; note: string; cover: string };
      const files = Object.values(FILES).flat();
      const f = files.find((x) => sha(x.key) === params.hash);
      if (!f) return HttpResponse.json({ code: 10004, message: 'Unknown' }, { status: 404 });
      seen.puts.push({ key: f.key, ...b });
      notes.set(f.key, b);
      return HttpResponse.json({});
    }),
  );
  return seen;
}

beforeEach(() => {
  localStorage.clear();
  useStore.setState({
    currentUserId: '1',
    users: { '1': { id: '1', username: 'me' }, '20': { id: '20', username: 'alex' } },
    guilds: { '70': { id: '70', name: 'Club' } },
  } as never);
});

describe('The Study page (STU-SHF-001)', () => {
  it('lists the sidebar by source and puts a provenance badge on every card', async () => {
    const user = userEvent.setup();
    useStudyApi(false);
    open();
    const nav = await screen.findByRole('navigation', { name: 'Shelves by source' });
    expect(await within(nav).findByRole('button', { name: 'My Files' })).toBeInTheDocument();
    expect(within(nav).getByText("Friends' Shelves")).toBeInTheDocument();
    expect(within(nav).getByText('Server Shelves')).toBeInTheDocument();
    // My Files is selected first.
    expect(await screen.findByRole('button', { name: /Open a, @me's bucket/ })).toBeInTheDocument();
    expect(screen.getAllByText("@me's bucket").length).toBeGreaterThanOrEqual(4);

    await user.click(within(nav).getByRole('button', { name: 'alex' }));
    expect(
      await screen.findByRole('button', { name: /Open d, @alex's bucket/ }),
    ).toBeInTheDocument();
    expect(screen.getByText("@alex's bucket")).toBeInTheDocument();

    await user.click(within(nav).getByRole('button', { name: 'Club' }));
    expect(await screen.findByRole('button', { name: /Open e, Server: Club/ })).toBeInTheDocument();
    expect(screen.getByText('Server: Club')).toBeInTheDocument();
  });

  it('filters by file type', async () => {
    const user = userEvent.setup();
    useStudyApi(false);
    open();
    await screen.findByRole('button', { name: /Open a, / });
    await user.click(screen.getByRole('button', { name: 'Video (1)' }));
    expect(screen.getByRole('button', { name: /Open a, / })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Open b, / })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Video (1)' }));
    await user.click(screen.getByRole('button', { name: 'Reading (1)' }));
    expect(screen.getByRole('button', { name: /Open c, / })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Open a, / })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Reading (1)' }));
    await user.click(screen.getByRole('button', { name: 'Other (1)' }));
    expect(screen.getByRole('button', { name: /Open notes, / })).toBeInTheDocument();
  });

  it('lets the owner edit a title; a viewer gets no editor', async () => {
    const user = userEvent.setup();
    const seen = useStudyApi(true);
    open();
    await user.click(await screen.findByRole('button', { name: 'Edit details of a' }));
    const dialog = await screen.findByRole('dialog');
    const title = within(dialog).getByLabelText('Title');
    await user.type(title, 'My cut');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(seen.puts).toHaveLength(1));
    expect(seen.puts[0]).toEqual({ key: 'Films/a.mp4', title: 'My cut', note: '', cover: '' });
    expect(await screen.findByRole('button', { name: /Open My cut, / })).toBeInTheDocument();
  });

  it('shows no edit control to someone who does not own the shelf', async () => {
    useStudyApi(false);
    open();
    await screen.findByRole('button', { name: /Open a, / });
    expect(screen.queryByRole('button', { name: /Edit details of/ })).toBeNull();
  });

  it('opens a file through its per-object URL (the catalog hash of its path)', async () => {
    const user = userEvent.setup();
    const seen = useStudyApi(false);
    const opened = vi.spyOn(window, 'open').mockReturnValue(null);
    open();
    await user.click(await screen.findByRole('button', { name: /Open notes, / }));
    const dialog = await screen.findByRole('dialog');
    await user.click(within(dialog).getByRole('button', { name: 'Open file' }));
    await waitFor(() => expect(opened).toHaveBeenCalled());
    expect(seen.urls).toContain(sha('notes.txt'));
    expect(opened.mock.calls[0]?.[0]).toBe('https://bucket.example/x?sig=1');
    opened.mockRestore();
  });

  it('shows the recently added rail and has no axe violations', async () => {
    useStudyApi(false);
    open();
    await screen.findByRole('button', { name: /Open a, / });
    expect(
      await screen.findByRole('region', { name: 'Recently Added to Shelves You Can See' }),
    ).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
