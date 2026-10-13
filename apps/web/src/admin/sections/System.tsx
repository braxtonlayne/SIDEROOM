import { Badge, Button, ConfirmDialog, EmptyState, Notice, Select } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import {
  ActionStatus,
  Async,
  LoadMore,
  SectionHeader,
  Stat,
  StateBadge,
  fmtMs,
  useAction,
  usePaged,
  useResource,
} from '../ui';

export function SystemSection() {
  const res = useResource('admin:system', api.getSystem);
  return (
    <>
      <SectionHeader
        title="System"
        description="Version and readiness probes. Probe errors name the dependency, never its credentials."
        actions={
          <Button variant="secondary" onClick={() => void res.reload()}>
            Refresh
          </Button>
        }
      />
      <Async res={res} label="Loading system status">
        {(s) => (
          <>
            <div className="adm-stats">
              <Stat label="Version" value={s.version} />
              <Stat
                label="Gateway connections"
                value={s.gateway_sessions == null ? '–' : s.gateway_sessions.toLocaleString()}
              />
              <Stat
                label="Jobs queued"
                value={s.counts.jobs_queued}
                hint={`${s.counts.jobs_running} running`}
              />
              <Stat label="Dead jobs" value={s.counts.jobs_dead} />
            </div>
            <section className="ss-card" aria-labelledby="sys-probes">
              <h2 id="sys-probes">Readiness probes</h2>
              <ul className="ss-list" aria-label="Readiness probes">
                {s.probes.map((p) => (
                  <li key={p.name} className="ss-row">
                    <div className="ss-grow">
                      <div className="ss-name">{p.name}</div>
                      {!p.ok && p.error && <div className="ss-meta">{p.error}</div>}
                    </div>
                    <Badge variant={p.ok ? 'success' : 'danger'}>{p.ok ? 'ok' : 'failing'}</Badge>
                  </li>
                ))}
              </ul>
            </section>
          </>
        )}
      </Async>
    </>
  );
}

const JOB_STATES = [
  { value: '', label: 'Any state' },
  { value: 'queued', label: 'Queued' },
  { value: 'running', label: 'Running' },
  { value: 'dead', label: 'Dead' },
  { value: 'done', label: 'Done' },
];

export function JobsSection() {
  const [state, setState] = useState('dead');
  const [del, setDel] = useState<api.JobRow | null>(null);
  const action = useAction();
  const paged = usePaged<api.JobRow>(`jobs:${state}`, async (after) => {
    const p = await api.listJobs({ state, after, limit: 50 });
    return { items: p.items, next: p.next_after };
  });
  return (
    <>
      <SectionHeader
        title="Jobs"
        description="Background work (transcoding, ingest, cleanup). Retry a dead job once its cause is fixed, or delete it."
      />
      <div className="ss-toolbar">
        <Select
          aria-label="Filter by job state"
          options={JOB_STATES}
          value={state}
          onValueChange={setState}
        />
      </div>
      <ActionStatus error={action.error ?? paged.error} done={action.done} />
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState title="No jobs" description="Nothing matches this filter." />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label="Jobs">
            <thead>
              <tr>
                <th scope="col">Job</th>
                <th scope="col">State</th>
                <th scope="col">Attempts</th>
                <th scope="col">Next run</th>
                <th scope="col">Last error</th>
                <th scope="col">
                  <span className="adm-vh">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {paged.items.map((j) => (
                <tr key={j.id}>
                  <td>
                    {j.kind}
                    <div className="ss-meta adm-mono">{j.id}</div>
                  </td>
                  <td>
                    <StateBadge state={j.state} />
                  </td>
                  <td>
                    {j.attempts}/{j.max_attempts}
                  </td>
                  <td>{fmtMs(j.run_at_ms)}</td>
                  <td>{j.last_error}</td>
                  <td>
                    {j.state === 'dead' && (
                      <div className="adm-actions">
                        <Button
                          size="sm"
                          variant="secondary"
                          aria-label={`Retry job ${j.id}`}
                          onClick={() =>
                            void action
                              .run(() => api.retryJob(j.id), 'Job re-queued.')
                              .then(() => paged.reload())
                          }
                        >
                          Retry
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          aria-label={`Delete job ${j.id}`}
                          onClick={() => setDel(j)}
                        >
                          Delete
                        </Button>
                      </div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <LoadMore paged={paged} />
      <ConfirmDialog
        open={!!del}
        danger
        title="Delete this dead job"
        description="Its record is removed; the work is not retried."
        confirmLabel="Delete job"
        loading={action.busy}
        onCancel={() => setDel(null)}
        onConfirm={() => {
          if (!del) return;
          void action
            .run(() => api.deleteJob(del.id), 'Job deleted.')
            .then(() => {
              setDel(null);
              paged.reload();
            });
        }}
      />
    </>
  );
}

export function ReviewQueuesSection() {
  return (
    <>
      <SectionHeader
        title="Review queues"
        description="Cinema, Library, Apps and Packages submissions land here."
      />
      <Notice variant="info" title="Not built yet">
        The submissions backend (docs/specs/submissions-and-review.md) has no endpoints in this
        release, so there is nothing to review here yet. Your role already carries the review
        permissions; the tabs will appear when the queues ship.
      </Notice>
    </>
  );
}
