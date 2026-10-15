import { Notice, ProgressBar } from '@sideroom/ui';
import * as api from '../api';
import { formatNumber, useT } from '../../i18n';
import { AlertsPanel, DonationsTile, ReviewTiles, SetupChecklist } from './DashboardExtras';
import { Async, SectionHeader, Stat, useAdminViewer, useResource } from '../ui';

const n = (v: number) => formatNumber(v);

const BYTE_UNITS = ['byte', 'kilobyte', 'megabyte', 'gigabyte', 'terabyte'] as const;
/** 1536 -> "1.5 kB" in the viewer's locale. */
export const bytes = (v: number) => {
  let i = 0;
  let x = Math.max(0, v);
  while (x >= 1024 && i < BYTE_UNITS.length - 1) {
    x /= 1024;
    i += 1;
  }
  return formatNumber(x, { style: 'unit', unit: BYTE_UNITS[i]!, maximumFractionDigits: 1 });
};

export function DashboardSection() {
  const t = useT();
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
        title={t('admin.dashboard.title')}
        description={t('admin.dashboard.description')}
      />
      <Async res={stats} label={t('admin.dashboard.loading')}>
        {(s) => (
          <>
            <div className="adm-stats">
              <Stat
                label={t('admin.dashboard.activeAccounts')}
                value={n(s.accounts_active)}
                hint={
                  cap.data != null
                    ? t('admin.dashboard.activeAccountsHint', { cap: n(cap.data) })
                    : undefined
                }
              />
              <Stat label={t('admin.dashboard.sessionsSignedIn')} value={n(s.sessions_active)} />
              {system.data && (
                <Stat
                  label={t('admin.dashboard.onlineNow')}
                  value={
                    system.data.gateway_sessions == null ? '–' : n(system.data.gateway_sessions)
                  }
                  hint={t('admin.dashboard.onlineNowHint')}
                />
              )}
              <Stat label={t('admin.dashboard.deactivated')} value={n(s.accounts_deactivated)} />
              <Stat label={t('admin.dashboard.platformBanned')} value={n(s.accounts_banned)} />
              <Stat label={t('admin.dashboard.staff')} value={n(s.staff)} />
              {s.storage_bytes != null && (
                <Stat label={t('admin.dashboard.storageUsed')} value={bytes(s.storage_bytes)} />
              )}
              {s.invites_open != null && (
                <Stat label={t('admin.dashboard.invitesOpen')} value={n(s.invites_open)} />
              )}
              {s.voice_rooms != null && (
                <Stat label={t('admin.dashboard.voiceRooms')} value={n(s.voice_rooms)} />
              )}
              {s.egress_bytes != null && (
                <Stat label={t('admin.dashboard.voiceEgress')} value={bytes(s.egress_bytes)} />
              )}
              <Stat
                label={t('admin.dashboard.jobsQueued')}
                value={n(s.jobs_queued)}
                hint={t('admin.dashboard.jobsRunning', { count: n(s.jobs_running) })}
              />
              <Stat
                label={t('admin.dashboard.deadJobs')}
                value={n(s.jobs_dead)}
                hint={
                  s.jobs_dead > 0 ? t('admin.dashboard.needsAttention') : t('admin.dashboard.none')
                }
              />
            </div>
            <ReviewTiles />
            {cap.data != null && (
              <section className="ss-card adm-gauge" aria-labelledby="adm-cap-h">
                <h2 id="adm-cap-h">{t('admin.dashboard.capTitle')}</h2>
                <ProgressBar
                  label={t('admin.dashboard.capGaugeLabel')}
                  value={s.accounts_active}
                  max={cap.data}
                />
                <p className="ss-sub">
                  {t('admin.dashboard.capSummary', {
                    active: n(s.accounts_active),
                    cap: n(cap.data),
                  })}
                </p>
              </section>
            )}
            <DonationsTile />
            <AlertsPanel deadJobs={s.jobs_dead} />
            {(viewer.can('OPERATOR') || viewer.can('MANAGE_FEATURES')) && (
              <SetupChecklist staff={s.staff} />
            )}
          </>
        )}
      </Async>
      <Notice
        variant="info"
        className="ss-status"
        title={t('admin.dashboardExtras.notTrackedTitle')}
      >
        {t('admin.dashboardExtras.notTracked')}
      </Notice>
    </>
  );
}
