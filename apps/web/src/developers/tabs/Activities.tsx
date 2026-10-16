import { Button, Notice, TextInput } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState, type FormEvent } from 'react';
import { useT } from '../../i18n';
import { errorText, getActivity, isUnavailable, keys, saveActivity, submitActivity } from '../api';
import type { ActivityMapping, DevApp } from '../types';
import { CopyButton, LoadError, Loading, NotAvailable } from '../ui';

/** Most URL mappings the server accepts. */
export const MAX_MAPPINGS = 10;

/**
 * The Activities tab (DEV-PORT-003, DEV-ACT-001): where the Activity runs (entry URL), the URL mappings the
 * proxy follows, and its review. Editing anything withdraws an approval; "Submit for review" makes the server
 * fetch the entry document and pin its SHA-256, and only that document is served once staff approves.
 */
export function ActivitiesTab({ app }: { app: DevApp }) {
  const t = useT();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: keys.activity(app.id), queryFn: () => getActivity(app.id) });
  const [entry, setEntry] = useState('');
  const [rows, setRows] = useState<ActivityMapping[]>([]);
  const [error, setError] = useState<string | null>(null);
  const cfg = q.data?.config;

  // Load the saved values into the form when they change on the server.
  useEffect(() => {
    if (!cfg) return;
    setEntry(cfg.entry_url ?? '');
    setRows(cfg.url_mappings);
  }, [cfg]);

  const refresh = () => void qc.invalidateQueries({ queryKey: keys.activity(app.id) });
  const save = useMutation({
    mutationFn: () =>
      saveActivity(app.id, {
        entry_url: entry.trim() || null,
        url_mappings: rows.map((r) => ({ prefix: r.prefix.trim(), target: r.target.trim() })),
      }),
    onSuccess: () => {
      setError(null);
      refresh();
    },
    onError: (e) => setError(errorText(e, t('developers.activities.saveFailed'))),
  });
  const submit = useMutation({
    mutationFn: () => submitActivity(app.id),
    onSuccess: () => {
      setError(null);
      refresh();
    },
    onError: (e) => setError(errorText(e, t('developers.activities.submitFailed'))),
  });

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
  const saved =
    (q.data.config.entry_url ?? '') === entry.trim() &&
    JSON.stringify(q.data.config.url_mappings) ===
      JSON.stringify(rows.map((r) => ({ prefix: r.prefix.trim(), target: r.target.trim() })));
  const setRow = (i: number, patch: Partial<ActivityMapping>) =>
    setRows((rs) => rs.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    save.mutate();
  };

  return (
    <section className="dev-card" aria-labelledby="dev-act-h">
      <h2 id="dev-act-h" className="dev-card-title">
        {t('developers.activities.heading')}
      </h2>
      <p className="dev-muted">{t('developers.activities.intro')}</p>
      {!enabled && <Notice variant="info">{t('developers.activities.disabled')}</Notice>}
      <p>
        <strong>{t('developers.activities.originLabel')}</strong> <code>{origin}</code>{' '}
        <CopyButton text={`${origin}/`} label={t('developers.activities.copyRedirect')} />
      </p>
      <p className="dev-muted">{t('developers.activities.redirectHint', { uri: `${origin}/` })}</p>

      <form onSubmit={onSubmit}>
        <TextInput
          label={t('developers.activities.entryUrl')}
          hint={t('developers.activities.entryHint')}
          value={entry}
          maxLength={512}
          placeholder="https://activity.example.com/index.html"
          onChange={(e) => setEntry(e.target.value)}
        />
        <h3 className="dev-card-title">{t('developers.activities.mappingsHeading')}</h3>
        <p className="dev-muted">{t('developers.activities.mappingsIntro')}</p>
        <ul className="dev-uri-list" aria-label={t('developers.activities.mappingsLabel')}>
          {rows.map((r, i) => (
            <li key={i}>
              <TextInput
                label={t('developers.activities.prefix')}
                value={r.prefix}
                maxLength={64}
                placeholder="/api"
                onChange={(e) => setRow(i, { prefix: e.target.value })}
              />
              <TextInput
                label={t('developers.activities.target')}
                value={r.target}
                maxLength={512}
                placeholder="https://api.example.com"
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
      {q.data.config.reviewed_sha256 && (
        <p className="dev-muted">
          {t('developers.activities.approvedHash')} <code>{q.data.config.reviewed_sha256}</code>
        </p>
      )}
      <p className="dev-muted">{t('developers.activities.submitHint')}</p>
      <div className="dev-actions">
        <Button
          loading={submit.isPending}
          disabled={!q.data.config.entry_url || !saved || state === 'approved'}
          onClick={() => submit.mutate()}
        >
          {state === 'pending'
            ? t('developers.activities.resubmit')
            : t('developers.activities.submit')}
        </Button>
      </div>
    </section>
  );
}
