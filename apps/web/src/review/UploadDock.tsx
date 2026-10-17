// The upload manager's face (REV-UP-002, REV-UP-003): a dock with the progress of every resumable upload
// (pause, resume, cancel, one overall bar) and the folder import: drop or pick a folder, see how the
// names were read, accept, and the chapters are created and uploaded.
import { Button, Checkbox, TextInput } from '@sideroom/ui';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { useT } from '../i18n';
import * as p13 from './apiP13';
import * as rv from './api';
import { fromFileList, readDropped, uploadable, type PickedFile } from './folderDrop';
import {
  IdbLedger,
  MemoryLedger,
  UploadManager,
  putPartWithFetch,
  type UploadTask,
} from './uploadManager';

/** Files at or above this size go through the resumable path (one part is 64 MB). */
export const RESUMABLE_FROM = 64 * 1024 * 1024;

let shared: UploadManager | null = null;

/** The page-wide manager. Browsers without IndexedDB keep the ledger in memory (no resume after a refresh). */
export function getUploadManager(): UploadManager {
  if (!shared) {
    const ledger = typeof indexedDB === 'undefined' ? new MemoryLedger() : new IdbLedger();
    shared = new UploadManager(
      {
        begin: p13.beginMultipart,
        parts: p13.uploadParts,
        complete: p13.completeUpload,
        abort: p13.abortUpload,
      },
      ledger,
      { put: putPartWithFetch },
    );
  }
  return shared;
}

/** Replace the manager (tests). */
export function setUploadManager(m: UploadManager | null) {
  shared = m;
}

export function useUploads(manager: UploadManager = getUploadManager()) {
  const tasks = useSyncExternalStore(
    (fn) => manager.subscribe(fn),
    () => manager.snapshot(),
  );
  return { tasks, manager, total: manager.total() };
}

const pct = (n: number) => Math.round(Math.min(1, Math.max(0, n)) * 100);

export function UploadDock({ manager }: { manager?: UploadManager }) {
  const t = useT();
  const m = manager ?? getUploadManager();
  const { tasks, total } = useUploads(m);
  // Pick up what a closed page left behind.
  useEffect(() => {
    void m.resumeAll();
  }, [m]);
  if (tasks.length === 0) return null;
  const stateText = (x: UploadTask) =>
    x.state === 'paused'
      ? t('review.upload.statePaused')
      : x.state === 'done'
        ? t('review.upload.stateDone')
        : x.state === 'failed'
          ? t('review.upload.stateFailed')
          : x.state === 'running'
            ? t('review.upload.stateRunning')
            : t('review.upload.stateQueued');
  return (
    <section className="rv-panel" aria-label={t('review.upload.dock')}>
      <h3>{t('review.upload.dock')}</h3>
      <progress
        value={pct(total)}
        max={100}
        aria-label={t('review.upload.total', { percent: pct(total) })}
      />
      <ul className="rv-changes">
        {tasks.map((x) => (
          <li key={x.id}>
            <strong>{x.filename}</strong>
            <progress
              value={pct(x.progress)}
              max={100}
              aria-label={t('review.upload.row', { name: x.filename, percent: pct(x.progress) })}
            />
            <span>
              {stateText(x)}
              {x.error ? `: ${x.error}` : ''}
            </span>
            <span className="rv-row">
              {x.state === 'running' && (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={t('review.upload.pause', { name: x.filename })}
                  onClick={() => void m.pause(x.id)}
                >
                  ❚❚
                </Button>
              )}
              {(x.state === 'paused' || x.state === 'failed') && (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={t(
                    x.state === 'failed' ? 'review.upload.retry' : 'review.upload.resume',
                    {
                      name: x.filename,
                    },
                  )}
                  onClick={() => void m.resume(x.id)}
                >
                  ▶
                </Button>
              )}
              {x.state !== 'done' && (
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={t('review.upload.cancel', { name: x.filename })}
                  onClick={() => void m.cancel(x.id)}
                >
                  ✕
                </Button>
              )}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

interface Proposal {
  pick: PickedFile;
  chapter: number | null;
  title: string | null;
  format: 'images' | 'epub' | 'pdf';
  /** Left out by the person (REV-ED-002): not created, not uploaded. */
  excluded?: boolean;
}

const formatOf = (path: string): Proposal['format'] =>
  /\.epub$/i.test(path) ? 'epub' : /\.pdf$/i.test(path) ? 'pdf' : 'images';

/** Read the names of the picked files into a proposal (chapter number and title per file). */
export async function propose(files: PickedFile[]): Promise<Proposal[]> {
  const picks = uploadable(files);
  if (picks.length === 0) return [];
  const parsed = await rv.parseNames(
    'library',
    picks.map((p) => p.path),
  );
  return picks.map((pick, i) => {
    const l = parsed.items[i]?.library;
    const chapter = l?.chapter ?? l?.volume ?? (formatOf(pick.path) === 'images' ? null : 1);
    return { pick, chapter, title: l?.title ?? null, format: formatOf(pick.path) };
  });
}

/**
 * Drop a folder (or choose one): the names are read into chapters, the person checks the proposal, and
 * "Create" adds a draft chapter per file and uploads it through the manager.
 */
export function FolderImport({
  submissionId,
  onAdded,
  manager,
}: {
  submissionId: string;
  onAdded: () => void;
  manager?: UploadManager;
}) {
  const t = useT();
  const m = manager ?? getUploadManager();
  const input = useRef<HTMLInputElement>(null);
  const [rows, setRows] = useState<Proposal[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const take = async (files: PickedFile[]) => {
    setError(null);
    try {
      setRows(await propose(files));
    } catch {
      setError(t('review.folder.readError'));
    }
  };
  const create = async () => {
    if (!rows) return;
    setBusy(true);
    setError(null);
    try {
      for (const r of rows) {
        if (r.chapter === null || r.excluded) continue;
        const ch = await rv.createDraftChapter(submissionId, {
          number: r.chapter,
          title: r.title,
          format: r.format,
        });
        void m.enqueue(submissionId, ch.id, r.pick.file as File).catch(() => undefined);
      }
      setRows(null);
      onAdded();
    } catch {
      setError(t('review.folder.createError'));
    } finally {
      setBusy(false);
    }
  };
  const ready = rows?.filter((r) => r.chapter !== null && !r.excluded) ?? [];
  const patch = (path: string, p: Partial<Proposal>) =>
    setRows((cur) => cur && cur.map((r) => (r.pick.path === path ? { ...r, ...p } : r)));
  const numbers = new Map<number, number>();
  for (const r of ready) numbers.set(r.chapter!, (numbers.get(r.chapter!) ?? 0) + 1);
  const clashes = [...numbers].filter(([, n]) => n > 1).map(([c]) => c);
  return (
    <div
      className="rv-stack"
      style={{ marginTop: 16 }}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        void readDropped(e.dataTransfer.items).then(take);
      }}
    >
      <p className="rv-muted">{t('review.folder.drop')}</p>
      <input
        ref={input}
        type="file"
        multiple
        // Not in the React types: lets the person pick a whole folder.
        {...({ webkitdirectory: '' } as Record<string, string>)}
        aria-label={t('review.folder.choose')}
        onChange={(e) => {
          if (e.target.files) void take(fromFileList(e.target.files));
          if (input.current) input.current.value = '';
        }}
      />
      {error && <p role="alert">{error}</p>}
      {rows && (
        <>
          <h4>{t('review.folder.proposal')}</h4>
          {rows.length === 0 ? (
            <p className="rv-muted">{t('review.folder.nothing')}</p>
          ) : (
            <ul className="rv-changes" aria-label={t('review.folder.proposal')}>
              {rows.map((r) => (
                <li key={r.pick.path}>
                  <Checkbox
                    label={t('review.folder.include', { path: r.pick.path })}
                    checked={!r.excluded}
                    onChange={(e) => patch(r.pick.path, { excluded: !e.target.checked })}
                  />
                  <TextInput
                    label={t('review.folder.numberLabel', { path: r.pick.path })}
                    inputMode="decimal"
                    value={r.chapter === null ? '' : String(r.chapter)}
                    disabled={r.excluded === true}
                    onChange={(e) => {
                      const v = e.target.value.trim();
                      const n = Number(v);
                      patch(r.pick.path, {
                        chapter: v === '' || !Number.isFinite(n) || n < 0 ? null : n,
                      });
                    }}
                  />
                  <TextInput
                    label={t('review.folder.titleLabel', { path: r.pick.path })}
                    value={r.title ?? ''}
                    maxLength={200}
                    disabled={r.excluded === true}
                    onChange={(e) => patch(r.pick.path, { title: e.target.value || null })}
                  />
                  {!r.excluded && r.chapter === null && <span>{t('review.folder.unplaced')}</span>}
                </li>
              ))}
            </ul>
          )}
          {clashes.map((n) => (
            <p key={n} role="alert">
              {t('review.folder.duplicate', { n })}
            </p>
          ))}
          <div className="rv-row">
            <Button
              onClick={() => void create()}
              loading={busy}
              disabled={ready.length === 0 || clashes.length > 0}
            >
              {t('review.folder.create', { count: ready.length })}
            </Button>
            <Button variant="secondary" onClick={() => setRows(null)}>
              {t('review.folder.discard')}
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
