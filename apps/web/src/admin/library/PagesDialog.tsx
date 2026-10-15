// Chapter pages (LIB-ADM-001): reorder, mark spreads, remove, and re-convert from a new archive.
// Reordering and removing send the whole page list to `PUT /admin/library/chapters/{id}/pages`; re-converting
// uploads an archive through the presigned-PUT flow and polls the ingest job.
import { Button, Checkbox, Modal, Notice } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { ApiProblem, Async, useResource } from '../ui';
import * as api from './api';

export function PagesDialog({
  chapter,
  onClose,
  onChanged,
}: {
  chapter: api.Chapter;
  onClose: () => void;
  onChanged: () => void;
}) {
  const t = useT();
  const res = useResource(`lib-pages:${chapter.id}`, () => api.getPages(chapter.id));
  return (
    <Modal
      open
      onClose={onClose}
      title={t('admin.library.pagesTitle', { n: chapter.number })}
      size="lg"
    >
      <Async res={res} label={t('admin.library.loadingPages')}>
        {(m) => (
          <Editor
            chapter={chapter}
            initial={m.pages}
            reload={() => res.reload()}
            onChanged={onChanged}
          />
        )}
      </Async>
    </Modal>
  );
}

function Editor({
  chapter,
  initial,
  reload,
  onChanged,
}: {
  chapter: api.Chapter;
  initial: api.AdminPage[];
  reload: () => Promise<unknown> | void;
  onChanged: () => void;
}) {
  const t = useT();
  const [pages, setPages] = useState(initial);
  useEffect(() => setPages(initial), [initial]);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const dirty = pages.length !== initial.length || pages.some((p, i) => p !== initial[i]);
  const move = (i: number, d: -1 | 1) =>
    setPages((cur) => {
      const j = i + d;
      if (j < 0 || j >= cur.length) return cur;
      const next = [...cur];
      [next[i], next[j]] = [next[j]!, next[i]!];
      return next;
    });
  const save = async () => {
    setError(null);
    setSaved(false);
    try {
      await api.replacePages(chapter.id, pages.map(api.toPageInput));
      setSaved(true);
      onChanged();
      await reload();
    } catch (e) {
      setError(api.errorText(e));
    }
  };
  return (
    <div className="adm-pages">
      <ApiProblem error={error} />
      {saved && <Notice variant="success">{t('admin.library.pagesSaved')}</Notice>}
      {pages.length === 0 ? (
        <p className="ss-na">{t('admin.library.noPages')}</p>
      ) : (
        <ol className="adm-page-list" aria-label={t('admin.library.pageList')}>
          {pages.map((p, i) => (
            <li key={api.keyOfUrl(p.url)} className="adm-page-row">
              <img
                src={p.variants[0]?.url ?? p.url}
                alt=""
                width={48}
                height={Math.round((48 * p.height) / Math.max(1, p.width))}
                loading="lazy"
              />
              <span className="ss-name">{t('admin.library.pageN', { n: i + 1 })}</span>
              <span className="ss-meta">
                {p.width}×{p.height}
              </span>
              <Checkbox
                label={t('admin.library.spread')}
                checked={p.is_double_spread}
                onChange={(e) =>
                  setPages((cur) =>
                    cur.map((x, k) => (k === i ? { ...x, is_double_spread: e.target.checked } : x)),
                  )
                }
              />
              <Button
                size="sm"
                variant="secondary"
                aria-label={t('admin.library.moveUp', { n: i + 1 })}
                disabled={i === 0}
                onClick={() => move(i, -1)}
              >
                {'↑'}
              </Button>
              <Button
                size="sm"
                variant="secondary"
                aria-label={t('admin.library.moveDown', { n: i + 1 })}
                disabled={i === pages.length - 1}
                onClick={() => move(i, 1)}
              >
                {'↓'}
              </Button>
              <Button
                size="sm"
                variant="danger"
                aria-label={t('admin.library.removePage', { n: i + 1 })}
                onClick={() => setPages((cur) => cur.filter((_, k) => k !== i))}
              >
                {t('admin.library.remove')}
              </Button>
            </li>
          ))}
        </ol>
      )}
      <div className="adm-actions">
        <Button onClick={() => void save()} disabled={!dirty || pages.length === 0}>
          {t('admin.library.savePages')}
        </Button>
        <Button variant="secondary" disabled={!dirty} onClick={() => setPages(initial)}>
          {t('admin.library.resetPages')}
        </Button>
      </div>
      <Reconvert chapter={chapter} reload={reload} onChanged={onChanged} />
    </div>
  );
}

const POLL_MS = 1500;

function Reconvert({
  chapter,
  reload,
  onChanged,
}: {
  chapter: api.Chapter;
  reload: () => Promise<unknown> | void;
  onChanged: () => void;
}) {
  const t = useT();
  const input = useRef<HTMLInputElement>(null);
  const [state, setState] = useState<{ phase: string; upload?: api.Upload } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const run = async (file: File) => {
    setError(null);
    try {
      setState({ phase: 'uploading' });
      const slot = await api.beginUpload(chapter.id, file);
      const put = await fetch(slot.upload_url, { method: slot.upload_method, body: file });
      if (!put.ok) throw new Error(`upload failed (${put.status})`);
      let up = await api.completeUpload(slot.upload.id);
      setState({ phase: 'converting', upload: up });
      while (up.status === 'queued' || up.status === 'running') {
        await new Promise((r) => setTimeout(r, POLL_MS));
        up = await api.uploadStatus(up.id);
        setState({ phase: 'converting', upload: up });
      }
      setState({ phase: up.status, upload: up });
      if (up.status === 'done') {
        onChanged();
        await reload();
      }
    } catch (e) {
      setState(null);
      setError(api.errorText(e));
    }
  };
  return (
    <section className="adm-reconvert" aria-label={t('admin.library.reconvert')}>
      <h3>{t('admin.library.reconvert')}</h3>
      <p className="ss-meta">{t('admin.library.reconvertHint')}</p>
      <ApiProblem error={error} />
      <input
        ref={input}
        type="file"
        accept=".cbz,.zip,.cb7,.7z,.cbr,.rar"
        hidden
        aria-label={t('admin.library.archiveFile')}
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) void run(f);
          e.target.value = '';
        }}
      />
      <Button
        variant="secondary"
        disabled={state?.phase === 'uploading' || state?.phase === 'converting'}
        onClick={() => input.current?.click()}
      >
        {t('admin.library.chooseArchive')}
      </Button>
      {state?.phase === 'uploading' && <p role="status">{t('admin.library.uploading')}</p>}
      {state?.phase === 'converting' && <p role="status">{t('admin.library.converting')}</p>}
      {state?.phase === 'done' && (
        <Notice variant="success">
          {t('admin.library.converted', { count: state.upload?.page_count ?? 0 })}
          {state.upload?.warnings.length ? ` ${state.upload.warnings.join(' ')}` : ''}
        </Notice>
      )}
      {(state?.phase === 'failed' || state?.phase === 'expired') && (
        <Notice variant="danger">
          {state.upload?.error ?? t('admin.library.conversionFailed')}
        </Notice>
      )}
    </section>
  );
}
