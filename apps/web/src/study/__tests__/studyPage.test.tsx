import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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
}

function useStudyApi(canEdit: boolean): Seen {
  const seen: Seen = { puts: [] };
  const notes: { key: string; title?: string; note?: string; cover_key?: string }[] = [];
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/users/@me/storage/mounts', () =>
      HttpResponse.json([{ id: '501', label: 'Home', status: 'active' }]),
    ),
    http.get('*/api/v1/users/@me/storage/shares', () =>
      HttpResponse.json([
        {
          mount_id: '502',
          owner_id: '20',
          label: 'Alex stuff',
          cinema: true,
          library: true,
          music: false,
        },
      ]),
    ),
    http.get('*/api/v1/users/@me/games/roms', () => HttpResponse.json({ roms: [] })),
    http.get('*/api/v1/shelves/:section', ({ params }) =>
      HttpResponse.json({
        items: [],
        total: 0,
        sources:
          params.section === 'cinema'
            ? [{ mount_id: '503', label: 'Club shelf', scope: 'guild', guild_id: '70' }]
            : [],
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
    http.get('*/api/v1/storage/mounts/:mount/notes', () =>
      HttpResponse.json({ notes, can_edit: canEdit }),
    ),
    http.put('*/api/v1/storage/mounts/:mount/notes', async ({ request }) => {
      const b = (await request.json()) as { key: string; title: string };
      seen.puts.push(b);
      notes.push({ key: b.key, title: b.title });
      return new HttpResponse(null, { status: 204 });
    }),
    http.post('*/api/v1/storage/mounts/:mount/presign', () =>
      HttpResponse.json({
        url: 'https://bucket.example/x?sig=1',
        expires_at: new Date(Date.now() + 600_000).toISOString(),
      }),
    ),
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
    expect(within(nav).getByText('Guild Shelves')).toBeInTheDocument();
    // My Files is selected first.
    expect(await screen.findByRole('button', { name: /Open a, @me's bucket/ })).toBeInTheDocument();
    expect(screen.getAllByText("@me's bucket").length).toBeGreaterThanOrEqual(4);

    await user.click(within(nav).getByRole('button', { name: 'alex' }));
    expect(
      await screen.findByRole('button', { name: /Open d, @alex's bucket/ }),
    ).toBeInTheDocument();
    expect(screen.getByText("@alex's bucket")).toBeInTheDocument();

    await user.click(within(nav).getByRole('button', { name: 'Club' }));
    expect(await screen.findByRole('button', { name: /Open e, Guild: Club/ })).toBeInTheDocument();
    expect(screen.getByText('Guild: Club')).toBeInTheDocument();
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
    expect(seen.puts[0]).toEqual({ key: 'Films/a.mp4', title: 'My cut', note: '', cover_key: '' });
    expect(await screen.findByRole('button', { name: /Open My cut, / })).toBeInTheDocument();
  });

  it('shows no edit control to someone who does not own the shelf', async () => {
    useStudyApi(false);
    open();
    await screen.findByRole('button', { name: /Open a, / });
    expect(screen.queryByRole('button', { name: /Edit details of/ })).toBeNull();
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
