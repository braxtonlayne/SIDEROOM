// Cinema transcoding (CIN-ADM-002): workers with drain, worker tokens, the job queue with logs, cancel and
// re-run. Needs MANAGE_TRANSCODE. The backend routes are assumed (see transcodeApi.ts); a server without them
// gets a plain "not available yet" state on every tab.
import {
  Badge,
  Button,
  ConfirmDialog,
  EmptyState,
  Modal,
  Notice,
  Select,
  Switch,
  Tabs,
  TextInput,
} from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import {
  LoadMore,
  ReasonDialog,
  SectionHeader,
  StateBadge,
  fmtMs,
  useAdminViewer,
  usePaged,
} from '../ui';
import { NotAvailableYet } from './AssetPanel';
import { Banner, LoadProblem, useLoad, useRun } from './hooks';
import * as tx from './transcodeApi';

export function TranscodingSection() {
  const t = useT();
  const allowed = useAdminViewer().can('MANAGE_TRANSCODE');
  return (
    <>
      <SectionHeader
        title={t('admin.transcoding.title')}
        description={t('admin.transcoding.description')}
      />
      {!allowed ? (
        <EmptyState
          title={t('admin.transcoding.noAccess.title')}
          description={t('admin.transcoding.noAccess.description')}
        />
      ) : (
        <Tabs
          label={t('admin.transcoding.tabs')}
          items={[
            { id: 'queue', label: t('admin.transcoding.tab.queue'), content: <QueueTab /> },
            { id: 'workers', label: t('admin.transcoding.tab.workers'), content: <WorkersTab /> },
            { id: 'tokens', label: t('admin.transcoding.tab.tokens'), content: <TokensTab /> },
          ]}
        />
      )}
    </>
  );
}

/** Load a transcode list; a missing route becomes the "not available yet" state. */
function useTx<T>(key: string, fetcher: () => Promise<T>) {
  const [unavailable, setUnavailable] = useState(false);
  const res = useLoad<T | null>(key, async () => {
    try {
      return await fetcher();
    } catch (e) {
      if (tx.isUnavailable(e)) {
        setUnavailable(true);
        return null;
      }
      throw e;
    }
  });
  return { res, unavailable };
}

function WorkersTab() {
  const t = useT();
  const { res, unavailable } = useTx('tx-workers', tx.listWorkers);
  const { error, notice, run } = useRun();
  if (unavailable) return <NotAvailableYet what={t('admin.transcoding.workers.whatList')} />;
  if (res.error && !res.data)
    return (
      <LoadProblem
        error={res.error}
        status={res.status}
        onRetry={() => void res.reload()}
        what={t('admin.transcoding.workers.what')}
      />
    );
  if (!res.data) return <p className="ss-loading">{t('admin.transcoding.workers.loading')}</p>;
  const workers = res.data.items;
  return (
    <>
      <Banner error={error} notice={notice} />
      {workers.length === 0 ? (
        <EmptyState
          title={t('admin.transcoding.workers.empty.title')}
          description={t('admin.transcoding.workers.empty.description')}
        />
      ) : (
        <ul className="ss-list" aria-label={t('admin.transcoding.tab.workers')}>
          {workers.map((w) => (
            <li key={w.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {w.name}{' '}
                  <Badge variant={w.online ? 'success' : 'neutral'}>
                    {w.online
                      ? t('admin.transcoding.workers.online')
                      : t('admin.transcoding.workers.offline')}
                  </Badge>{' '}
                  {w.drain && (
                    <Badge variant="warning">{t('admin.transcoding.workers.draining')}</Badge>
                  )}
                </div>
                <div className="ss-meta">
                  {t('admin.transcoding.workers.meta', {
                    version: w.version,
                    n: w.active_jobs,
                    seen: fmtMs(w.last_seen_ms),
                  })}
                  {w.capabilities.nvenc ? ' · NVENC' : ''}
                  {w.capabilities.concurrency
                    ? ` · ${t('admin.transcoding.workers.atOnce', { n: w.capabilities.concurrency })}`
                    : ''}
                </div>
              </div>
              <Switch
                label={t('admin.transcoding.workers.drain', { name: w.name })}
                checked={w.drain}
                onChange={(e) =>
                  void run(
                    async () => {
                      await tx.setDrain(w.id, e.target.checked);
                      await res.reload();
                      return true;
                    },
                    e.target.checked
                      ? t('admin.transcoding.workers.draining.on', { name: w.name })
                      : t('admin.transcoding.workers.draining.off', { name: w.name }),
                  )
                }
              />
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function TokensTab() {
  const t = useT();
  const { res, unavailable } = useTx('tx-tokens', tx.listTokens);
  const { busy, error, notice, run } = useRun();
  const [name, setName] = useState('');
  const [secret, setSecret] = useState<string | null>(null);
  const [revoke, setRevoke] = useState<tx.WorkerToken | null>(null);
  if (unavailable) return <NotAvailableYet what={t('admin.transcoding.tab.tokens')} />;
  if (res.error && !res.data)
    return (
      <LoadProblem
        error={res.error}
        status={res.status}
        onRetry={() => void res.reload()}
        what={t('admin.transcoding.tokens.what')}
      />
    );
  if (!res.data) return <p className="ss-loading">{t('admin.transcoding.tokens.loading')}</p>;
  return (
    <>
      <Banner error={error} notice={notice} />
      <form
        className="ss-toolbar"
        aria-label={t('admin.transcoding.tokens.newAria')}
        onSubmit={(e) => {
          e.preventDefault();
          void run(async () => {
            const r = await tx.createToken(name.trim());
            setSecret(r.secret);
            setName('');
            await res.reload();
            return true;
          });
        }}
      >
        <TextInput
          label={t('admin.transcoding.tokens.name')}
          value={name}
          onChange={(e) => setName(e.target.value)}
          hint={t('admin.transcoding.tokens.nameHint')}
        />
        <Button type="submit" loading={busy} disabled={!name.trim()}>
          {t('admin.transcoding.tokens.create')}
        </Button>
      </form>
      {secret && (
        <Notice variant="warning" title={t('admin.transcoding.tokens.copy.title')} role="status">
          {t('admin.transcoding.tokens.copy.body')} <span className="adm-mono">{secret}</span>{' '}
          <Button size="sm" variant="secondary" onClick={() => setSecret(null)}>
            {t('admin.transcoding.tokens.copy.done')}
          </Button>
        </Notice>
      )}
      {res.data.items.length === 0 ? (
        <EmptyState
          title={t('admin.transcoding.tokens.empty.title')}
          description={t('admin.transcoding.tokens.empty.description')}
        />
      ) : (
        <ul className="ss-list" aria-label={t('admin.transcoding.tab.tokens')}>
          {res.data.items.map((tk) => (
            <li key={tk.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {tk.name}{' '}
                  {tk.revoked_at_ms ? (
                    <Badge>{t('admin.transcoding.tokens.revoked')}</Badge>
                  ) : (
                    <Badge variant="success">{t('admin.transcoding.tokens.active')}</Badge>
                  )}
                </div>
                <div className="ss-meta">
                  {t('admin.transcoding.tokens.created', { time: fmtMs(tk.created_at_ms) })}
                  {tk.last_used_ms
                    ? ` · ${t('admin.transcoding.tokens.lastUsed', { time: fmtMs(tk.last_used_ms) })}`
                    : ` · ${t('admin.transcoding.tokens.neverUsed')}`}
                </div>
              </div>
              {!tk.revoked_at_ms && (
                <Button
                  size="sm"
                  variant="danger"
                  aria-label={t('admin.transcoding.tokens.revokeAria', { name: tk.name })}
                  onClick={() => setRevoke(tk)}
                >
                  {t('admin.transcoding.tokens.revoke')}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      <ConfirmDialog
        open={!!revoke}
        danger
        title={t('admin.transcoding.tokens.revokeTitle', {
          name: revoke?.name ?? t('admin.transcoding.tokens.tokenWord'),
        })}
        description={t('admin.transcoding.tokens.revokeDescription')}
        confirmLabel={t('admin.transcoding.tokens.revokeConfirm')}
        loading={busy}
        onCancel={() => setRevoke(null)}
        onConfirm={() => {
          const tk = revoke;
          if (!tk) return;
          void run(
            async () => {
              await tx.revokeToken(tk.id);
              await res.reload();
              return true;
            },
            t('admin.transcoding.tokens.revoked.done', { name: tk.name }),
          ).then(() => setRevoke(null));
        }}
      />
    </>
  );
}

const ALL = '__all';
function QueueTab() {
  const t = useT();
  const stateOptions = [
    { value: ALL, label: t('admin.transcoding.queue.allStates') },
    ...tx.JOB_STATES.map((s) => ({ value: s, label: s })),
  ];
  const [state, setState] = useState(ALL);
  const [unavailable, setUnavailable] = useState(false);
  const [failure, setFailure] = useState<number | null>(null);
  const paged = usePaged<tx.TranscodeJob>(`tx-jobs:${state}`, async (after) => {
    try {
      const p = await tx.listJobs({ state: state === ALL ? undefined : state, after, limit: 50 });
      setUnavailable(false);
      return { items: p.items, next: p.next_after };
    } catch (e) {
      if (tx.isUnavailable(e)) {
        setUnavailable(true);
        return { items: [], next: null };
      }
      setFailure((e as { status?: number }).status ?? null);
      throw e;
    }
  });
  const { busy, error, notice, run } = useRun();
  const [logs, setLogs] = useState<tx.TranscodeJob | null>(null);
  const [cancel, setCancel] = useState<tx.TranscodeJob | null>(null);
  const [rerun, setRerun] = useState<tx.TranscodeJob | null>(null);

  if (unavailable) return <NotAvailableYet what={t('admin.transcoding.queue.whatQueue')} />;
  return (
    <>
      <div className="ss-toolbar">
        <Select
          aria-label={t('admin.transcoding.queue.jobState')}
          options={stateOptions}
          value={state}
          onValueChange={setState}
        />
        <Button variant="secondary" onClick={paged.reload}>
          {t('admin.transcoding.queue.refresh')}
        </Button>
      </div>
      <Banner error={error} notice={notice} />
      {paged.error && (
        <LoadProblem
          error={paged.error}
          status={failure}
          onRetry={paged.reload}
          what={t('admin.transcoding.queue.what')}
        />
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={t('admin.transcoding.queue.empty.title')}
          description={t('admin.transcoding.queue.empty.description')}
        />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label={t('admin.transcoding.queue.table')}>
            <thead>
              <tr>
                <th scope="col">{t('admin.transcoding.col.job')}</th>
                <th scope="col">{t('admin.transcoding.col.asset')}</th>
                <th scope="col">{t('admin.transcoding.col.state')}</th>
                <th scope="col">{t('admin.transcoding.col.worker')}</th>
                <th scope="col">{t('admin.transcoding.col.created')}</th>
                <th scope="col">{t('admin.transcoding.col.actions')}</th>
              </tr>
            </thead>
            <tbody>
              {paged.items.map((j) => (
                <tr key={j.id}>
                  <td>
                    <span className="adm-mono">{j.id}</span>
                    <div className="ss-meta">
                      {j.kind} · {t('admin.transcoding.queue.attempt', { n: j.attempt })}
                    </div>
                  </td>
                  <td>
                    <span className="adm-mono">{j.asset_id}</span>
                  </td>
                  <td>
                    <StateBadge state={j.state} />
                    {j.state === 'running' && (
                      <div className="ss-meta">
                        {j.stage ?? ''} {j.percent != null ? `${Math.round(j.percent)}%` : ''}
                      </div>
                    )}
                    {j.error && <div className="ss-meta">{j.error}</div>}
                  </td>
                  <td>{j.worker_name ?? j.worker_id ?? ''}</td>
                  <td>{fmtMs(j.created_at_ms)}</td>
                  <td>
                    <div className="adm-actions">
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('admin.transcoding.queue.logsAria', { id: j.id })}
                        onClick={() => setLogs(j)}
                      >
                        {t('admin.transcoding.queue.logs')}
                      </Button>
                      {(j.state === 'queued' || j.state === 'running') && (
                        <Button
                          size="sm"
                          variant="danger"
                          aria-label={t('admin.transcoding.queue.cancelAria', { id: j.id })}
                          onClick={() => setCancel(j)}
                        >
                          {t('admin.transcoding.queue.cancel')}
                        </Button>
                      )}
                      {(j.state === 'failed' || j.state === 'cancelled') && (
                        <Button
                          size="sm"
                          variant="secondary"
                          aria-label={t('admin.transcoding.queue.rerunAria', { id: j.id })}
                          onClick={() => setRerun(j)}
                        >
                          {t('admin.transcoding.queue.rerun')}
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <LoadMore paged={paged} />
      <JobLogs job={logs} onClose={() => setLogs(null)} />
      <ConfirmDialog
        open={!!cancel}
        danger
        title={t('admin.transcoding.queue.cancelDialog.title')}
        description={t('admin.transcoding.queue.cancelDialog.description')}
        confirmLabel={t('admin.transcoding.queue.cancelDialog.confirm')}
        loading={busy}
        onCancel={() => setCancel(null)}
        onConfirm={() => {
          const j = cancel;
          if (!j) return;
          void run(async () => {
            await tx.cancelJob(j.id);
            paged.reload();
            return true;
          }, t('admin.transcoding.queue.cancelRequested')).then(() => setCancel(null));
        }}
      />
      <ReasonDialog
        open={!!rerun}
        title={t('admin.transcoding.queue.rerunDialog.title')}
        description={t('admin.transcoding.queue.rerunDialog.description')}
        confirmLabel={t('admin.transcoding.queue.rerunDialog.confirm')}
        reasonOptional
        onCancel={() => setRerun(null)}
        onConfirm={async (reason) => {
          if (!rerun) return;
          await tx.retranscodeAsset(rerun.asset_id, reason);
          setRerun(null);
          paged.reload();
        }}
      />
    </>
  );
}

function JobLogs({ job, onClose }: { job: tx.TranscodeJob | null; onClose: () => void }) {
  const t = useT();
  return (
    <Modal
      open={!!job}
      onClose={onClose}
      title={t('admin.transcoding.queue.logsAria', { id: job?.id ?? '' })}
      size="lg"
      footer={<Button onClick={onClose}>{t('admin.transcoding.logs.done')}</Button>}
    >
      {job && <JobLogBody id={job.id} />}
    </Modal>
  );
}

function JobLogBody({ id }: { id: string }) {
  const t = useT();
  const { res, unavailable } = useTx(`tx-job:${id}`, () => tx.getJob(id));
  if (unavailable) return <NotAvailableYet what={t('admin.transcoding.logs.whatLogs')} />;
  if (res.error && !res.data)
    return (
      <LoadProblem error={res.error} status={res.status} what={t('admin.transcoding.logs.what')} />
    );
  if (!res.data) return <p className="ss-loading">{t('admin.transcoding.logs.loading')}</p>;
  const j = res.data;
  return (
    <>
      {j.encoder && (
        <p className="ss-meta">{t('admin.transcoding.logs.encoder', { name: j.encoder })}</p>
      )}
      <pre className="adm-pre" aria-label={t('admin.transcoding.logs.output')} tabIndex={0}>
        {j.log_tail?.trim() ? j.log_tail : t('admin.transcoding.logs.none')}
      </pre>
    </>
  );
}
