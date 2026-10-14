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
  const allowed = useAdminViewer().can('MANAGE_TRANSCODE');
  return (
    <>
      <SectionHeader
        title="Cinema transcoding"
        description="Transcode workers, the tokens they sign in with, and the job queue. Re-running a job reuses the stored original."
      />
      {!allowed ? (
        <EmptyState
          title="You do not have access"
          description="Transcoding takes the Manage transcode permission."
        />
      ) : (
        <Tabs
          label="Transcoding"
          items={[
            { id: 'queue', label: 'Queue', content: <QueueTab /> },
            { id: 'workers', label: 'Workers', content: <WorkersTab /> },
            { id: 'tokens', label: 'Worker tokens', content: <TokensTab /> },
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
  const { res, unavailable } = useTx('tx-workers', tx.listWorkers);
  const { error, notice, run } = useRun();
  if (unavailable) return <NotAvailableYet what="The worker list" />;
  if (res.error && !res.data)
    return (
      <LoadProblem
        error={res.error}
        status={res.status}
        onRetry={() => void res.reload()}
        what="workers"
      />
    );
  if (!res.data) return <p className="ss-loading">Loading workers…</p>;
  const workers = res.data.items;
  return (
    <>
      <Banner error={error} notice={notice} />
      {workers.length === 0 ? (
        <EmptyState
          title="No workers"
          description="A worker appears here after it registers with one of the tokens on the next tab."
        />
      ) : (
        <ul className="ss-list" aria-label="Workers">
          {workers.map((w) => (
            <li key={w.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {w.name}{' '}
                  <Badge variant={w.online ? 'success' : 'neutral'}>
                    {w.online ? 'online' : 'offline'}
                  </Badge>{' '}
                  {w.drain && <Badge variant="warning">draining</Badge>}
                </div>
                <div className="ss-meta">
                  v{w.version} · {w.active_jobs} active job{w.active_jobs === 1 ? '' : 's'} · seen{' '}
                  {fmtMs(w.last_seen_ms)}
                  {w.capabilities.nvenc ? ' · NVENC' : ''}
                  {w.capabilities.concurrency ? ` · ${w.capabilities.concurrency} at once` : ''}
                </div>
              </div>
              <Switch
                label={`Drain ${w.name}`}
                checked={w.drain}
                onChange={(e) =>
                  void run(
                    async () => {
                      await tx.setDrain(w.id, e.target.checked);
                      await res.reload();
                      return true;
                    },
                    e.target.checked
                      ? `${w.name} will finish its jobs and take no more.`
                      : `${w.name} is taking jobs again.`,
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
  const { res, unavailable } = useTx('tx-tokens', tx.listTokens);
  const { busy, error, notice, run } = useRun();
  const [name, setName] = useState('');
  const [secret, setSecret] = useState<string | null>(null);
  const [revoke, setRevoke] = useState<tx.WorkerToken | null>(null);
  if (unavailable) return <NotAvailableYet what="Worker tokens" />;
  if (res.error && !res.data)
    return (
      <LoadProblem
        error={res.error}
        status={res.status}
        onRetry={() => void res.reload()}
        what="worker tokens"
      />
    );
  if (!res.data) return <p className="ss-loading">Loading tokens…</p>;
  return (
    <>
      <Banner error={error} notice={notice} />
      <form
        className="ss-toolbar"
        aria-label="New worker token"
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
          label="Token name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          hint="For example the machine it runs on"
        />
        <Button type="submit" loading={busy} disabled={!name.trim()}>
          Create token
        </Button>
      </form>
      {secret && (
        <Notice variant="warning" title="Copy this token now" role="status">
          It is shown once and only its hash is stored. <span className="adm-mono">{secret}</span>{' '}
          <Button size="sm" variant="secondary" onClick={() => setSecret(null)}>
            I have copied it
          </Button>
        </Notice>
      )}
      {res.data.items.length === 0 ? (
        <EmptyState title="No worker tokens" description="Create one for each transcode machine." />
      ) : (
        <ul className="ss-list" aria-label="Worker tokens">
          {res.data.items.map((t) => (
            <li key={t.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {t.name}{' '}
                  {t.revoked_at_ms ? (
                    <Badge>revoked</Badge>
                  ) : (
                    <Badge variant="success">active</Badge>
                  )}
                </div>
                <div className="ss-meta">
                  created {fmtMs(t.created_at_ms)}
                  {t.last_used_ms ? ` · last used ${fmtMs(t.last_used_ms)}` : ' · never used'}
                </div>
              </div>
              {!t.revoked_at_ms && (
                <Button
                  size="sm"
                  variant="danger"
                  aria-label={`Revoke token ${t.name}`}
                  onClick={() => setRevoke(t)}
                >
                  Revoke
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      <ConfirmDialog
        open={!!revoke}
        danger
        title={`Revoke ${revoke?.name ?? 'token'}`}
        description="Workers using this token are refused from now on."
        confirmLabel="Revoke token"
        loading={busy}
        onCancel={() => setRevoke(null)}
        onConfirm={() => {
          const t = revoke;
          if (!t) return;
          void run(async () => {
            await tx.revokeToken(t.id);
            await res.reload();
            return true;
          }, `${t.name} revoked.`).then(() => setRevoke(null));
        }}
      />
    </>
  );
}

const ALL = '__all';
const STATE_OPTIONS = [
  { value: ALL, label: 'All states' },
  ...tx.JOB_STATES.map((s) => ({ value: s, label: s })),
];

function QueueTab() {
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

  if (unavailable) return <NotAvailableYet what="The transcode queue" />;
  return (
    <>
      <div className="ss-toolbar">
        <Select
          aria-label="Job state"
          options={STATE_OPTIONS}
          value={state}
          onValueChange={setState}
        />
        <Button variant="secondary" onClick={paged.reload}>
          Refresh
        </Button>
      </div>
      <Banner error={error} notice={notice} />
      {paged.error && (
        <LoadProblem error={paged.error} status={failure} onRetry={paged.reload} what="the queue" />
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState title="The queue is empty" description="No jobs match this filter." />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label="Transcode jobs">
            <thead>
              <tr>
                <th scope="col">Job</th>
                <th scope="col">Asset</th>
                <th scope="col">State</th>
                <th scope="col">Worker</th>
                <th scope="col">Created</th>
                <th scope="col">Actions</th>
              </tr>
            </thead>
            <tbody>
              {paged.items.map((j) => (
                <tr key={j.id}>
                  <td>
                    <span className="adm-mono">{j.id}</span>
                    <div className="ss-meta">
                      {j.kind} · attempt {j.attempt}
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
                        aria-label={`Logs for job ${j.id}`}
                        onClick={() => setLogs(j)}
                      >
                        Logs
                      </Button>
                      {(j.state === 'queued' || j.state === 'running') && (
                        <Button
                          size="sm"
                          variant="danger"
                          aria-label={`Cancel job ${j.id}`}
                          onClick={() => setCancel(j)}
                        >
                          Cancel
                        </Button>
                      )}
                      {(j.state === 'failed' || j.state === 'cancelled') && (
                        <Button
                          size="sm"
                          variant="secondary"
                          aria-label={`Re-run job ${j.id}`}
                          onClick={() => setRerun(j)}
                        >
                          Re-run
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
        title="Cancel this job"
        description="A queued job stops at once. A running one is told to stop and answers within a heartbeat."
        confirmLabel="Cancel job"
        loading={busy}
        onCancel={() => setCancel(null)}
        onConfirm={() => {
          const j = cancel;
          if (!j) return;
          void run(async () => {
            await tx.cancelJob(j.id);
            paged.reload();
            return true;
          }, 'Cancel requested.').then(() => setCancel(null));
        }}
      />
      <ReasonDialog
        open={!!rerun}
        title="Re-run the transcode"
        description="Queues a fresh ladder job for this asset from the stored original."
        confirmLabel="Queue transcode"
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
  return (
    <Modal
      open={!!job}
      onClose={onClose}
      title={`Logs for job ${job?.id ?? ''}`}
      size="lg"
      footer={<Button onClick={onClose}>Done</Button>}
    >
      {job && <JobLogBody id={job.id} />}
    </Modal>
  );
}

function JobLogBody({ id }: { id: string }) {
  const { res, unavailable } = useTx(`tx-job:${id}`, () => tx.getJob(id));
  if (unavailable) return <NotAvailableYet what="Job logs" />;
  if (res.error && !res.data)
    return <LoadProblem error={res.error} status={res.status} what="job logs" />;
  if (!res.data) return <p className="ss-loading">Loading logs…</p>;
  const j = res.data;
  return (
    <>
      {j.encoder && <p className="ss-meta">Encoder {j.encoder}</p>}
      <pre className="adm-pre" aria-label="Log output" tabIndex={0}>
        {j.log_tail?.trim() ? j.log_tail : 'No log output was kept for this job.'}
      </pre>
    </>
  );
}
