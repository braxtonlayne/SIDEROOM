import { Notice, Spinner } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { EpubReader } from '../../library/epub/EpubReader';
import { PdfReader } from '../../library/pdf/PdfReader';
import { presignShelfFile } from './api';

/** EPUB and PDF from a mount: one presigned fetch, then the library's own foliate-js and pdf.js readers. */
export function PresignedReader({
  mountId,
  objectKey,
  kind,
  title,
  seriesId,
}: {
  mountId: string;
  objectKey: string;
  kind: 'epub' | 'pdf';
  title: string;
  seriesId: string;
}) {
  const t = useT();
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let alive = true;
    presignShelfFile(mountId, objectKey)
      .then((p) => alive && setUrl(p.url))
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
    };
  }, [mountId, objectKey]);
  if (failed) return <Notice variant="error">{t('shelves.reader.failed')}</Notice>;
  if (!url) return <Spinner label={t('shelves.reader.loading')} />;
  // No `chapter`: a shelf book has no row in the instance's progress tables, so nothing is saved.
  return kind === 'epub' ? (
    <EpubReader url={url} title={title} seriesId={seriesId} />
  ) : (
    <PdfReader url={url} title={title} seriesId={seriesId} />
  );
}
