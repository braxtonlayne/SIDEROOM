import { Button, Notice, TextInput } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState, type DragEvent, type FormEvent } from 'react';
import { useT } from '../../i18n';
import {
  MAX_BUNDLE_BYTES,
  errorText,
  getActivity,
  getActivityBundle,
  isUnavailable,
  keys,
  saveActivity,
  submitActivity,
  uploadActivityBundle,
} from '../api';
import type { ActivityBundle, ActivityBundleDetail, ActivityMapping, DevApp } from '../types';
import { LoadError, Loading, NotAvailable } from '../ui';

/** Most URL mappings the server accepts. */
export const MAX_MAPPINGS = 10;

function size(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MiB`;
}

/** The listing of one bundle's files, loaded when asked for. */
function Files({ appId, bundleId }: { appId: string; bundleId: string }) {
  const t = useT();
  const q = useQuery({
    queryKey: ['dev', 'activity', appId, 'bundle', bundleId],
    queryFn: () => getActivityBundle(appId, bundleId),
  });
  if (q.isPending) return <Loading what={t('developers.activities.filesWhat')} />;
  if (q.error)
    return (
      <LoadError
        what={t('developers.activities.filesWhat')}
        error={q.error}
        onRetry={() => void q.refetch()}
      />
    );
  return (
    <ul className="dev-file-list" aria-label={t('developers.activities.filesLabel')}>
      {q.data.entries.map((e) => (
        <li key={e.path}>
          <code>{e.path}</code> <span className="dev-muted">{size(e.size)}</span>
        </li>
      ))}
    </ul>
  );
}

/**
 * The Activities tab (DEV-PORT-003, DEV-ACT-001, J-12): an Activity is a zip of static files that the server
 * hosts. Upload it (drag a zip or pick a file), see what it holds, submit that bundle for review, and read
 * the review history. An application in development can also point at a live development URL. URL mappings
 * send requests under a path to servers of yours; changing them withdraws an approval.
 */
export function ActivitiesTab({ app }: { app: DevApp }) {
  const t = useT();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: keys.activity(app.id), queryFn: () => getActivity(app.id) });
  const [dev, setDev] = useState('');
  const [rows, setRows] = useState<ActivityMapping[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [uploaded, setUploaded] = useState<ActivityBundleDetail | null>(null);
  const [showFiles, setShowFiles] = useState<string | null>(null);
  const [over, setOver] = useState(false);
  const picker = useRef<HTMLInputElement | null>(null);
  const cfg = q.data?.config;

  // Load the saved values into the form when they change on the server.
  useEffect(() => {
    if (!cfg) return;
    setDev(cfg.dev_entry_url ?? '');
    setRows(cfg.url_mappings);
  }, [cfg]);

  const refresh = () => void qc.invalidateQueries({ queryKey: keys.activity(app.id) });
  const save = useMutation({
    mutationFn: () =>
      saveActivity(app.id, {
        dev_entry_url: dev.trim() || null,
        url_mappings: rows.map((r) => ({ prefix: r.prefix.trim(), target: r.target.trim() })),
      }),
    onSuccess: () => {
      setError(null);
      refresh();
    },
    onError: (e) => setError(errorText(e, t('developers.activities.saveFailed'))),
  });
  const submit = useMutation({
    mutationFn: (bundleId: string) => submitActivity(app.id, bundleId),
    onSuccess: () => {
      setError(null);
      refresh();
    },
    onError: (e) => setError(errorText(e, t('developers.activities.submitFailed'))),
  });
  const upload = useMutation({
    mutationFn: (file: File) => {
      setProgress(0);
      return uploadActivityBundle(app.id, file, setProgress);
    },
    onSuccess: (detail) => {
      setUploadError(null);
      setUploaded(detail);
      setShowFiles(detail.bundle.id);
      refresh();
    },
    onError: (e) => setUploadError(errorText(e, t('developers.activities.uploadFailed'))),
    onSettled: () => setProgress(null),
  });

  const take = (file: File | undefined) => {
    if (!file) return;
    setUploadError(null);
    if (file.size > MAX_BUNDLE_BYTES) {
      setUploadError(t('developers.activities.tooLarge'));
      return;
    }
    upload.mutate(file);
  };
  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setOver(false);
    take(e.dataTransfer.files[0]);
  };

  if (q.isPending) return <Loading what={t('developers.activities.what')} />;
  if (q.error && isUnavailable(q.error))
    return <NotAvailable>{t('developers.activities.unavailable')}</NotAvailable>;
  if (q.error)
    return (
      <LoadError
        what={t('developers.activities.what')}
        error={q.error}
        onRetry={() => void q.refetch()}
      />
    );

  const { enabled, origin } = q.data;
  const state = q.data.config.review_state;
  const stateText = {
    draft: t('developers.activities.stateDraft'),
    pending: t('developers.activities.statePending'),
    approved: t('developers.activities.stateApproved'),
    rejected: t('developers.activities.stateRejected'),
  } as const;
  const bundleState = {
    draft: t('developers.activities.bundleDraft'),
    pending: t('developers.activities.bundlePending'),
    approved: t('developers.activities.bundleApproved'),
    rejected: t('developers.activities.bundleRejected'),
    superseded: t('developers.activities.bundleSuperseded'),
  } as const;
  const saved =
    (q.data.config.dev_entry_url ?? '') === dev.trim() &&
    JSON.stringify(q.data.config.url_mappings) ===
      JSON.stringify(rows.map((r) => ({ prefix: r.prefix.trim(), target: r.target.trim() })));
  const setRow = (i: number, patch: Partial<ActivityMapping>) =>
    setRows((rs) => rs.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    save.mutate();
  };
  const bundles = q.data.config.bundles.filter((b) => b.state !== 'superseded');
  const reviewed = q.data.config.reviewed_bundle;
  const inDevelopment = app.release_state === 'development';

  const row = (b: ActivityBundle) => (
    <li key={b.id} className="dev-bundle">
      <div>
        <strong>{bundleState[b.state]}</strong>{' '}
        <span className="dev-muted">
          {t('developers.activities.bundleMeta', { n: b.entry_count, size: size(b.byte_size) })}
        </span>
        <br />
        <code className="dev-hash">{b.sha256}</code>
        {b.note ? (
          <span className="dev-muted">
            {' '}
            {t('developers.activities.reviewerNote', { note: b.note })}
          </span>
        ) : null}
      </div>
      <div className="dev-actions">
        <Button
          variant="ghost"
          size="sm"
          aria-expanded={showFiles === b.id}
          onClick={() => setShowFiles(showFiles === b.id ? null : b.id)}
        >
          {showFiles === b.id
            ? t('developers.activities.hideFiles')
            : t('developers.activities.showFiles')}
        </Button>
        {b.state === 'draft' && (
          <Button
            size="sm"
            loading={submit.isPending && submit.variables === b.id}
            onClick={() => submit.mutate(b.id)}
          >
            {t('developers.activities.submit')}
          </Button>
        )}
      </div>
      {showFiles === b.id && <Files appId={app.id} bundleId={b.id} />}
    </li>
  );

  return (
    <section className="dev-card" aria-labelledby="dev-act-h">
      <h2 id="dev-act-h" className="dev-card-title">
        {t('developers.activities.heading')}
      </h2>
      <p className="dev-muted">{t('developers.activities.intro')}</p>
      {!enabled && <Notice variant="info">{t('developers.activities.disabled')}</Notice>}
      <p>
        <strong>{t('developers.activities.originLabel')}</strong> <code>{origin}</code>
      </p>
      <p className="dev-muted">{t('developers.activities.redirectAuto')}</p>

      <h3 className="dev-card-title">{t('developers.activities.bundleHeading')}</h3>
      <p className="dev-muted">{t('developers.activities.bundleIntro')}</p>
      <div
        className="dev-dropzone"
        data-over={over || undefined}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={onDrop}
      >
        <input
          ref={picker}
          type="file"
          hidden
          accept=".zip,application/zip"
          aria-label={t('developers.activities.pickZip')}
          data-testid="bundle-input"
          onChange={(e) => {
            take(e.target.files?.[0]);
            e.target.value = '';
          }}
        />
        <p>{t('developers.activities.drop')}</p>
        <Button variant="ghost" loading={upload.isPending} onClick={() => picker.current?.click()}>
          {t('developers.activities.choose')}
        </Button>
        {progress !== null && (
          <progress
            value={Math.round(progress * 100)}
            max={100}
            aria-label={t('developers.activities.uploading')}
          />
        )}
      </div>
      {uploadError && (
        <p className="dev-error" role="alert">
          {uploadError}
        </p>
      )}
      {uploaded && !uploadError && (
        <p role="status">
          {t('developers.activities.uploaded', {
            n: uploaded.bundle.entry_count,
            size: size(uploaded.bundle.byte_size),
          })}
        </p>
      )}
      {bundles.length > 0 && (
        <ul className="dev-bundle-list" aria-label={t('developers.activities.bundlesLabel')}>
          {bundles.map(row)}
        </ul>
      )}

      <h3 className="dev-card-title">{t('developers.activities.reviewHeading')}</h3>
      <p role="status">
        {stateText[state]}
        {q.data.config.review_note ? (
          <>
            {' '}
            <span className="dev-muted">
              {t('developers.activities.reviewerNote', { note: q.data.config.review_note })}
            </span>
          </>
        ) : null}
      </p>
      {reviewed && (
        <p className="dev-muted">
          {t('developers.activities.approvedHash')}{' '}
          <code className="dev-hash">{reviewed.sha256}</code>
        </p>
      )}
      <p className="dev-muted">{t('developers.activities.submitHint')}</p>

      <form onSubmit={onSubmit}>
        {inDevelopment && (
          <TextInput
            label={t('developers.activities.devUrl')}
            hint={t('developers.activities.devUrlHint')}
            value={dev}
            maxLength={512}
            placeholder={t('developers.activities.devUrlPlaceholder')}
            onChange={(e) => setDev(e.target.value)}
          />
        )}
        <h3 className="dev-card-title">{t('developers.activities.mappingsHeading')}</h3>
        <p className="dev-muted">{t('developers.activities.mappingsIntro')}</p>
        <ul className="dev-uri-list" aria-label={t('developers.activities.mappingsLabel')}>
          {rows.map((r, i) => (
            <li key={i}>
              <TextInput
                label={t('developers.activities.prefix')}
                value={r.prefix}
                maxLength={64}
                placeholder={t('developers.activities.prefixPlaceholder')}
                onChange={(e) => setRow(i, { prefix: e.target.value })}
              />
              <TextInput
                label={t('developers.activities.target')}
                value={r.target}
                maxLength={512}
                placeholder={t('developers.activities.targetPlaceholder')}
                onChange={(e) => setRow(i, { target: e.target.value })}
              />
              <Button
                variant="ghost"
                size="sm"
                aria-label={t('developers.activities.removeMapping', { prefix: r.prefix || '…' })}
                onClick={() => setRows((rs) => rs.filter((_, j) => j !== i))}
              >
                {t('developers.activities.remove')}
              </Button>
            </li>
          ))}
        </ul>
        <div className="dev-actions">
          <Button
            variant="ghost"
            disabled={rows.length >= MAX_MAPPINGS}
            onClick={() => setRows((rs) => [...rs, { prefix: '', target: '' }])}
          >
            {t('developers.activities.addMapping')}
          </Button>
          <Button type="submit" loading={save.isPending} disabled={saved}>
            {t('developers.activities.save')}
          </Button>
        </div>
      </form>

      {error && (
        <p className="dev-error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
