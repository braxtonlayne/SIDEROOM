// D-991: the Transcoding queue's Mount cell shows the mount's label, then its id.
import { screen, waitFor, within } from '@testing-library/react';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const job = (over: Record<string, unknown> = {}) => ({
  job_id: '31',
  asset_id: '0',
  mount_id: '5550001',
  kind: 'mount_transcode',
  state: 'queued',
  priority: 4,
  attempt: 0,
  cancel_requested: false,
  created_at_ms: 1_700_000_000_000,
  ...over,
});
const mount = (over: Record<string, unknown> = {}) => ({
  id: '5550001',
  owner_kind: 'user',
  owner_id: '7',
  owner_name: 'Ada',
  label: 'Ada films',
  provider: 's3',
  bucket: 'ada-films',
  status: 'active',
  key_hint: 'AKIA',
  created_at_ms: 1_700_000_000_000,
  delisted_at_ms: null,
  delist_reason: '',
  delisted_objects: 0,
  ...over,
});

async function openQueue(jobs: unknown[], mounts: () => Response | Promise<Response>) {
  asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TRANSCODE'] });
  server.use(
    http.get('*/api/v1/admin/transcode/jobs', () => HttpResponse.json(jobs)),
    http.get('*/api/v1/admin/transcode/queue', () => new HttpResponse(null, { status: 404 })),
    http.get('*/api/v1/admin/storage/mounts', () => mounts()),
  );
  openAdmin('cinema-transcoding');
  const table = await screen.findByRole('table', { name: /jobs/i });
  return table;
}
const mountCell = (table: HTMLElement) =>
  table.querySelector<HTMLElement>('tbody tr td[data-label="Mount"]')!;

describe('Transcoding queue: the mount label (D-991)', () => {
  it('shows the label and then the id', async () => {
    const table = await openQueue([job()], () => HttpResponse.json([mount()]));
    await waitFor(() =>
      expect(within(mountCell(table)).getByText('Ada films')).toBeInTheDocument(),
    );
    expect(mountCell(table).textContent).toBe('Ada films 5550001');
    expect(within(mountCell(table)).getByText('5550001')).toHaveClass('adm-mono');
  });

  it('shows the label as text, never as HTML', async () => {
    const table = await openQueue([job()], () =>
      HttpResponse.json([mount({ label: '<b>bold</b>' })]),
    );
    await waitFor(() => expect(mountCell(table).textContent).toBe('<b>bold</b> 5550001'));
    expect(mountCell(table).querySelector('b')).toBeNull();
  });

  it('shows the id alone when the list fails, with no alert', async () => {
    const table = await openQueue([job()], () => new HttpResponse(null, { status: 500 }));
    expect(within(table).getByText('5550001')).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 50));
    expect(mountCell(table).textContent).toBe('5550001');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('shows the id alone for a mount the list does not hold', async () => {
    const table = await openQueue([job()], () => HttpResponse.json([mount({ id: '9' })]));
    await new Promise((r) => setTimeout(r, 50));
    expect(mountCell(table).textContent).toBe('5550001');
  });

  it('leaves the cell empty for a job with no mount', async () => {
    const table = await openQueue([job({ mount_id: null, kind: 'proxy' })], () =>
      HttpResponse.json([mount()]),
    );
    await new Promise((r) => setTimeout(r, 50));
    expect(mountCell(table).textContent).toBe('');
  });
});
