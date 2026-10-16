// Lazy loader for foliate-js (LIB-RDR-010). The package is only fetched when an EPUB chapter is opened, so it
// stays out of every first-load bundle. Importing `view.js` registers the `<foliate-view>` element.
let loading: Promise<void> | null = null;

export function loadFoliate(): Promise<void> {
  loading ??= import('foliate-js/view.js').then(
    () => undefined,
    (e: unknown) => {
      loading = null;
      throw e;
    },
  );
  return loading;
}

/** The parts of the `<foliate-view>` element this app uses. */
export interface FoliateView extends HTMLElement {
  open(book: File | Blob): Promise<void>;
  init(opts: { lastLocation?: string | null; showTextStart?: boolean }): Promise<void>;
  goTo(target: string | number): Promise<void>;
  goToFraction(f: number): Promise<void>;
  prev(): Promise<void>;
  next(): Promise<void>;
  close?(): void;
  book?: { toc?: TocItem[]; metadata?: { title?: unknown } };
  renderer: HTMLElement & { setStyles?(css: string): void };
}

export interface TocItem {
  label: string;
  href: string;
  subitems?: TocItem[] | null;
}

export interface RelocateDetail {
  cfi?: string;
  fraction?: number;
  tocItem?: { label?: string } | null;
}
