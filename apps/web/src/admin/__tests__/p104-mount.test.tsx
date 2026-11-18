// D-991 / D-999: the Transcoding queue's Mount cell shows the mount's label, then its id. The label comes from the job
// row itself (`mount_label`); the page makes no request to the Storage mounts list.
import { screen, within } from '@testing-library/react';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const job = (over: Record<string, unknown> = {}) => ({
  job_id: '31',
  asset_id: '0',
  mount_id: '5550001',
  mount_label: 'Ada films',
  kind: 'mount_transcode',
  state: 'queued',
  priority: 4,
  attempt: 0,
  cancel_requested: false,
  created_at_ms: 1_700_000_000_000,
  ...over,
});

async function openQueue(jobs: unknown[]) {
  asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TRANSCODE'] });
  const mountsAsked = vi.fn();
  server.use(
    http.get('*/api/v1/admin/transcode/jobs', () => HttpResponse.json(jobs)),
    http.get('*/api/v1/admin/transcode/queue', () => new HttpResponse(null, { status: 404 })),
    http.get('*/api/v1/admin/storage/mounts', () => {
      mountsAsked();
      return HttpResponse.json([]);
    }),
  );
  openAdmin('cinema-transcoding');
  const table = await screen.findByRole('table', { name: /jobs/i });
  return { table, mountsAsked };
}
const mountCell = (table: HTMLElement) =>
  table.querySelector<HTMLElement>('tbody tr td[data-label="Mount"]')!;

describe('Transcoding queue: the mount label (D-999)', () => {
  it('shows the label from the job and then the id, and asks the mounts list nothing', async () => {
    const { table, mountsAsked } = await openQueue([job()]);
    expect(mountCell(table).textContent).toBe('Ada films 5550001');
    expect(within(mountCell(table)).getByText('5550001')).toHaveClass('adm-mono');
    await new Promise((r) => setTimeout(r, 50));
    expect(mountsAsked).not.toHaveBeenCalled();
  });

  it('shows the label as text, never as HTML', async () => {
    const { table } = await openQueue([job({ mount_label: '<b>bold</b>' })]);
    expect(mountCell(table).textContent).toBe('<b>bold</b> 5550001');
    expect(mountCell(table).querySelector('b')).toBeNull();
  });

  it('shows the id alone when the job carries no label (the mount is gone), with no alert', async () => {
    const { table } = await openQueue([job({ mount_label: undefined })]);
    expect(mountCell(table).textContent).toBe('5550001');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('leaves the cell empty for a job with no mount', async () => {
    const { table } = await openQueue([
      job({ mount_id: undefined, mount_label: undefined, kind: 'proxy' }),
    ]);
    expect(mountCell(table).textContent).toBe('');
  });
});
