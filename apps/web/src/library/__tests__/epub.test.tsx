// EPUB reader (LIB-RDR-010). foliate-js itself is replaced by a small stand-in custom element; this covers
// our side: opening an EPUB chapter, the contents, settings, resuming from the saved CFI and saving progress.
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { localProgress, setProgressApi } from '../progress';
import { DEFAULT_EPUB, epubCss, normalize, useEpubSettings } from '../epub/settings';
import { chapter, manifest, renderAt, useLibraryApi } from './harness';

interface Fake extends HTMLElement {
  init: ReturnType<typeof vi.fn>;
  styles: string[];
}
const made: Fake[] = [];

vi.mock('foliate-js/view.js', () => {
  class FoliateView extends HTMLElement {
    styles: string[] = [];
    renderer = Object.assign(document.createElement('div'), {
      setStyles: (css: string) => this.styles.push(css),
    });
    book = {
      toc: [
        {
          label: 'Chapter One',
          href: 'c1.xhtml',
          subitems: [{ label: 'Part A', href: 'c1.xhtml#a' }],
        },
      ],
    };
    init = vi.fn(async () => undefined);
    open = vi.fn(async () => undefined);
    goTo = vi.fn(async () => undefined);
    next = vi.fn(async () => undefined);
    prev = vi.fn(async () => undefined);
    constructor() {
      super();
      made.push(this as unknown as Fake);
    }
  }
  if (!customElements.get('foliate-view')) customElements.define('foliate-view', FoliateView);
  return {};
});

const epub = () =>
  manifest(9, {
    chapter: chapter(9, { format: 'epub', title: 'The Book' }),
    pages: [],
    source_url: '/cdn/library/1/c9/book.epub?sig=1',
    next_chapter_id: null,
  });

beforeEach(() => {
  localStorage.clear();
  made.length = 0;
  useEpubSettings.setState({ settings: DEFAULT_EPUB });
  setProgressApi(localProgress);
});

describe('settings', () => {
  it('drops out-of-range values and builds the stylesheet', () => {
    expect(
      normalize({ size: 999, lineHeight: 0, margin: -3, theme: 'neon', flow: 'x', font: 'comic' }),
    ).toEqual({
      ...DEFAULT_EPUB,
      size: 200,
      lineHeight: 1.2,
      margin: 0,
    });
    const css = epubCss({
      ...DEFAULT_EPUB,
      theme: 'dark',
      size: 120,
      font: 'serif',
      lineHeight: 1.8,
    });
    expect(css).toContain('font-size: 120%');
    expect(css).toContain('font-family: serif');
    expect(css).toContain('line-height: 1.8');
    expect(css).toContain('#16161a');
  });
});

describe('EPUB reader', () => {
  it('opens the book, resumes at the saved position, lists the contents and restyles', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', () => HttpResponse.json(epub())),
      http.get('*/api/v1/library/chapters/c9/progress', () =>
        HttpResponse.json({ page_index: 400, percent: 40, completed: false, cfi: 'epubcfi(/6/8)' }),
      ),
      http.get('/cdn/library/1/c9/book.epub', () => new HttpResponse('PK', { status: 200 })),
    );
    setProgressApi({
      ...localProgress,
      get: async () => ({ page_index: 400, percent: 40, completed: false, cfi: 'epubcfi(/6/8)' }),
    });
    const user = userEvent.setup();
    renderAt('/library/r/c9');
    await waitFor(() => expect(made.length).toBe(1));
    const view = made[0]!;
    await waitFor(() =>
      expect(view.init).toHaveBeenCalledWith({
        lastLocation: 'epubcfi(/6/8)',
        showTextStart: false,
      }),
    );
    await waitFor(() => expect(view.styles.length).toBeGreaterThan(0));
    await user.click(await screen.findByRole('button', { name: 'Contents' }));
    expect(await screen.findByRole('button', { name: 'Chapter One' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Part A' }));
    expect((view as unknown as { goTo: ReturnType<typeof vi.fn> }).goTo).toHaveBeenCalledWith(
      'c1.xhtml#a',
    );
    // Settings restyle the open book.
    const before = view.styles.length;
    useEpubSettings.getState().set({ theme: 'sepia', size: 140 });
    await waitFor(() => expect(view.styles.length).toBeGreaterThan(before));
    expect(view.styles.at(-1)).toContain('font-size: 140%');
  });

  it('saves the CFI and the fraction as progress', async () => {
    useLibraryApi();
    const saved: Record<string, unknown>[] = [];
    server.use(
      http.get('*/api/v1/library/chapters/:id/pages', () => HttpResponse.json(epub())),
      http.get('/cdn/library/1/c9/book.epub', () => new HttpResponse('PK', { status: 200 })),
    );
    setProgressApi({
      ...localProgress,
      get: async () => null,
      save: async (p) => void saved.push({ ...p }),
    });
    renderAt('/library/r/c9');
    await waitFor(() => expect(made.length).toBe(1));
    const view = made[0]!;
    await waitFor(() => expect(view.init).toHaveBeenCalled());
    view.dispatchEvent(
      new CustomEvent('relocate', {
        detail: { cfi: 'epubcfi(/6/10)', fraction: 0.25, tocItem: { label: 'Chapter One' } },
      }),
    );
    expect(await screen.findByText(/Chapter One · 25%/)).toBeInTheDocument();
    await waitFor(() => expect(saved.length).toBeGreaterThan(0), { timeout: 4000 });
    expect(saved[0]).toMatchObject({
      cfi: 'epubcfi(/6/10)',
      percent: 25,
      pageIndex: 250,
      pageCount: 1000,
      completed: false,
    });
  });
});
