import { EmptyState, Icon, Spinner } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { openPdf, PdfNotInstalledError, renderPdfPage, type PdfDocumentLike } from './pdfjs';

type State =
  | { kind: 'loading' }
  | { kind: 'missing' }
  | { kind: 'failed' }
  | { kind: 'ready'; doc: PdfDocumentLike };

/**
 * Reads a PDF chapter with pdf.js in the browser (G-59): `source_url` is the signed link the page manifest
 * returns for PDF chapters (docs/specs/library-reader.md section 6.4). One page at a time, fit to width, with
 * previous and next. Shows a plain "not installed" state while `pdfjs-dist` is absent from the build.
 */
export function PdfReader({
  url,
  title,
  seriesId,
  initialPage = 1,
}: {
  url: string;
  title: string;
  seriesId: string;
  initialPage?: number;
}) {
  const t = useT();
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [page, setPage] = useState(Math.max(1, initialPage));
  const [busy, setBusy] = useState(false);
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    let live = true;
    let opened: PdfDocumentLike | undefined;
    openPdf({ url })
      .then((doc) => {
        opened = doc;
        if (live) setState({ kind: 'ready', doc });
        else void doc.destroy?.();
      })
      .catch((e: unknown) => {
        if (live) setState({ kind: e instanceof PdfNotInstalledError ? 'missing' : 'failed' });
      });
    return () => {
      live = false;
      void opened?.destroy?.();
    };
  }, [url]);

  const count = state.kind === 'ready' ? state.doc.numPages : 0;
  const current = count ? Math.min(page, count) : page;
  useEffect(() => {
    if (state.kind !== 'ready' || !canvas.current) return;
    let live = true;
    setBusy(true);
    const width = Math.min(1100, Math.max(320, canvas.current.parentElement?.clientWidth ?? 800));
    renderPdfPage(state.doc, current, canvas.current, width)
      .catch(() => live && setState({ kind: 'failed' }))
      .finally(() => live && setBusy(false));
    return () => {
      live = false;
    };
  }, [state, current]);

  const back = (
    <Link to="/library/s/$seriesId" params={{ seriesId }} className="lib-btn">
      {t('library.pdf.backToSeries')}
    </Link>
  );
  if (state.kind === 'missing' || state.kind === 'failed')
    return (
      <div className="lib-reader lib-reader-msg" data-bg="black">
        <EmptyState
          title={
            state.kind === 'missing' ? t('library.pdf.missingTitle') : t('library.pdf.failedTitle')
          }
          description={
            state.kind === 'missing'
              ? t('library.pdf.missingDescription')
              : t('library.pdf.failedDescription')
          }
          action={back}
        />
      </div>
    );
  if (state.kind === 'loading')
    return (
      <div className="lib-reader lib-reader-msg" data-bg="black">
        <Spinner label={t('library.reader.opening')} />
      </div>
    );
  return (
    <div className="lib-reader lib-pdf" data-bg="black">
      <header className="lib-pdf-bar">
        {back}
        <h1 className="lib-pdf-title">{title}</h1>
        <div className="lib-pdf-nav">
          <button
            type="button"
            className="lib-btn"
            aria-label={t('library.pdf.previous')}
            disabled={current <= 1}
            onClick={() => setPage(current - 1)}
          >
            <Icon name="arrow-left" size={16} />
          </button>
          <span role="status">{t('library.pdf.pageOf', { page: current, count })}</span>
          <button
            type="button"
            className="lib-btn"
            aria-label={t('library.pdf.next')}
            disabled={current >= count}
            onClick={() => setPage(current + 1)}
          >
            <Icon name="arrow-right" size={16} />
          </button>
        </div>
      </header>
      <div className="lib-pdf-page" aria-busy={busy}>
        <canvas ref={canvas} aria-label={t('library.pdf.pageLabel', { page: current })} />
      </div>
    </div>
  );
}
