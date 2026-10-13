import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../test/server';
import { useReaderPrefsStore } from '../prefs';
import { localProgress, setProgressApi, type ProgressApi } from '../progress';
import { manifest, pages, renderAt, useLibraryApi } from './harness';
import type { PageManifest } from '../types';

function serveChapter(m: PageManifest) {
  server.use(http.get('*/api/v1/library/chapters/:id/pages', () => HttpResponse.json(m)));
}

const imgs = () => screen.getAllByRole('img').filter((i) => i.tagName === 'IMG');
const shown = () =>
  document.querySelector('.lib-spread')?.querySelectorAll('img.lib-page-img') ?? [];

beforeEach(() => {
  localStorage.clear();
  useReaderPrefsStore.setState({ global: {}, series: {} });
  setProgressApi(localProgress);
});

describe('reader', () => {
  it('opens at page 1, turns pages by direction (RTL default), keeps the URL in step, passes axe', async () => {
    useLibraryApi();
    serveChapter(manifest(1));
    const user = userEvent.setup();
    const { router, container } = renderAt('/library/r/c1');
    expect(await screen.findByTestId('reader-status')).toHaveTextContent('Page 1 of 3');
    expect(imgs().find((i) => i.getAttribute('alt') === 'Page 1')).toBeTruthy();
    await user.keyboard('{ArrowLeft}'); // RTL: left goes forward
    await waitFor(() =>
      expect(screen.getByTestId('reader-status')).toHaveTextContent('Page 2 of 3'),
    );
    await waitFor(() => expect(router.state.location.pathname).toBe('/library/r/c1/2'));
    await user.keyboard('{ArrowRight}');
    await waitFor(() =>
      expect(screen.getByTestId('reader-status')).toHaveTextContent('Page 1 of 3'),
    );
    await user.keyboard('{End}');
    await waitFor(() =>
      expect(screen.getByTestId('reader-status')).toHaveTextContent('Page 3 of 3'),
    );
    await user.keyboard('{Home}');
    await waitFor(() =>
      expect(screen.getByTestId('reader-status')).toHaveTextContent('Page 1 of 3'),
    );
    expect(await axeViolations(container)).toEqual([]);
  });

  it('follows LTR series direction and the end card offers the next chapter', async () => {
    useLibraryApi();
    serveChapter(manifest(1, { reading_direction: 'ltr' }));
    const user = userEvent.setup();
    const { router } = renderAt('/library/r/c1');
    await screen.findByTestId('reader-status');
    await user.keyboard('{ArrowRight}{ArrowRight}{ArrowRight}');
    expect(await screen.findByRole('region', { name: 'End of chapter' })).toBeInTheDocument();
    await user.click(
      within(screen.getByRole('region', { name: 'End of chapter' })).getByRole('button', {
        name: /Next chapter/,
      }),
    );
    await waitFor(() => expect(router.state.location.pathname).toBe('/library/r/c2'));
  });

  it('opens at the page in the URL, else at the saved page', async () => {
    useLibraryApi();
    serveChapter(manifest(1));
    const first = renderAt('/library/r/c1/3');
    expect(await screen.findByTestId('reader-status')).toHaveTextContent('Page 3 of 3');
    first.unmount();

    await localProgress.save({
      chapterId: 'c1',
      seriesId: '1',
      seriesTitle: 'Frieren',
      chapterNumber: 1,
      pageCount: 3,
      pageIndex: 1,
      percent: 67,
      completed: false,
    });
    renderAt('/library/r/c1');
    await waitFor(() =>
      expect(screen.getByTestId('reader-status')).toHaveTextContent('Page 2 of 3'),
    );
  });

  it('saves progress through the data layer after a page turn (debounced) and on leaving', async () => {
    useLibraryApi();
    serveChapter(manifest(1));
    const save = vi.fn().mockResolvedValue(undefined);
    const fake: ProgressApi = {
      save,
      recent: () => Promise.resolve([]),
      forSeries: () => Promise.resolve({}),
      get: () => Promise.resolve(null),
      bookmarks: () => Promise.resolve([]),
      addBookmark: vi.fn(),
      removeBookmark: vi.fn(),
    };
    setProgressApi(fake);
    const user = userEvent.setup();
    const view = renderAt('/library/r/c1');
    await screen.findByTestId('reader-status');
    expect(save).not.toHaveBeenCalled(); // opening alone saves nothing
    await user.keyboard('{ArrowLeft}');
    await waitFor(() =>
      expect(screen.getByTestId('reader-status')).toHaveTextContent('Page 2 of 3'),
    );
    expect(save).not.toHaveBeenCalled();
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1), { timeout: 4000 });
    expect(save).toHaveBeenLastCalledWith(
      expect.objectContaining({
        chapterId: 'c1',
        seriesId: '1',
        pageIndex: 1,
        pageCount: 3,
        percent: 67,
        completed: false,
      }),
    );
    await user.keyboard('{End}');
    await waitFor(() =>
      expect(screen.getByTestId('reader-status')).toHaveTextContent('Page 3 of 3'),
    );
    view.unmount(); // flushes at once, without waiting for the debounce
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2));
    expect(save).toHaveBeenLastCalledWith(
      expect.objectContaining({ pageIndex: 2, percent: 100, completed: true }),
    );
  });

  it('long strip mode (vertical series) renders every page', async () => {
    useLibraryApi();
    serveChapter(manifest(1, { reading_direction: 'vertical', pages: pages(3) }));
    renderAt('/library/r/c1');
    await screen.findByTestId('reader-status');
    await waitFor(() => expect(document.querySelectorAll('.lib-strip-item')).toHaveLength(3));
  });

  it('D toggles double-page mode: the cover stays alone, then pairs', async () => {
    useLibraryApi();
    serveChapter(manifest(1, { pages: pages(5) }));
    const user = userEvent.setup();
    renderAt('/library/r/c1');
    await screen.findByTestId('reader-status');
    await user.keyboard('d');
    await waitFor(() => expect(shown()).toHaveLength(1)); // cover alone
    await user.keyboard('{ArrowLeft}');
    await waitFor(() => expect(shown()).toHaveLength(2)); // pages 2 and 3
  });

  it('bookmarks the current page with B and lists it in the panel', async () => {
    useLibraryApi();
    serveChapter(manifest(1));
    const user = userEvent.setup();
    renderAt('/library/r/c1');
    await screen.findByTestId('reader-status');
    await user.keyboard('b');
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: 'Remove bookmark from this page' }),
      ).toBeInTheDocument(),
    );
    await user.click(screen.getByRole('button', { name: 'Bookmarks' }));
    expect(await screen.findByRole('button', { name: 'Page 1' })).toBeInTheDocument();
  });

  it('reader settings change the mode and keep a per-series override', async () => {
    useLibraryApi();
    serveChapter(manifest(1));
    const user = userEvent.setup();
    renderAt('/library/r/c1');
    await screen.findByTestId('reader-status');
    await user.keyboard('m');
    await user.click(await screen.findByRole('radio', { name: 'Long strip' }));
    expect(useReaderPrefsStore.getState().series['1']).toMatchObject({ mode: 'strip' });
    expect(useReaderPrefsStore.getState().global).toEqual({});
    await waitFor(() => expect(document.querySelectorAll('.lib-strip-item')).toHaveLength(3));
  });

  it('shows a message for a chapter that is gone', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', () =>
        HttpResponse.json({ message: 'Not found' }, { status: 404 }),
      ),
    );
    renderAt('/library/r/c9');
    expect(await screen.findByText('This chapter is not available')).toBeInTheDocument();
  });
});

describe('age gate in the reader (D-184)', () => {
  const gate = (m: PageManifest) => {
    const calls = { manifest: 0, ageGate: [] as unknown[] };
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', ({ params }) => {
        if (params.id === 'c1') calls.manifest++;
        return calls.manifest === 1 || calls.ageGate.length === 0
          ? HttpResponse.json(
              { code: 900011, message: 'You need to confirm you are 18 or older to open this' },
              { status: 403 },
            )
          : HttpResponse.json(m);
      }),
      http.post('*/api/v1/sr/age-gate', async ({ request }) => {
        calls.ageGate.push(await request.json());
        return new HttpResponse(null, { status: 204 });
      }),
    );
    return calls;
  };

  it('asks once on 403 900011, posts the yes and refetches the manifest once', async () => {
    useLibraryApi();
    const calls = gate(manifest(1));
    const user = userEvent.setup();
    renderAt('/library/r/c1');
    expect(await screen.findByRole('dialog', { name: 'Are you 18 or older?' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Yes' }));
    expect(await screen.findByTestId('reader-status')).toHaveTextContent('Page 1 of 3');
    expect(calls.ageGate).toEqual([{ over_18: true }]);
    expect(calls.manifest).toBe(2);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('No leaves the reader closed and goes back to the library', async () => {
    useLibraryApi();
    const calls = gate(manifest(1));
    const user = userEvent.setup();
    const { router } = renderAt('/library/r/c1');
    await user.click(await screen.findByRole('button', { name: 'No' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/library'));
    expect(calls.ageGate).toEqual([{ over_18: false }]);
    expect(calls.manifest).toBe(1);
    expect(document.querySelector('.lib-reader')).toBeNull();
  });

  it('dismissing the dialog also leaves the reader closed', async () => {
    useLibraryApi();
    gate(manifest(1));
    const user = userEvent.setup();
    const { router } = renderAt('/library/r/c1');
    await screen.findByRole('dialog', { name: 'Are you 18 or older?' });
    await user.keyboard('{Escape}');
    await waitFor(() => expect(router.state.location.pathname).toBe('/library'));
  });

  it('keeps the dialog open with an error when saving the answer fails', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', () =>
        HttpResponse.json({ code: 900011, message: 'x' }, { status: 403 }),
      ),
      http.post('*/api/v1/sr/age-gate', () =>
        HttpResponse.json({ message: 'boom' }, { status: 500 }),
      ),
    );
    const user = userEvent.setup();
    renderAt('/library/r/c1');
    await user.click(await screen.findByRole('button', { name: 'Yes' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/could not save/);
    expect(screen.getByRole('dialog', { name: 'Are you 18 or older?' })).toBeInTheDocument();
  });
});
