import { Notice, ProgressBar } from '@sideroom/ui';
import * as api from '../api';
import { Async, SectionHeader, Stat, useAdminViewer, useResource } from '../ui';

const n = (v: number) => v.toLocaleString();

export function DashboardSection() {
  const viewer = useAdminViewer();
  const stats = useResource('admin:stats', api.getStats);
  // The cap lives in platform settings (MANAGE_FEATURES); without that permission the gauge is simply absent.
  const cap = useResource(
    `admin:cap:${viewer.can('MANAGE_FEATURES')}`,
    async (): Promise<number | null> => {
      if (!viewer.can('MANAGE_FEATURES')) return null;
      const all = await api.listSettings();
      const c = all.find((s) => s.key === 'accounts.cap');
      return typeof c?.value === 'number' ? c.value : null;
    },
  );
  const system = useResource(
    `admin:dash-system:${viewer.can('OPERATOR')}`,
    async (): Promise<api.SystemView | null> => (viewer.can('OPERATOR') ? api.getSystem() : null),
  );

  return (
    <>
      <SectionHeader
        title="Dashboard"
        description="A read-only snapshot of the installation. Counts are live, not cached."
      />
      <Async res={stats} label="Loading dashboard">
        {(s) => (
          <>
            <div className="adm-stats">
              <Stat
                label="Active accounts"
                value={n(s.accounts_active)}
                hint={cap.data != null ? <>of {n(cap.data)} allowed</> : undefined}
              />
              <Stat label="Sessions signed in" value={n(s.sessions_active)} />
              {system.data && (
                <Stat
                  label="Online now"
                  value={
                    system.data.gateway_sessions == null ? '–' : n(system.data.gateway_sessions)
                  }
                  hint="Open gateway connections"
                />
              )}
              <Stat label="Deactivated" value={n(s.accounts_deactivated)} />
              <Stat label="Platform-banned" value={n(s.accounts_banned)} />
              <Stat label="Staff" value={n(s.staff)} />
              <Stat
                label="Jobs queued"
                value={n(s.jobs_queued)}
                hint={`${n(s.jobs_running)} running`}
              />
              <Stat
                label="Dead jobs"
                value={n(s.jobs_dead)}
                hint={s.jobs_dead > 0 ? 'Needs attention' : 'None'}
              />
            </div>
            {cap.data != null && (
              <section className="ss-card adm-gauge" aria-labelledby="adm-cap-h">
                <h2 id="adm-cap-h">Accounts against the cap</h2>
                <ProgressBar
                  label="Active accounts against the account cap"
                  value={s.accounts_active}
                  max={cap.data}
                />
                <p className="ss-sub">
                  {n(s.accounts_active)} of {n(cap.data)} accounts. Reactivating a suspended account
                  is refused once the cap is reached.
                </p>
              </section>
            )}
          </>
        )}
      </Async>
      <Notice variant="info" className="ss-status" title="Not tracked here yet">
        Voice rooms, egress, storage, review-queue sizes and donations need their own backends; they
        will appear as those phases land.
      </Notice>
    </>
  );
}
