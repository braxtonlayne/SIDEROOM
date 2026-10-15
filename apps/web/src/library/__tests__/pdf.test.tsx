import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { setProgressApi, localProgress } from '../progress';
import {
  __setPdfjsForTests,
  loadPdfjs,
  pdfjsInstalled,
  PdfNotInstalledError,
  renderPdfCover,
  type PdfjsLike,
} from '../pdf/pdfjs';
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
  it('reports pdf.js as not installed in this tree and rejects with a typed error', async () => {
    expect(pdfjsInstalled()).toBe(false);
    await expect(loadPdfjs()).rejects.toBeInstanceOf(PdfNotInstalledError);
    await expect(renderPdfCover(new Blob(['%PDF']))).rejects.toBeInstanceOf(PdfNotInstalledError);
  });

  it('shows a clear not-installed state when a PDF chapter is opened', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', () => HttpResponse.json(pdfManifest())),
    );
    renderAt('/library/r/c1');
    expect(await screen.findByText('PDF reading is not installed')).toBeInTheDocument();
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

  it('renders page 1 as a JPEG cover in the browser', async () => {
    __setPdfjsForTests(() => Promise.resolve(fakePdfjs(2)));
    const blob = await renderPdfCover(new Blob(['%PDF-1.7']), 600);
    expect(blob.type).toBe('image/jpeg');
    expect(rendered).toEqual([1]);
  });
});
