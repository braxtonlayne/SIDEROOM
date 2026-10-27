import { Button, Notice, Spinner } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { openCbz, type CbzBook, type PageLister } from '../../library/reader/cbzRange';
import { presignShelfFile } from './api';
import './shelves.css';

let lister: Promise<PageLister> | null = null;
/** The page lister of `sr-library-wasm`, loaded on first use. */
async function wasmLister(): Promise<PageLister> {
  lister ??= import('@sideroom/wasm').then(async (m) => {
    const api = await m.loadLibrary();
    return { firstTail: api.firstTail, listPages: api.listPages };
  });
  return lister;
}

/**
 * A simple page-by-page reader for a CBZ in a mount (BYOS-CAT-002). The file is read by range requests straight
 * from the bucket; the next page is fetched ahead. A link that expires mid-book is replaced and the reader
 * stays on the same page.
 */
export function CbzReader({
  mountId,
  objectKey,
  title,
  lister: injected,
}: {
  mountId: string;
  objectKey: string;
  title: string;
  /** Tests inject the WebAssembly module's stand-in. */
  lister?: PageLister;
}) {
  const t = useT();
  const [book, setBook] = useState<CbzBook | null>(null);
  const [index, setIndex] = useState(0);
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const urls = useRef<string[]>([]);
  const reopening = useRef(false);

  const open = useCallback(async () => {
    const [signed, api] = await Promise.all([
      presignShelfFile(mountId, objectKey),
      injected ?? wasmLister(),
    ]);
    return openCbz(signed.url, api);
  }, [mountId, objectKey, injected]);

  useEffect(() => {
    let alive = true;
    open()
      .then((b) => alive && setBook(b))
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
    };
  }, [open]);

  useEffect(() => {
    if (!book) return;
    let alive = true;
    book
      .page(index)
      .then((blob) => {
        if (!alive) return;
        const u = URL.createObjectURL(blob);
        urls.current.push(u);
        setSrc(u);
        if (index + 1 < book.pages.length) void book.page(index + 1).catch(() => undefined);
      })
      .catch(async () => {
        if (!alive || reopening.current) return setFailed(true);
        // Probably an expired link: open the book again once and stay on this page.
        reopening.current = true;
        try {
          const b = await open();
          if (alive) setBook(b);
        } catch {
          if (alive) setFailed(true);
        } finally {
          reopening.current = false;
        }
      });
    return () => {
      alive = false;
    };
  }, [book, index, open]);

  useEffect(
    () => () => {
      urls.current.forEach((u) => URL.revokeObjectURL(u));
      urls.current = [];
    },
    [],
  );

  if (failed) return <Notice variant="danger">{t('shelves.reader.failed')}</Notice>;
  if (!book) return <Spinner label={t('shelves.reader.loading')} />;
  const last = book.pages.length - 1;
  return (
    <section className="shv-reader" aria-label={title}>
      {src ? (
        <img
          className="shv-page-img"
          src={src}
          alt={t('shelves.reader.page', { n: index + 1, total: book.pages.length })}
        />
      ) : (
        <Spinner label={t('shelves.reader.loading')} />
      )}
      <div className="shv-actions">
        <Button variant="secondary" disabled={index <= 0} onClick={() => setIndex(index - 1)}>
          {t('shelves.reader.prev')}
        </Button>
        <span aria-live="polite">
          {t('shelves.reader.page', { n: index + 1, total: book.pages.length })}
        </span>
        <Button variant="secondary" disabled={index >= last} onClick={() => setIndex(index + 1)}>
          {t('shelves.reader.next')}
        </Button>
      </div>
    </section>
  );
}
