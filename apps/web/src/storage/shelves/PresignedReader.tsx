import { Notice, Spinner } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { EpubReader } from '../../library/epub/EpubReader';
import { PdfReader } from '../../library/pdf/PdfReader';
import type { Presigned } from '../api';
import { presignShelfFile } from './api';

/** EPUB and PDF from a mount: one presigned fetch, then the library's own foliate-js and pdf.js readers. */
export function PresignedReader({
  mountId,
  objectKey,
  kind,
  title,
  seriesId,
  presign = presignShelfFile,
}: {
  mountId: string;
  objectKey: string;
  kind: 'epub' | 'pdf';
  title: string;
  seriesId: string;
  /** Where the short-lived link comes from (the Study passes its per-object URL mint). */
  presign?: (mountId: string, key: string) => Promise<Presigned>;
}) {
  const t = useT();
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let alive = true;
    presign(mountId, objectKey)
      .then((p) => alive && setUrl(p.url))
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
    };
  }, [mountId, objectKey, presign]);
  if (failed) return <Notice variant="danger">{t('shelves.reader.failed')}</Notice>;
  if (!url) return <Spinner label={t('shelves.reader.loading')} />;
  // No `chapter`: a shelf book has no row in the instance's progress tables, so nothing is saved.
  return kind === 'epub' ? (
    <EpubReader url={url} title={title} seriesId={seriesId} />
  ) : (
    <PdfReader url={url} title={title} seriesId={seriesId} />
  );
}
