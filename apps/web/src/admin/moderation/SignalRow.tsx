// One evasion signal (MOD-SUS-003; spec section 10). A signal is a review item: it says "look at this pair" and
// nothing acts on it. The addresses behind a shared-address signal are read on demand (a logged read) and are
// shown only for what is still inside the 180-day window.
import { Badge, Button, Notice } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import * as api from '../../moderation/api';
import { ApiProblem, ReasonDialog, errorText, fmtMs } from '../ui';

const KIND_KEYS = {
  shared_login_ip: 'admin.suspicion.signalSharedIp',
  shared_invite_lineage: 'admin.suspicion.signalLineage',
  new_account_after_ban: 'admin.suspicion.signalNewAfterBan',
} as const;

const who = (a: api.AccountRef | null, fallback: string) => (a ? `@${a.username}` : fallback);

export function SignalRow({
  signal,
  canReview,
  onChanged,
}: {
  signal: api.EvasionSignal;
  canReview: boolean;
  onChanged: () => void;
}) {
  const t = useT();
  const [detail, setDetail] = useState<api.SignalDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<'dismissed' | 'confirmed' | null>(null);

  const showAddresses = async () => {
    setError(null);
    try {
      setDetail(await api.getEvasionSignal(signal.id));
    } catch (e) {
      setError(errorText(e));
    }
  };

  const relation = signal.detail.relation;
  return (
    <li className="ss-row">
      <div className="ss-grow">
        <div className="ss-name">
          {t(KIND_KEYS[signal.kind])} {who(signal.account, signal.person_id)}
          {' / '}
          {who(signal.related_account, signal.related_person_id)}{' '}
          {signal.state === 'open' ? (
            <Badge variant="warning">{t('admin.suspicion.signalOpen')}</Badge>
          ) : (
            <Badge>
              {signal.state === 'confirmed'
                ? t('admin.suspicion.signalConfirmed')
                : t('admin.suspicion.signalDismissed')}
            </Badge>
          )}
        </div>
        <div className="ss-meta">
          {fmtMs(signal.created_at_ms)}
          {signal.kind === 'shared_login_ip' &&
            ` · ${t('admin.suspicion.sharedCount', { n: Number(signal.detail.shared_ips ?? 0) })}`}
          {signal.kind === 'shared_invite_lineage' && typeof relation === 'string'
            ? ` · ${t(
                relation === 'invited_by_banned'
                  ? 'admin.suspicion.relationInvitedByBanned'
                  : 'admin.suspicion.relationSameInviter',
              )}`
            : ''}
          {signal.review_reason ? ` · ${signal.review_reason}` : ''}
        </div>
        <ApiProblem error={error} />
        {detail && (
          <div className="ss-meta">
            {detail.shared_addresses.length === 0 && !detail.snapshot ? (
              <Notice variant="info">
                {t('admin.suspicion.noAddresses', { days: detail.window_days })}
              </Notice>
            ) : null}
            {detail.snapshot && detail.shared_addresses.length === 0 ? (
              <Notice variant="info">
                {t('admin.suspicion.snapshotKept', {
                  n: Number(detail.snapshot.shared_addresses ?? 0),
                  days: Number(detail.snapshot.window_days ?? detail.window_days),
                })}
              </Notice>
            ) : null}
            {detail.shared_addresses.length > 0 && (
              <ul className="ss-list" aria-label={t('admin.suspicion.addressesLabel')}>
                {detail.shared_addresses.map((a) => (
                  <li key={a.ip} className="ss-row">
                    <span className="ss-grow">
                      {a.ip} ·{' '}
                      {t('admin.suspicion.addressSeen', {
                        mine: fmtMs(a.person_last_seen_ms),
                        theirs: fmtMs(a.related_last_seen_ms),
                      })}{' '}
                      · {t('admin.suspicion.addressPersons', { n: a.persons })}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </div>
      {signal.kind === 'shared_login_ip' && canReview && !detail && (
        <Button size="sm" variant="ghost" onClick={() => void showAddresses()}>
          {t('admin.suspicion.showAddresses')}
        </Button>
      )}
      {canReview && signal.state === 'open' && (
        <>
          <Button size="sm" variant="secondary" onClick={() => setPending('dismissed')}>
            {t('admin.suspicion.dismiss')}
          </Button>
          <Button size="sm" variant="secondary" onClick={() => setPending('confirmed')}>
            {t('admin.suspicion.confirm')}
          </Button>
        </>
      )}
      <ReasonDialog
        open={pending !== null}
        title={
          pending === 'confirmed'
            ? t('admin.suspicion.confirmTitle')
            : t('admin.suspicion.dismissTitle')
        }
        description={t('admin.suspicion.reviewDescription')}
        confirmLabel={
          pending === 'confirmed' ? t('admin.suspicion.confirm') : t('admin.suspicion.dismiss')
        }
        onCancel={() => setPending(null)}
        onConfirm={async (reason) => {
          if (!pending) return;
          await api.reviewEvasionSignal(signal.id, pending, reason);
          setPending(null);
          onChanged();
        }}
      />
    </li>
  );
}
