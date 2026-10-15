import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { setProgressApi, localProgress } from '../progress';
import { __setPdfjsForTests, pdfjsInstalled, renderPdfCover, type PdfjsLike } from '../pdf/pdfjs';
import { chapter, manifest, renderAt, useLibraryApi } from './harness';

const rendered: number[] = [];
const fakePdfjs = (pages = 3): PdfjsLike => ({
  getDocument: () => ({
    promise: Promise.resolve({
      numPages: pages,
      getPage: (n: number) =>
        Promise.resolve({
          getViewport: ({ scale }: { scale: number }) => ({
            width: 500 * scale,
            height: 700 * scale,
          }),
          render: () => {
            rendered.push(n);
            return { promise: Promise.resolve() };
          },
        }),
      destroy: () => undefined,
    }),
  }),
});

const pdfManifest = () =>
  manifest(1, {
    chapter: chapter(1, { format: 'pdf', page_count: 0, title: 'Volume one' }),
    pages: [],
    source_url: '/cdn/library/1/source.pdf?ex=1',
  });

beforeEach(() => {
  localStorage.clear();
  setProgressApi(localProgress);
  rendered.length = 0;
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(
    {} as unknown as CanvasRenderingContext2D,
  );
  vi.spyOn(HTMLCanvasElement.prototype, 'toBlob').mockImplementation((cb) =>
    cb(new Blob(['jpeg'], { type: 'image/jpeg' })),
  );
});
afterEach(() => {
  __setPdfjsForTests(null);
  vi.restoreAllMocks();
});

describe('G-59 pdf.js integration', () => {
  it('treats pdf.js as part of the build', () => {
    expect(pdfjsInstalled()).toBe(true);
  });

  it('shows the failed state when pdf.js cannot be loaded', async () => {
    __setPdfjsForTests(() => Promise.reject(new Error('chunk load failed')));
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', () => HttpResponse.json(pdfManifest())),
    );
    renderAt('/library/r/c1');
    expect(await screen.findByText('This PDF could not be opened')).toBeInTheDocument();
  });

  it('reads a PDF chapter page by page through pdf.js', async () => {
    __setPdfjsForTests(() => Promise.resolve(fakePdfjs(3)));
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', () => HttpResponse.json(pdfManifest())),
    );
    renderAt('/library/r/c1');
    expect(await screen.findByText('Page 1 of 3')).toBeInTheDocument();
    await waitFor(() => expect(rendered).toContain(1));
    await userEvent.click(screen.getByRole('button', { name: 'Next page' }));
    expect(await screen.findByText('Page 2 of 3')).toBeInTheDocument();
    await waitFor(() => expect(rendered).toContain(2));
    expect(screen.getByRole('button', { name: 'Previous page' })).toBeEnabled();
  });

  it('saves the page index as reading progress and reopens there (G-78)', async () => {
    __setPdfjsForTests(() => Promise.resolve(fakePdfjs(5)));
    const saves: unknown[] = [];
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', () => HttpResponse.json(pdfManifest())),
    );
    const saved = vi.spyOn(localProgress, 'save').mockImplementation((p) => {
      saves.push(p);
      return Promise.resolve();
    });
    renderAt('/library/r/c1');
    expect(await screen.findByText('Page 1 of 5')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Next page' }));
    await userEvent.click(screen.getByRole('button', { name: 'Next page' }));
    expect(await screen.findByText('Page 3 of 5')).toBeInTheDocument();
    await waitFor(() => expect(saves.length).toBeGreaterThan(0), { timeout: 4000 });
    expect(saves.at(-1)).toMatchObject({
      chapterId: 'c1',
      pageIndex: 2,
      pageCount: 5,
      percent: 60,
      completed: false,
    });
    saved.mockRestore();
  });

  it('opens a PDF at the saved page when the address names none', async () => {
    __setPdfjsForTests(() => Promise.resolve(fakePdfjs(5)));
    await localProgress.save({
      chapterId: 'c1',
      seriesId: '1',
      seriesTitle: 'S',
      chapterNumber: 1,
      pageCount: 5,
      pageIndex: 3,
      percent: 75,
      completed: false,
    });
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', () => HttpResponse.json(pdfManifest())),
    );
    renderAt('/library/r/c1');
    expect(await screen.findByText('Page 4 of 5')).toBeInTheDocument();
  });

  it('renders page 1 as a JPEG cover in the browser', async () => {
    __setPdfjsForTests(() => Promise.resolve(fakePdfjs(2)));
    const blob = await renderPdfCover(new Blob(['%PDF-1.7']), 600);
    expect(blob.type).toBe('image/jpeg');
    expect(rendered).toEqual([1]);
  });
});
