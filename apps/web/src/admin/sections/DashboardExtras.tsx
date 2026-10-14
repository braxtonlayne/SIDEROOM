// Dashboard tiles built from the routes that exist today (ADM-DASH-001): review queue counts, donations
// against the hosting cost, and recent alerts. Each tile loads on its own and is simply absent when the staff
// member lacks the permission or the call fails, so one broken backend never blanks the dashboard.
import { Badge, Button, ProgressBar } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import * as api from '../api';
import * as donations from '../donations/api';
import * as review from '../../review/api';
import { formatNumber, useT } from '../../i18n';
import { Stat, useAdminViewer, useResource } from '../ui';

const n = (v: number) => formatNumber(v);

/** First of the current calendar month, in UTC milliseconds. */
export const monthStartMs = (now = Date.now()) => {
  const d = new Date(now);
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), 1);
};

const settle = async <T,>(p: Promise<T>): Promise<T | null> => {
  try {
    return await p;
  } catch {
    return null;
  }
};

export function ReviewTiles() {
  const t = useT();
  const v = useAdminViewer();
  const counts = useResource(
    `dash:review:${v.perms}`,
    async (): Promise<{ library: number | null; apps: number | null; cinema: number | null }> => {
      const [library, requests, versions, reports] = await Promise.all([
        v.can('REVIEW_READING') || v.can('REVIEW_MEDIA')
          ? settle(review.reviewQueue({ limit: 1 }))
          : null,
        v.can('REVIEW_APPS') ? settle(api.listDeveloperRequests({ state: 'pending' })) : null,
        v.can('REVIEW_APPS') ? settle(api.listAppVersions({ state: 'pending' })) : null,
        v.can('DELETE_CONTENT') ? settle(api.listCinemaReports({ status: 'open' })) : null,
      ]);
      return {
        library: library ? library.total : null,
        apps: requests || versions ? (requests?.length ?? 0) + (versions?.length ?? 0) : null,
        cinema: reports ? reports.items.length : null,
      };
    },
  );
  const c = counts.data;
  if (!c || (c.library === null && c.apps === null && c.cinema === null)) return null;
  return (
    <div className="adm-stats" role="group" aria-label={t('admin.dashboardExtras.reviewGroup')}>
      {c.library !== null && (
        <Stat label={t('admin.dashboardExtras.reviewLibrary')} value={n(c.library)} />
      )}
      {c.apps !== null && <Stat label={t('admin.dashboardExtras.reviewApps')} value={n(c.apps)} />}
      {c.cinema !== null && (
        <Stat label={t('admin.dashboardExtras.reviewCinema')} value={n(c.cinema)} />
      )}
    </div>
  );
}

const usd = (cents: number) => formatNumber(cents / 100, { style: 'currency', currency: 'USD' });

export function DonationsTile() {
  const t = useT();
  const v = useAdminViewer();
  const res = useResource(`dash:donations:${v.can('MANAGE_DONATIONS')}`, async () => {
    if (!v.can('MANAGE_DONATIONS')) return null;
    const [cfg, report] = await Promise.all([
      settle(donations.getConfig()),
      settle(donations.getReport({ from_ms: monthStartMs() })),
    ]);
    return cfg && report ? { cost: cfg.hosting_cost_cents, credited: report.credited_cents } : null;
  });
  const d = res.data;
  if (!d) return null;
  return (
    <section className="ss-card adm-gauge" aria-labelledby="adm-don-h">
      <h2 id="adm-don-h">{t('admin.dashboardExtras.donationsTitle')}</h2>
      {d.cost > 0 ? (
        <>
          <ProgressBar
            label={t('admin.dashboardExtras.donationsGauge')}
            value={Math.min(d.credited, d.cost)}
            max={d.cost}
          />
          <p className="ss-sub">
            {t('admin.dashboardExtras.donationsSummary', {
              credited: usd(d.credited),
              cost: usd(d.cost),
            })}
          </p>
        </>
      ) : (
        <p className="ss-sub">{t('admin.dashboardExtras.noCost', { credited: usd(d.credited) })}</p>
      )}
    </section>
  );
}

interface Alert {
  id: string;
  text: string;
}

export function AlertsPanel({ deadJobs }: { deadJobs: number }) {
  const t = useT();
  const v = useAdminViewer();
  const res = useResource(`dash:alerts:${v.perms}:${deadJobs}`, async (): Promise<Alert[]> => {
    const out: Alert[] = [];
    if (deadJobs > 0)
      out.push({ id: 'jobs', text: t('admin.dashboardExtras.alertDeadJobs', { count: deadJobs }) });
    const [system, status, chain] = await Promise.all([
      v.can('OPERATOR') ? settle(api.getSystem()) : null,
      v.can('MANAGE_DONATIONS') ? settle(donations.getStatus()) : null,
      v.can('VIEW_ACCESS_LOG') ? settle(api.verifyAccessLog()) : null,
    ]);
    for (const p of system?.probes ?? [])
      if (!p.ok)
        out.push({
          id: `probe:${p.name}`,
          text: t('admin.dashboardExtras.alertProbe', { name: p.name, error: p.error ?? '' }),
        });
    if (status && status.needs_attention > 0)
      out.push({
        id: 'donations',
        text: t('admin.dashboardExtras.alertDonations', { count: status.needs_attention }),
      });
    if (chain && !chain.ok)
      out.push({
        id: 'chain',
        text: t('admin.dashboardExtras.alertChain', { seq: chain.first_bad_seq ?? 0 }),
      });
    return out;
  });
  if (!res.data) return null;
  return (
    <section className="ss-card adm-gauge" aria-labelledby="adm-alerts-h">
      <h2 id="adm-alerts-h">{t('admin.dashboardExtras.alertsTitle')}</h2>
      {res.data.length === 0 ? (
        <p className="ss-sub">{t('admin.dashboardExtras.noAlerts')}</p>
      ) : (
        <ul className="ss-list" aria-label={t('admin.dashboardExtras.alertsTitle')}>
          {res.data.map((a) => (
            <li key={a.id} className="ss-row">
              <Badge variant="danger">{t('admin.dashboardExtras.alertBadge')}</Badge>
              <span className="ss-grow">{a.text}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

type StepState = 'done' | 'todo' | 'unknown';
interface Step {
  id: string;
  label: string;
  detail: string;
  state: StepState;
  /** Admin section that fixes it, when one exists. */
  section?: string;
}

/**
 * Setup checklist (ADM-DASH-003). Only checks what the server can already report: object storage, the wallet,
 * limits left at their defaults and a second staff member. SMTP, Discord sign-in and voice/TURN have no status
 * route yet, so they are listed as unknown instead of pretending to pass.
 */
export function SetupChecklist({ staff }: { staff: number }) {
  const t = useT();
  const v = useAdminViewer();
  const navigate = useNavigate();
  const res = useResource(`dash:setup:${v.perms}:${staff}`, async (): Promise<Step[]> => {
    const [system, status, settings] = await Promise.all([
      v.can('OPERATOR') ? settle(api.getSystem()) : null,
      v.can('MANAGE_DONATIONS') ? settle(donations.getStatus()) : null,
      v.can('MANAGE_FEATURES') ? settle(api.listSettings()) : null,
    ]);
    const steps: Step[] = [];
    if (system) {
      const storage = system.probes.find((p) => /storage|garage|s3/i.test(p.name));
      steps.push({
        id: 'storage',
        label: t('admin.setup.storage'),
        detail: storage?.ok ? t('admin.setup.storageOk') : t('admin.setup.storageBad'),
        state: storage ? (storage.ok ? 'done' : 'todo') : 'unknown',
        section: 'system',
      });
    }
    if (status)
      steps.push({
        id: 'wallet',
        label: t('admin.setup.wallet'),
        detail: status.wallet ? t('admin.setup.walletOk') : t('admin.setup.walletMissing'),
        state: status.wallet ? 'done' : 'todo',
        section: 'donation-pricing',
      });
    if (settings) {
      const changed = settings.filter((s) => !s.is_default).length;
      steps.push({
        id: 'limits',
        label: t('admin.setup.limits'),
        detail:
          changed > 0
            ? t('admin.setup.limitsChanged', { count: changed })
            : t('admin.setup.limitsDefault'),
        state: changed > 0 ? 'done' : 'todo',
        section: 'limits',
      });
    }
    steps.push({
      id: 'staff',
      label: t('admin.setup.staff'),
      detail: t('admin.setup.staffCount', { count: staff }),
      state: staff > 1 ? 'done' : 'todo',
      section: 'staff',
    });
    for (const [id, label] of [
      ['smtp', t('admin.setup.smtp')],
      ['discord', t('admin.setup.discord')],
      ['voice', t('admin.setup.voice')],
    ] as const)
      steps.push({ id, label, detail: t('admin.setup.noStatusRoute'), state: 'unknown' });
    return steps;
  });
  if (!res.data || res.data.length === 0) return null;
  const done = res.data.filter((s) => s.state === 'done').length;
  return (
    <section className="ss-card adm-gauge" aria-labelledby="adm-setup-h">
      <h2 id="adm-setup-h">{t('admin.setup.title')}</h2>
      <p className="ss-sub">{t('admin.setup.progress', { done, total: res.data.length })}</p>
      <ul className="ss-list" aria-label={t('admin.setup.title')}>
        {res.data.map((s) => (
          <li key={s.id} className="ss-row">
            <div className="ss-grow">
              <div className="ss-name">
                {s.label}{' '}
                <Badge
                  variant={
                    s.state === 'done' ? 'success' : s.state === 'todo' ? 'warning' : 'neutral'
                  }
                >
                  {s.state === 'done'
                    ? t('admin.setup.stateDone')
                    : s.state === 'todo'
                      ? t('admin.setup.stateTodo')
                      : t('admin.setup.stateUnknown')}
                </Badge>
              </div>
              <div className="ss-meta">{s.detail}</div>
            </div>
            {s.section && s.state !== 'done' && (
              <Button
                size="sm"
                variant="secondary"
                aria-label={t('admin.setup.openAria', { label: s.label })}
                onClick={() =>
                  void navigate({ to: '/admin/$section', params: { section: s.section! } })
                }
              >
                {t('admin.setup.open')}
              </Button>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
