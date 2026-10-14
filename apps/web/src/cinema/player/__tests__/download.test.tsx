import { ThemeProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import { DownloadModal } from '../DownloadModal';
import { progressApi, localProgress, setProgressApi } from '../progress';

function mount(open = true) {
  const onClose = vi.fn();
  const view = render(
    <QueryClientProvider client={new QueryClient()}>
      <ThemeProvider section="cinema">
        <DownloadModal
          open={open}
          onClose={onClose}
          titleId="t1"
          episodeId="e2"
          title="Frieren · E2"
        />
      </ThemeProvider>
    </QueryClientProvider>,
  );
  return { onClose, ...view };
}

const options = {
  renditions: [
    { id: 'r1080', label: '1080p', height: 1080, size_bytes: 1_800_000_000, kind: 'remux' },
    {
      id: 'r720',
      label: '720p',
      height: 720,
      size_bytes: 900_000_000,
      kind: 'remux',
      cached: true,
    },
    { id: 'orig', label: 'Original', height: null, size_bytes: 4_500_000_000, kind: 'original' },
  ],
};

describe('download modal (CIN-DL-002)', () => {
  it('lists the renditions with their sizes and passes axe', async () => {
    server.use(
      http.get('*/api/v1/cinema/titles/t1/downloads', ({ request }) => {
        expect(new URL(request.url).searchParams.get('episode')).toBe('e2');
        return HttpResponse.json(options);
      }),
    );
    mount();
    const dialog = await screen.findByRole('dialog', { name: 'Download Frieren · E2' });
    expect(await within(dialog).findByLabelText(/1080p · 1\.8 GB/)).toBeChecked();
    expect(within(dialog).getByLabelText(/720p · 900 MB/)).not.toBeChecked();
    expect(within(dialog).getByLabelText(/Original \(original\) · 4\.5 GB/)).toBeInTheDocument();
    expect(within(dialog).getByText('Ready to download')).toBeInTheDocument();
    expect(await axeViolations(dialog)).toEqual([]);
  });

  it('starts the chosen rendition, follows the prep job and offers the link when ready', async () => {
    let polls = 0;
    let body: unknown;
    server.use(
      http.get('*/api/v1/cinema/titles/t1/downloads', () => HttpResponse.json(options)),
      http.post('*/api/v1/cinema/titles/t1/downloads', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ id: 'j1', state: 'queued', progress: 0 });
      }),
      http.get('*/api/v1/cinema/downloads/j1', () => {
        polls += 1;
        return HttpResponse.json(
          polls < 2
            ? { id: 'j1', state: 'preparing', progress: 0.5 }
            : { id: 'j1', state: 'ready', progress: 1, url: 'https://dl.test/f.mp4?sig=1' },
        );
      }),
    );
    mount();
    const user = userEvent.setup();
    await user.click(await screen.findByLabelText(/720p/));
    await user.click(screen.getByRole('button', { name: 'Prepare download' }));
    expect(await screen.findByText('Waiting in line…')).toBeInTheDocument();
    expect(body).toEqual({ rendition_id: 'r720', episode_id: 'e2' });
    expect(
      await screen.findByText('Preparing your file…', undefined, { timeout: 4000 }),
    ).toBeInTheDocument();
    expect(screen.getByRole('progressbar', { name: 'Download preparation' })).toHaveAttribute(
      'aria-valuenow',
      '50',
    );
    const link = await screen.findByRole('link', { name: 'Download' }, { timeout: 6000 });
    expect(link).toHaveAttribute('href', 'https://dl.test/f.mp4?sig=1');
  }, 15_000);

  it('tells non-Patrons that downloads are for Patrons', async () => {
    server.use(
      http.get('*/api/v1/cinema/titles/t1/downloads', () =>
        HttpResponse.json({ message: 'Patrons only' }, { status: 403 }),
      ),
    );
    mount();
    expect(await screen.findByText(/Downloads are for Patrons/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Prepare download' })).not.toBeInTheDocument();
  });

  it('explains the daily limit', async () => {
    server.use(
      http.get('*/api/v1/cinema/titles/t1/downloads', () => HttpResponse.json(options)),
      http.post('*/api/v1/cinema/titles/t1/downloads', () =>
        HttpResponse.json({ message: 'Too many' }, { status: 429 }),
      ),
    );
    mount();
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Prepare download' }));
    expect(await screen.findByText(/download limit/)).toBeInTheDocument();
  });

  it('reports a failed job', async () => {
    server.use(
      http.get('*/api/v1/cinema/titles/t1/downloads', () => HttpResponse.json(options)),
      http.post('*/api/v1/cinema/titles/t1/downloads', () =>
        HttpResponse.json({ id: 'j2', state: 'failed', progress: 0, error: 'Remux failed.' }),
      ),
    );
    mount();
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Prepare download' }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Remux failed.'));
  });
});

describe('watch progress store', () => {
  afterEach(() => {
    setProgressApi(null);
    localStorage.clear();
  });

  it('PUTs the position to the server', async () => {
    let body: unknown;
    server.use(
      http.put('*/api/v1/cinema/progress/m1', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await progressApi.save({
      mediaId: 'm1',
      titleId: 't1',
      episodeId: 'e2',
      positionS: 61.9,
      durationS: 1500.2,
      completed: false,
    });
    expect(body).toEqual({
      title_id: 't1',
      episode_id: 'e2',
      position_s: 61,
      duration_s: 1500,
      completed: false,
    });
  });

  it('keeps the position locally when the server save fails, and resume reads it back', async () => {
    server.use(
      http.put('*/api/v1/cinema/progress/m1', () => HttpResponse.json({}, { status: 500 })),
      http.get('*/api/v1/cinema/progress/m1', () => HttpResponse.json({}, { status: 404 })),
    );
    await expect(
      progressApi.save({
        mediaId: 'm1',
        titleId: 't1',
        episodeId: null,
        positionS: 700,
        durationS: 6000,
        completed: false,
      }),
    ).rejects.toBeTruthy();
    expect(await progressApi.get('m1')).toMatchObject({ position_s: 700, completed: false });
  });

  it('the local store lists unfinished items, newest first', async () => {
    setProgressApi(localProgress);
    const base = { titleId: 't', episodeId: null, durationS: 100 };
    vi.useFakeTimers({ toFake: ['Date'] });
    try {
      vi.setSystemTime(1000);
      await localProgress.save({ ...base, mediaId: 'a', positionS: 10, completed: false });
      vi.setSystemTime(2000);
      await localProgress.save({ ...base, mediaId: 'b', positionS: 100, completed: true });
      vi.setSystemTime(3000);
      await localProgress.save({ ...base, mediaId: 'c', positionS: 20, completed: false });
    } finally {
      vi.useRealTimers();
    }
    expect(localProgress.recent(10).map((i) => i.mediaId)).toEqual(['c', 'a']);
    expect(await progressApi.get('b')).toMatchObject({ completed: true });
    expect(await progressApi.get('zzz')).toBeNull();
  });
});
