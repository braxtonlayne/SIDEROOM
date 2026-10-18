// Phase 13 Library admin: the chapter page editor (reorder, spreads, remove), credit revocation and the
// comment report queue. LIB-ADM-001, LIB-ADM-002, LIB-SOC-003.
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { keyOfUrl, toPageInput, type AdminPage } from '../library/api';
import { PagesDialog } from '../library/PagesDialog';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const page = (i: number, over: Partial<AdminPage> = {}): AdminPage => ({
  index: i,
  width: 1000,
  height: 1500,
  is_double_spread: false,
  blurhash: null,
  url: `/cdn/library/7/11/000${i}.webp?exp=9&sig=abc`,
  variants: [{ width: 720, url: `/cdn/library/7/11/000${i}_720.webp?exp=9&sig=abc` }],
  ...over,
});

const chapter = {
  id: '11',
  series_id: '7',
  edition_id: '1',
  number: 3,
  title: null,
  page_count: 3,
  format: 'images' as const,
  published: true,
};

describe('page helpers', () => {
  it('reads the storage key out of a signed CDN URL', () => {
    expect(keyOfUrl('/cdn/library/7/11/0001.webp?exp=9&sig=abc')).toBe('library/7/11/0001.webp');
    expect(keyOfUrl('https://cdn.example/cdn/library/7/11/a%20b.webp?x=1')).toBe(
      'library/7/11/a b.webp',
    );
  });

  it('turns a manifest page into the replacement body', () => {
    expect(toPageInput(page(1, { is_double_spread: true }))).toEqual({
      key: 'library/7/11/0001.webp',
      variants: { '720': 'library/7/11/0001_720.webp' },
      width: 1000,
      height: 1500,
      is_double_spread: true,
      blurhash: null,
      crop: null,
    });
  });
});

describe('page editor (LIB-ADM-001)', () => {
  it('reorders, marks a spread, removes and saves the whole list', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PUBLISH_READING'] });
    let sent: unknown = null;
    server.use(
      http.get('*/api/v1/admin/library/chapters/11/pages', () =>
        HttpResponse.json({ chapter, pages: [page(0), page(1), page(2)] }),
      ),
      http.put('*/api/v1/admin/library/chapters/11/pages', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(chapter);
      }),
    );
    const user = userEvent.setup();
    render(<PagesDialog chapter={chapter} onClose={() => undefined} onChanged={() => undefined} />);
    const list = await screen.findByRole('list', { name: 'Pages in reading order' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(3);
    await user.click(screen.getByRole('button', { name: 'Move page 3 up' }));
    await user.click(screen.getByRole('button', { name: 'Remove page 1' }));
    await user.click(within(list).getAllByRole('checkbox', { name: 'Double spread' })[0]!);
    await user.click(screen.getByRole('button', { name: 'Save page order' }));
    await waitFor(() => expect(sent).not.toBeNull());
    const body = sent as { key: string; is_double_spread: boolean }[];
    // Pages 1, 2, 3 -> 3 and 2 swapped -> 1, 3, 2 -> page 1 removed -> 3, 2.
    expect(body.map((p) => p.key)).toEqual(['library/7/11/0002.webp', 'library/7/11/0001.webp']);
    expect(body[0]!.is_double_spread).toBe(true);
  });
});

describe('single-page replacement (H-43)', () => {
  it('sends the chosen image for that page index as a data URI and reloads the list', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PUBLISH_READING'] });
    let sent: { image: string } | null = null;
    let reloads = 0;
    server.use(
      http.get('*/api/v1/admin/library/chapters/11/pages', () => {
        reloads += 1;
        return HttpResponse.json({ chapter, pages: [page(0), page(1), page(2)] });
      }),
      http.put('*/api/v1/admin/library/chapters/11/pages/1', async ({ request }) => {
        sent = (await readJson(request)) as { image: string };
        return HttpResponse.json(chapter);
      }),
    );
    const user = userEvent.setup();
    render(<PagesDialog chapter={chapter} onClose={() => undefined} onChanged={() => undefined} />);
    await screen.findByRole('list', { name: 'Pages in reading order' });
    await user.click(screen.getByRole('button', { name: 'Replace page 2' }));
    const file = new File([new Uint8Array([137, 80, 78, 71])], 'new.png', { type: 'image/png' });
    await user.upload(screen.getByLabelText('New image for the page'), file);
    await waitFor(() => expect(sent).not.toBeNull());
    expect(sent!.image).toMatch(/^data:image\/png;base64,/);
    await waitFor(() => expect(reloads).toBe(2));
    expect(await screen.findByText('The page list was saved.')).toBeInTheDocument();
    // Reordering first locks Replace (indexes are the server's).
    await user.click(screen.getByRole('button', { name: 'Move page 3 up' }));
    expect(screen.getByRole('button', { name: 'Replace page 1' })).toBeDisabled();
  });
});

describe('credits and comment reports (LIB-ADM-002)', () => {
  it('loads a submission, revokes its credits with a reason, and resolves a report', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PUBLISH_READING', 'REVIEW_READING'] });
    let revoked: unknown = null;
    let resolved: unknown = null;
    server.use(
      http.get('*/api/v1/admin/library/series', () =>
        HttpResponse.json({ items: [], total: 0, limit: 50, offset: 0 }),
      ),
      http.get('*/api/v1/admin/review/submissions/55/credits', () =>
        HttpResponse.json([
          {
            id: '1',
            section: 'library',
            ref_id: '11',
            user_id: '9',
            submission_id: '55',
            label: 'Chapter 3',
            granted_at_ms: 1_700_000_000_000,
          },
        ]),
      ),
      http.post('*/api/v1/admin/review/submissions/55/revoke-credits', async ({ request }) => {
        revoked = await readJson(request);
        return HttpResponse.json({ revoked: 1 });
      }),
      http.get('*/api/v1/admin/library/comment-reports', () =>
        HttpResponse.json({
          items: [
            {
              id: '8',
              reporter_id: '2',
              reason: 'spam',
              status: 'open',
              created_at_ms: 1_700_000_000_000,
              comment: {
                id: '4',
                body: 'Buy cheap watches',
                author_name: 'Spammer',
                series_id: '7',
              },
            },
          ],
        }),
      ),
      http.post('*/api/v1/admin/library/comment-reports/8/resolve', async ({ request }) => {
        resolved = await readJson(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('library-catalog');
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('Submission ID'), '55');
    await user.click(screen.getByRole('button', { name: 'Load credits' }));
    expect(await screen.findByText('Chapter 3')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Revoke credits' }));
    const dialog = await screen.findByRole('dialog', { name: 'Revoke credits' });
    await user.type(within(dialog).getByLabelText('Reason'), 'Stolen scans');
    await user.click(within(dialog).getByRole('button', { name: 'Revoke' }));
    await waitFor(() => expect(revoked).toEqual({ reason: 'Stolen scans' }));

    expect(await screen.findByText('Buy cheap watches')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Hide comment' }));
    await waitFor(() => expect(resolved).toEqual({ hide: true }));
  });
});

describe('featured reading lists (ADM-CNT-004)', () => {
  it('lists the featured lists, features one by id and takes one away', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PUBLISH_READING'] });
    let featured = [{ id: '31', name: 'Quiet mysteries', owner_name: 'Mina', item_count: 4 }];
    const calls: unknown[] = [];
    server.use(
      http.get('*/api/v1/admin/library/series', () =>
        HttpResponse.json({ items: [], total: 0, limit: 50, offset: 0 }),
      ),
      http.get('*/api/v1/library/lists/featured', () => HttpResponse.json({ items: featured })),
      http.post('*/api/v1/admin/library/lists/:id/feature', async ({ request, params }) => {
        const body = (await readJson(request)) as { featured: boolean };
        calls.push([params.id, body.featured]);
        if (body.featured)
          featured = [
            ...featured,
            { id: '32', name: 'Slow burns', owner_name: 'Ravi', item_count: 2 },
          ];
        else featured = featured.filter((l) => l.id !== params.id);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('library-catalog');
    const user = userEvent.setup();
    expect(await screen.findByText('Quiet mysteries')).toBeInTheDocument();
    expect(screen.getByText('By Mina · 4 series')).toBeInTheDocument();
    await user.type(screen.getByLabelText('List id'), '32');
    await user.click(screen.getByRole('button', { name: 'Feature list' }));
    expect(await screen.findByText('Slow burns')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Stop featuring Quiet mysteries' }));
    await waitFor(() => expect(screen.queryByText('Quiet mysteries')).toBeNull());
    expect(calls).toEqual([
      ['32', true],
      ['31', false],
    ]);
  });
});
