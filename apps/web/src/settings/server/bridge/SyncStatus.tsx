import { Button, ProgressBar } from '@sideroom/ui';
import { useEffect } from 'react';
import { formatDateTime, useT } from '../../../i18n';
import { useResource } from '../hooks';
import { ActionStatus, Async, useAction } from '../ui';
import * as api from './api';

type T = ReturnType<typeof useT>;

function stateText(s: api.SyncState, t: T): string {
  switch (s) {
    case 'seeding':
      return t('settings.bridgeSync.status.stateSeeding');
    case 'live':
      return t('settings.bridgeSync.status.stateLive');
    case 'paused':
      return t('settings.bridgeSync.status.statePaused');
    case 'paused_discord_access':
      return t('settings.bridgeSync.status.statePausedAccess');
    case 'paused_invalid_requests':
      return t('settings.bridgeSync.status.statePausedInvalid');
  }
}

/**
 * The sync status page (BRG-SYN-004, design 15): mode, state, what the bot can do on Discord, queue and limits,
 * seed progress, "Reconcile now" and Pause/Resume. Looks again every few seconds (faster while the seed runs).
 */
export function SyncStatus({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`bridge-sync-${guildId}`, () => api.getSyncStatus(guildId));
  const act = useAction();
  const reload = res.reload;
  const seeding = res.data?.sync_state === 'seeding';
  useEffect(() => {
    const timer = setTimeout(() => void reload(), seeding ? 3000 : 15000);
    return () => clearTimeout(timer);
  }, [reload, seeding, res.data]);

  const run = (fn: () => Promise<void>, done: string) =>
    void act.run(async () => {
      await fn();
      await reload();
    }, done);

  return (
    <section className="ss-card" aria-labelledby="ss-bridge-sync" data-testid="bridge-sync-status">
      <h2 id="ss-bridge-sync">{t('settings.bridgeSync.status.title')}</h2>
      <Async res={res} label={t('settings.bridgeSync.status.loading')}>
        {(s) => {
          const paused = s.sync_state.startsWith('paused');
          const wait = s.reconcile_available_at
            ? s.reconcile_available_at * 1000 > Date.now()
            : false;
          return (
            <>
              <p role="status">
                <strong>{stateText(s.sync_state, t)}</strong>
              </p>
              {s.seed && (
                <>
                  <ProgressBar
                    value={s.seed.done}
                    max={Math.max(1, s.seed.total)}
                    label={t('settings.bridgeSync.status.seedProgress', {
                      done: s.seed.done,
                      total: s.seed.total,
                    })}
                  />
                  <p className="ss-sub">
                    {t('settings.bridgeSync.status.seedProgress', {
                      done: s.seed.done,
                      total: s.seed.total,
                    })}{' '}
                    {t('settings.bridgeSync.status.seedEta', {
                      minutes: Math.ceil(s.seed.eta_secs / 60),
                    })}
                  </p>
                </>
              )}
              <dl className="ss-bridge-facts">
                <dt>{t('settings.bridgeSync.status.botAdmin')}</dt>
                <dd>{yn(s.bot.administrator, t)}</dd>
                <dt>{t('settings.bridgeSync.status.botTop')}</dt>
                <dd>{yn(s.bot.top_role, t)}</dd>
                <dt>{t('settings.bridgeSync.status.botIntents')}</dt>
                <dd>{yn(s.bot.intents, t)}</dd>
                <dt>{t('settings.bridgeSync.status.lastReconcile')}</dt>
                <dd>
                  {s.last_reconcile_at
                    ? formatDateTime(s.last_reconcile_at * 1000)
                    : t('settings.bridgeSync.status.never')}
                </dd>
                <dt>{t('settings.bridgeSync.status.queue')}</dt>
                <dd>{s.queue_depth}</dd>
                <dt>{t('settings.bridgeSync.status.dead')}</dt>
                <dd>{s.dead_ops}</dd>
                <dt>{t('settings.bridgeSync.status.rateWait')}</dt>
                <dd>
                  {s.rate_limit_wait_secs > 0
                    ? t('settings.bridgeSync.status.rateSeconds', { n: s.rate_limit_wait_secs })
                    : t('settings.bridgeSync.status.none')}
                </dd>
                <dt>{t('settings.bridgeSync.status.mapped')}</dt>
                <dd>{s.counts.mapped}</dd>
                <dt>{t('settings.bridgeSync.status.oneSided')}</dt>
                <dd>{s.counts.one_sided}</dd>
                <dt>{t('settings.bridgeSync.status.degraded')}</dt>
                <dd>{s.counts.degraded}</dd>
                <dt>{t('settings.bridgeSync.status.frozen')}</dt>
                <dd>{s.counts.frozen}</dd>
              </dl>
              <div className="ss-actions">
                <Button
                  variant="secondary"
                  disabled={act.busy || wait || paused}
                  onClick={() =>
                    run(() => api.reconcileNow(guildId), t('settings.bridgeSync.status.reconcileStarted'))
                  }
                >
                  {t('settings.bridgeSync.status.reconcile')}
                </Button>
                {s.sync_state === 'paused' ? (
                  <Button
                    disabled={act.busy}
                    onClick={() =>
                      run(() => api.resumeSync(guildId), t('settings.bridgeSync.status.resumed'))
                    }
                  >
                    {t('settings.bridgeSync.status.resume')}
                  </Button>
                ) : (
                  <Button
                    variant="secondary"
                    disabled={act.busy || paused}
                    onClick={() =>
                      run(() => api.pauseSync(guildId), t('settings.bridgeSync.status.paused'))
                    }
                  >
                    {t('settings.bridgeSync.status.pause')}
                  </Button>
                )}
              </div>
              {wait && s.reconcile_available_at && (
                <p className="ss-sub">
                  {t('settings.bridgeSync.status.reconcileSoon', {
                    when: formatDateTime(s.reconcile_available_at * 1000, {
                      hour: 'numeric',
                      minute: 'numeric',
                    }),
                  })}
                </p>
              )}
              {paused && <p className="ss-sub">{t('settings.bridgeSync.status.pausedNote')}</p>}
              <p className="ss-sub">{t('settings.bridgeSync.status.window')}</p>
            </>
          );
        }}
      </Async>
      <ActionStatus error={act.error} done={act.done} />
    </section>
  );
}

const yn = (v: boolean, t: T) =>
  v ? t('settings.bridgeSync.status.yes') : t('settings.bridgeSync.status.no');
