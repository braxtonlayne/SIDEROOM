import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const source = {
  id: '9',
  label: 'Storage Box',
  kind: 'watched_folder',
  backend: 'local',
  location: '/mnt/box/ingest',
  owner_id: '1',
  enabled: true,
  poll_secs: 300,
  stable_secs: 60,
  last_scan_at_ms: null,
  last_scan: null,
  last_error: null,
};

describe('ingest sources', () => {
  beforeEach(() => asStaff({ perms: ['STAFF_BASIC', 'PUBLISH_READING'] }));

  it('lists sources, shows a dry run, and creates a source', async () => {
    const created: unknown[] = [];
    server.use(
      http.get('*/api/v1/admin/ingest-sources', () => HttpResponse.json({ sources: [source] })),
      http.post('*/api/v1/admin/ingest-sources/9/dry-run', () =>
        HttpResponse.json({
          dry_run: true,
          files_seen: 3,
          series: [
            {
              title: 'Blue Hour',
              existing_series: '7',
              chapters: [
                {
                  rel_path: 'Blue Hour/c001.cbz',
                  size_bytes: 10,
                  chapter: 1,
                  volume: null,
                  title: null,
                  status: 'new',
                  note: null,
                },
              ],
            },
          ],
          unplaced: [
            {
              rel_path: 'stray.cbz',
              size_bytes: 1,
              chapter: null,
              volume: null,
              title: null,
              status: 'skipped',
              note: 'no series title in the name',
            },
          ],
          imported: 0,
          waiting: 3,
          failed: 0,
          submissions: [],
          problems: [],
        }),
      ),
      http.post('*/api/v1/admin/ingest-sources', async ({ request }) => {
        created.push(await readJson(request));
        return HttpResponse.json({ ...source, id: '10', label: 'Second' }, { status: 201 });
      }),
    );
    const user = userEvent.setup();
    openAdmin('ingest-sources');
    expect(await screen.findByText('/mnt/box/ingest')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Dry run: Storage Box' }));
    expect(await screen.findByText('Blue Hour/c001.cbz')).toBeInTheDocument();
    expect(screen.getByText('adds to an existing series')).toBeInTheDocument();
    expect(screen.getByText('Files that could not be placed')).toBeInTheDocument();

    await user.type(screen.getByLabelText('Name'), 'Second');
    await user.type(screen.getByLabelText('Path or prefix'), '/mnt/box/two');
    await user.type(screen.getByLabelText('Owner account id'), '1');
    await user.click(screen.getByRole('button', { name: 'Add source' }));
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toMatchObject({
      label: 'Second',
      location: '/mnt/box/two',
      owner_id: '1',
      kind: 'watched_folder',
      backend: 'local',
    });
  });

  it('is hidden without the publisher permission', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_MEDIA'] });
    openAdmin('ingest-sources');
    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Ingest sources' })).toBeNull(),
    );
  });
});
