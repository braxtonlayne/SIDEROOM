import { Badge, Button, ConfirmDialog, EmptyState, Select } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { formatNumber, type MessageKey, useT } from '../../i18n';
import { formatBytes } from '../../lounge/msg/format';
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

const INTEGRATION_LABELS: Record<string, MessageKey> = {
  smtp: 'admin.system.integrationSmtp',
  discord_oauth: 'admin.system.integrationDiscordOauth',
  voice: 'admin.system.integrationVoice',
  turn: 'admin.system.integrationTurn',
  gif: 'admin.system.integrationGif',
  push: 'admin.system.integrationPush',
  connections: 'admin.system.integrationConnections',
};

export function SystemSection() {
  const t = useT();
  const res = useResource('admin:system', api.getSystem);
  const mail = useAction();
  const [sentTo, setSentTo] = useState<string | null>(null);
  return (
    <>
      <SectionHeader
        title={t('admin.system.title')}
        description={t('admin.system.description')}
        actions={
          <Button variant="secondary" onClick={() => void res.reload()}>
            {t('admin.system.refresh')}
          </Button>
        }
      />
      <Async res={res} label={t('admin.system.loading')}>
        {(s) => (
          <>
            <div className="adm-stats">
              <Stat label={t('admin.system.version')} value={s.version} />
              <Stat
                label={t('admin.system.gatewayConnections')}
                value={s.gateway_sessions == null ? '–' : formatNumber(s.gateway_sessions)}
              />
              <Stat
                label={t('admin.system.jobsQueued')}
                value={s.counts.jobs_queued}
                hint={t('admin.system.jobsRunning', { count: s.counts.jobs_running })}
              />
              <Stat label={t('admin.system.deadJobs')} value={s.counts.jobs_dead} />
            </div>
            <section className="ss-card" aria-labelledby="sys-probes">
              <h2 id="sys-probes">{t('admin.system.probesTitle')}</h2>
              <ul className="ss-list" aria-label={t('admin.system.probesTitle')}>
                {s.probes.map((p) => (
                  <li key={p.name} className="ss-row">
                    <div className="ss-grow">
                      <div className="ss-name">{p.name}</div>
                      {!p.ok && p.error && <div className="ss-meta">{p.error}</div>}
                    </div>
                    <Badge variant={p.ok ? 'success' : 'danger'}>
                      {p.ok ? t('admin.system.probeOk') : t('admin.system.probeFailing')}
                    </Badge>
                  </li>
                ))}
              </ul>
            </section>
            {s.backups && (
              <section className="ss-card" aria-labelledby="sys-backups">
                <h2 id="sys-backups">{t('admin.system.backupsTitle')}</h2>
                <ul className="ss-list" aria-label={t('admin.system.backupsTitle')}>
                  {(
                    [
                      ['admin.system.backupLastRun', s.backups.last_run_ms],
                      ['admin.system.backupLastVerify', s.backups.last_verify_ms],
                      ['admin.system.backupSnapshot', s.backups.snapshot_ms],
                    ] as const
                  ).map(([label, at]) => (
                    <li key={label} className="ss-row">
                      <div className="ss-grow">
                        <div className="ss-name">{t(label)}</div>
                      </div>
                      <span className="ss-meta">
                        {at ? fmtMs(at) : t('admin.system.backupsNone')}
                      </span>
                    </li>
                  ))}
                </ul>
              </section>
            )}
            {s.disk && (
              <section className="ss-card" aria-labelledby="sys-disk">
                <h2 id="sys-disk">{t('admin.system.diskTitle')}</h2>
                {s.disk.length === 0 ? (
                  <p className="ss-meta">{t('admin.system.diskNone')}</p>
                ) : (
                  <ul className="ss-list" aria-label={t('admin.system.diskTitle')}>
                    {s.disk.map((d) => (
                      <li key={d.path} className="ss-row">
                        <div className="ss-grow">
                          <div className="ss-name">{d.path}</div>
                        </div>
                        <span className="ss-meta">
                          {d.total_bytes == null || d.free_bytes == null
                            ? t('admin.system.diskUnreadable')
                            : t('admin.system.diskFree', {
                                free: formatBytes(d.free_bytes),
                                total: formatBytes(d.total_bytes),
                              })}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            )}
            {s.integrations && s.integrations.length > 0 && (
              <section className="ss-card" aria-labelledby="sys-integrations">
                <h2 id="sys-integrations">{t('admin.system.integrationsTitle')}</h2>
                <ul className="ss-list" aria-label={t('admin.system.integrationsTitle')}>
                  {s.integrations.map((i) => (
                    <li key={i.id} className="ss-row">
                      <div className="ss-grow">
                        <div className="ss-name">
                          {INTEGRATION_LABELS[i.id] ? t(INTEGRATION_LABELS[i.id]!) : i.id}
                        </div>
                        {i.detail && <div className="ss-meta">{i.detail}</div>}
                      </div>
                      <Badge variant={i.configured ? 'success' : 'neutral'}>
                        {i.configured
                          ? t('admin.system.integrationConfigured')
                          : t('admin.system.integrationMissing')}
                      </Badge>
                      {i.id === 'smtp' && i.configured && (
                        <Button
                          size="sm"
                          variant="secondary"
                          disabled={mail.busy}
                          onClick={() => {
                            void mail
                              .run(() => api.sendEmailTest())
                              .then((r) => {
                                if (r) setSentTo(r.sent_to);
                              });
                          }}
                        >
                          {t('admin.system.sendTestEmail')}
                        </Button>
                      )}
                    </li>
                  ))}
                </ul>
                <ActionStatus
                  error={mail.error}
                  done={sentTo ? t('admin.system.testEmailSent', { to: sentTo }) : null}
                />
              </section>
            )}
          </>
        )}
      </Async>
    </>
  );
}

const JOB_STATES: { value: string; label: MessageKey }[] = [
  { value: '', label: 'admin.jobs.stateAny' },
  { value: 'queued', label: 'admin.jobs.stateQueued' },
  { value: 'running', label: 'admin.jobs.stateRunning' },
  { value: 'dead', label: 'admin.jobs.stateDead' },
  { value: 'done', label: 'admin.jobs.stateDone' },
];

export function JobsSection() {
  const t = useT();
  const stateOptions = JOB_STATES.map((o) => ({ value: o.value, label: t(o.label) }));
  const [state, setState] = useState('dead');
  const [del, setDel] = useState<api.JobRow | null>(null);
  const action = useAction();
  const paged = usePaged<api.JobRow>(`jobs:${state}`, async (after) => {
    const p = await api.listJobs({ state, after, limit: 50 });
    return { items: p.items, next: p.next_after };
  });
  return (
    <>
      <SectionHeader title={t('admin.jobs.title')} description={t('admin.jobs.description')} />
      <div className="ss-toolbar">
        <Select
          aria-label={t('admin.jobs.filterByState')}
          options={stateOptions}
          value={state}
          onValueChange={setState}
        />
      </div>
      <ActionStatus error={action.error ?? paged.error} done={action.done} />
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={t('admin.jobs.emptyTitle')}
          description={t('admin.jobs.emptyDescription')}
        />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label={t('admin.jobs.tableLabel')}>
            <thead>
              <tr>
                <th scope="col">{t('admin.jobs.colJob')}</th>
                <th scope="col">{t('admin.jobs.colState')}</th>
                <th scope="col">{t('admin.jobs.colAttempts')}</th>
                <th scope="col">{t('admin.jobs.colNextRun')}</th>
                <th scope="col">{t('admin.jobs.colLastError')}</th>
                <th scope="col">
                  <span className="adm-vh">{t('admin.jobs.colActions')}</span>
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
                          aria-label={t('admin.jobs.retryAria', { id: j.id })}
                          onClick={() =>
                            void action
                              .run(() => api.retryJob(j.id), t('admin.jobs.requeued'))
                              .then(() => paged.reload())
                          }
                        >
                          {t('admin.jobs.retry')}
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          aria-label={t('admin.jobs.deleteAria', { id: j.id })}
                          onClick={() => setDel(j)}
                        >
                          {t('admin.jobs.delete')}
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
        title={t('admin.jobs.deleteTitle')}
        description={t('admin.jobs.deleteDescription')}
        confirmLabel={t('admin.jobs.deleteConfirm')}
        loading={action.busy}
        onCancel={() => setDel(null)}
        onConfirm={() => {
          if (!del) return;
          void action
            .run(() => api.deleteJob(del.id), t('admin.jobs.deleted'))
            .then(() => {
              setDel(null);
              paged.reload();
            });
        }}
      />
    </>
  );
}
