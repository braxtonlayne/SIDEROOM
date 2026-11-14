// Wave 68 staff tools: the review queue names people (D-692, SEC-AUD-213) and the transcoding workers show their
// GPU and throughput (D-705, SEC-AUD-216).
import { switchStudySurfacesOn } from '../../study/testing';
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { detail, preview, submission } from '../../review/__tests__/fixtures';
import { asStaff, http, HttpResponse, openAdmin, server } from './helpers';

const maya = { id: '77', username: 'maya', global_name: 'Maya L', avatar: null };
const sam = { id: '90', username: 'sam', global_name: null, avatar: null };

describe('review queue names people', () => {
  beforeEach(() => {
    switchStudySurfacesOn();
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_READING'] });
  });

  it('shows @username on a row, the id when the account is gone, and the claimer by name', async () => {
    server.use(
      http.get('*/api/v1/admin/review/queue', () =>
        HttpResponse.json({
          items: [
            submission('1', {
              title: 'Named row',
              submitter_id: '77',
              state: 'in_review',
              claimed_by: '90',
              claim_expires_at_ms: Date.now() + 20 * 60_000,
            }),
            submission('2', { title: 'Gone row', submitter_id: '88' }),
            submission('3', { title: 'Nobody row', submitter_id: null }),
          ],
          total: 3,
          limit: 25,
          offset: 0,
          users: [maya, sam],
        }),
      ),
    );
    openAdmin('review');
    const table = await screen.findByRole('table', { name: 'Review queue' });
    const named = within(table).getByRole('button', {
      name: /^@maya\. Show only this person.s submissions$/,
    });
    expect(named).toHaveTextContent('@maya');
    // The display name rides in the title.
    expect(named).toHaveAttribute('title', expect.stringContaining('Maya L'));
    // The claimer is named too.
    expect(within(table).getByText(/@sam until/)).toBeInTheDocument();
    // An account the answer does not name keeps its id; a deleted one says so.
    expect(
      within(table).getByRole('button', { name: /^88\. Show only this person.s submissions$/ }),
    ).toHaveTextContent('88');
    expect(within(table).getByText('Deleted account')).toBeInTheDocument();
  });

  it('names the filter chip, even when no row matches, and keeps the id in the address', async () => {
    server.use(
      http.get('*/api/v1/admin/review/queue', () =>
        HttpResponse.json({ items: [], total: 0, limit: 25, offset: 0, users: [maya] }),
      ),
    );
    const { router } = openAdmin('review?submitter=77');
    expect(
      await screen.findByRole('button', { name: /^Submitted by @maya\./ }),
    ).toBeInTheDocument();
    expect(router.state.location.searchStr).toContain('submitter=77');
    expect(screen.getByText('Nothing to review')).toBeInTheDocument();
  });

  it('falls back to the account number on the chip when the answer does not name it', async () => {
    server.use(
      http.get('*/api/v1/admin/review/queue', () =>
        HttpResponse.json({ items: [], total: 0, limit: 25, offset: 0, users: [] }),
      ),
    );
    openAdmin('review?submitter=55');
    expect(
      await screen.findByRole('button', { name: /Submitted by Account 55/ }),
    ).toBeInTheDocument();
  });

  it('names the submitter and the claimer on the detail, the id beside the name', async () => {
    const d = detail({
      id: '7',
      title: 'Linked series',
      submitter_id: '77',
      state: 'in_review',
      claimed_by: '90',
      claim_expires_at_ms: Date.now() + 20 * 60_000,
    });
    server.use(
      http.get('*/api/v1/admin/review/submissions/:id', () =>
        HttpResponse.json({ ...d, users: [maya, sam] }),
      ),
      http.get('*/api/v1/admin/review/submissions/:id/preview', () =>
        HttpResponse.json({ preview: preview() }),
      ),
      http.get('*/api/v1/admin/review/submissions/:id/duplicates', () =>
        HttpResponse.json({ matches: [], resolution: null }),
      ),
      http.get('*/api/v1/admin/review/submissions/:id/media', () =>
        HttpResponse.json({ items: [] }),
      ),
    );
    openAdmin('review?submission=7');
    const details = await screen.findByLabelText('Submission details');
    expect(details).toHaveTextContent('@maya (Maya L) 77');
    expect(details).toHaveTextContent('@sam until');
    // Held by someone else: the notice names them.
    expect(await screen.findByText(/@sam is reviewing this until/)).toBeInTheDocument();
  });
});

const worker = (over: Record<string, unknown> = {}) => ({
  id: 'w1',
  name: 'gpu-box',
  version: '1.2',
  token_id: 't1',
  token_name: 'gpu-box',
  token_revoked: false,
  capabilities: { nvenc: true, hwaccels: ['cuda'], concurrency: 2, gpu: 'NVIDIA GeForce RTX 3060' },
  last_seen_ms: 1_700_000_000_000,
  online: true,
  drain: false,
  active_jobs: 1,
  speed_now: 2.43,
  jobs_done_24h: 14,
  jobs_failed_24h: 1,
  ...over,
});

describe('transcoding workers show their GPU and throughput', () => {
  beforeEach(() => asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TRANSCODE'] }));

  async function openWorkers(list: unknown[]) {
    server.use(
      http.get('*/api/v1/admin/transcode/jobs', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/transcode/queue', () =>
        HttpResponse.json({ queued: 0, running: 0, failed_last_day: 0, succeeded_last_day: 0 }),
      ),
      http.get('*/api/v1/admin/transcode/workers', () => HttpResponse.json(list)),
    );
    const user = userEvent.setup();
    openAdmin('cinema-transcoding');
    await user.click(await screen.findByRole('tab', { name: 'Workers' }));
  }

  it('a GPU worker that is running: its name, the speed now and the day', async () => {
    await openWorkers([worker()]);
    const gear = await screen.findByTestId('worker-gear-w1');
    expect(gear).toHaveTextContent('NVIDIA GeForce RTX 3060');
    expect(gear).toHaveTextContent('2.4x now');
    expect(gear).toHaveTextContent('14 done, 1 failed in 24 h');
  });

  it('a worker without a GPU name shows its hardware encoders; an idle one shows no speed', async () => {
    await openWorkers([
      worker({
        capabilities: { nvenc: true, hwaccels: ['cuda', 'vaapi'], concurrency: 1 },
        speed_now: null,
        jobs_done_24h: 0,
        jobs_failed_24h: 0,
      }),
    ]);
    const gear = await screen.findByTestId('worker-gear-w1');
    expect(gear).toHaveTextContent('NVENC, cuda, vaapi');
    expect(gear).not.toHaveTextContent('now');
    expect(gear).toHaveTextContent('0 done, 0 failed in 24 h');
  });

  it('a CPU worker says it has no hardware encoder, and an older server shows no counts', async () => {
    await openWorkers([
      worker({
        id: 'w2',
        capabilities: { nvenc: false, concurrency: 1 },
        speed_now: undefined,
        jobs_done_24h: undefined,
        jobs_failed_24h: undefined,
        online: false,
      }),
    ]);
    const gear = await screen.findByTestId('worker-gear-w2');
    expect(gear).toHaveTextContent('no hardware encoder');
    expect(gear).not.toHaveTextContent('done');
  });
});
