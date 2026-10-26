import { Button, Notice } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useCallback, useEffect, useState } from 'react';
import { ApiError } from '../api/client';
import { formatDateTime, useT } from '../i18n';
import { errorText } from '../settings/user/api';
import '../settings/user/storage/storage.css';
import { listObjects, presignGet, type ObjectEntry, type ObjectPage, type Presigned } from './api';

const IMAGE = /\.(png|jpe?g|gif|webp|avif|svg)$/i;
const AUDIO = /\.(mp3|flac|ogg|oga|opus|m4a|aac|wav)$/i;
export const previewKind = (key: string): 'image' | 'audio' | null =>
  IMAGE.test(key) ? 'image' : AUDIO.test(key) ? 'audio' : null;

const baseName = (key: string) => key.split('/').filter(Boolean).pop() ?? key;
const sizeText = (n: number) =>
  n < 1024
    ? `${n} B`
    : n < 1024 ** 2
      ? `${(n / 1024).toFixed(1)} KiB`
      : n < 1024 ** 3
        ? `${(n / 1024 ** 2).toFixed(1)} MiB`
        : `${(n / 1024 ** 3).toFixed(1)} GiB`;

/**
 * The read side of the bucket browser (BYOS-UI-001), route `/storage/{mount}?prefix=`: folders, objects,
 * presigned previews of images and audio, and "copy link" which copies a presigned URL and shows its expiry.
 * Presigned URLs live in component state only: never stored, never logged, dropped at `expires_at`.
 */
export function StorageBrowser({ mountId, prefix }: { mountId: string; prefix: string }) {
  const t = useT();
  const [page, setPage] = useState<ObjectPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [preview, setPreview] = useState<{ key: string; signed: Presigned } | null>(null);
  const [copied, setCopied] = useState<{ key: string; expires_at: string } | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const failText = useCallback(
    (e: unknown) =>
      e instanceof ApiError && e.status === 409 ? t('storage.browser.corsNotReady') : errorText(e),
    [t],
  );

  useEffect(() => {
    let live = true;
    setLoading(true);
    setError(null);
    setPreview(null);
    setCopied(null);
    listObjects(mountId, prefix)
      .then((p) => live && setPage(p))
      .catch((e: unknown) => live && setError(failText(e)))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [mountId, prefix, failText]);

  // Drop a presigned URL the moment it expires.
  useEffect(() => {
    if (!preview) return;
    const ms = new Date(preview.signed.expires_at).getTime() - Date.now();
    const id = window.setTimeout(
      () => {
        setPreview(null);
        setNote(t('storage.browser.previewExpired'));
      },
      Math.max(0, ms),
    );
    return () => window.clearTimeout(id);
  }, [preview, t]);

  const more = async () => {
    if (!page?.next_cursor) return;
    try {
      const next = await listObjects(mountId, prefix, page.next_cursor);
      setPage({
        folders: [...page.folders, ...next.folders],
        objects: [...page.objects, ...next.objects],
        next_cursor: next.next_cursor,
      });
    } catch (e) {
      setError(failText(e));
    }
  };
  const show = async (o: ObjectEntry) => {
    setNote(null);
    try {
      setPreview({ key: o.key, signed: await presignGet(mountId, o.key) });
    } catch (e) {
      setError(failText(e));
    }
  };
  const copy = async (o: ObjectEntry) => {
    setNote(null);
    try {
      const signed = await presignGet(mountId, o.key);
      await navigator.clipboard.writeText(signed.url);
      setCopied({ key: o.key, expires_at: signed.expires_at });
    } catch (e) {
      setError(failText(e));
    }
  };

  const crumbs = prefix.split('/').filter(Boolean);
  return (
    <main className="sto-browser">
      <h1 className="us-title">{t('storage.browser.title')}</h1>
      <nav aria-label={t('storage.browser.crumbsAria')} className="sto-crumbs">
        <Link to="/storage/$mountId" params={{ mountId }} search={{ prefix: '' }}>
          {t('storage.browser.root')}
        </Link>
        {crumbs.map((c, i) => (
          <span key={crumbs.slice(0, i + 1).join('/')}>
            {' / '}
            <Link
              to="/storage/$mountId"
              params={{ mountId }}
              search={{ prefix: `${crumbs.slice(0, i + 1).join('/')}/` }}
            >
              {c}
            </Link>
          </span>
        ))}
      </nav>
      {error && (
        <Notice variant="danger" title={t('storage.browser.error')}>
          {error}
        </Notice>
      )}
      {note && <p role="status">{note}</p>}
      {copied && (
        <p role="status">
          {t('storage.browser.copied', {
            name: baseName(copied.key),
            at: formatDateTime(new Date(copied.expires_at)),
          })}
        </p>
      )}
      {loading && <p role="status">{t('storage.browser.loading')}</p>}
      {page && !loading && (
        <>
          {page.folders.length === 0 && page.objects.length === 0 && (
            <p className="us-muted">{t('storage.browser.empty')}</p>
          )}
          <ul className="sto-folders" aria-label={t('storage.browser.foldersAria')}>
            {page.folders.map((f) => (
              <li key={f}>
                <Link to="/storage/$mountId" params={{ mountId }} search={{ prefix: f }}>
                  {baseName(f)}/
                </Link>
              </li>
            ))}
          </ul>
          {page.objects.length > 0 && (
            <table className="sto-table" aria-label={t('storage.browser.objectsAria')}>
              <thead>
                <tr>
                  <th scope="col">{t('storage.browser.colName')}</th>
                  <th scope="col">{t('storage.browser.colSize')}</th>
                  <th scope="col">{t('storage.browser.colActions')}</th>
                </tr>
              </thead>
              <tbody>
                {page.objects.map((o) => (
                  <tr key={o.key}>
                    <td>{baseName(o.key)}</td>
                    <td>{sizeText(o.size)}</td>
                    <td className="us-actions">
                      {previewKind(o.key) && (
                        <Button
                          size="sm"
                          variant="secondary"
                          aria-label={t('storage.browser.previewNamed', { name: baseName(o.key) })}
                          onClick={() => void show(o)}
                        >
                          {t('storage.browser.preview')}
                        </Button>
                      )}
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('storage.browser.copyNamed', { name: baseName(o.key) })}
                        onClick={() => void copy(o)}
                      >
                        {t('storage.browser.copyLink')}
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {page.next_cursor && (
            <Button variant="secondary" onClick={() => void more()}>
              {t('storage.browser.more')}
            </Button>
          )}
        </>
      )}
      {preview && (
        <section className="sto-preview" aria-label={t('storage.browser.previewAria')}>
          <p>
            {baseName(preview.key)}{' '}
            <span className="us-muted">
              {t('storage.browser.expires', {
                at: formatDateTime(new Date(preview.signed.expires_at)),
              })}
            </span>
          </p>
          {previewKind(preview.key) === 'image' ? (
            <img src={preview.signed.url} alt={baseName(preview.key)} className="sto-img" />
          ) : (
            <audio controls src={preview.signed.url} aria-label={baseName(preview.key)} />
          )}
        </section>
      )}
    </main>
  );
}
