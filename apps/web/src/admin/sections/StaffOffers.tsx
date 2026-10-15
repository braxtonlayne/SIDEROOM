// Pending appointment offers and delegations (ADM-STF-004). An offer is a system DM the appointee answers;
// only an accepted offer creates the appointment. Delegations are Owner-only and carry a permission ceiling.
import { Badge, Button, Checkbox, ConfirmDialog, Select } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT, type MessageKey } from '../../i18n';
import { AP, OWNER_ONLY_GRANTS, bit, parsePerms, permNames, type AdminPerm } from '../permissions';
import { ActionStatus, Async, ReasonDialog, fmtMs, useAction, useResource } from '../ui';

const STATE_KEY: Record<api.OfferState, MessageKey> = {
  pending: 'admin.staffOffers.statePending',
  accepted: 'admin.staffOffers.stateAccepted',
  declined: 'admin.staffOffers.stateDeclined',
  cancelled: 'admin.staffOffers.stateCancelled',
  expired: 'admin.staffOffers.stateExpired',
};

export function OffersCard({ reloadKey }: { reloadKey: number }) {
  const t = useT();
  const offers = useResource(`admin:staff-offers:${reloadKey}`, api.listStaffOffers);
  const [withdrawing, setWithdrawing] = useState<api.StaffOffer | null>(null);
  const action = useAction();
  return (
    <section className="ss-card" aria-labelledby="adm-offers-h">
      <h2 id="adm-offers-h">{t('admin.staffOffers.title')}</h2>
      <p className="ss-sub">{t('admin.staffOffers.description')}</p>
      <ActionStatus error={action.error} />
      <Async res={offers} label={t('admin.staffOffers.loading')}>
        {(list) =>
          list.length === 0 ? (
            <p className="ss-sub">{t('admin.staffOffers.empty')}</p>
          ) : (
            <ul className="ss-list" aria-label={t('admin.staffOffers.listLabel')}>
              {list.map((o) => (
                <li key={o.id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">
                      @{o.username}{' '}
                      <Badge variant={o.state === 'pending' ? 'accent' : 'neutral'}>
                        {t(STATE_KEY[o.state])}
                      </Badge>
                    </div>
                    <div className="ss-meta">
                      {o.role_names.join(', ')} ·{' '}
                      {t('admin.staffOffers.offeredBy', { name: o.offered_by_name })} ·{' '}
                      {o.state === 'pending'
                        ? t('admin.staffOffers.expires', { when: fmtMs(o.expires_at_ms) })
                        : fmtMs(o.responded_at_ms ?? o.created_at_ms)}
                    </div>
                  </div>
                  {o.state === 'pending' && (
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={t('admin.staffOffers.withdrawAria', { username: o.username })}
                      onClick={() => setWithdrawing(o)}
                    >
                      {t('admin.staffOffers.withdraw')}
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <ConfirmDialog
        open={!!withdrawing}
        title={t('admin.staffOffers.withdrawTitle', { username: withdrawing?.username ?? '' })}
        description={t('admin.staffOffers.withdrawDescription')}
        confirmLabel={t('admin.staffOffers.withdrawConfirm')}
        loading={action.busy}
        onCancel={() => setWithdrawing(null)}
        onConfirm={() => {
          if (!withdrawing) return;
          void action
            .run(async () => {
              await api.withdrawStaffOffer(withdrawing.id);
              return true;
            })
            .then(async (ok) => {
              setWithdrawing(null);
              if (ok) await offers.reload();
            });
        }}
      />
    </section>
  );
}

/** Permissions a delegation may cover: everything except Owner and the Owner-only grants. */
const CEILING_PERMS = (Object.keys(AP) as AdminPerm[]).filter(
  (p) => p !== 'OWNER' && !OWNER_ONLY_GRANTS.includes(p),
);

export function DelegationsCard({ members }: { members: api.StaffMember[] }) {
  const t = useT();
  const list = useResource('admin:staff-delegations', api.listDelegations);
  const [open, setOpen] = useState(false);
  const [target, setTarget] = useState<string | null>(null);
  const [picked, setPicked] = useState<AdminPerm[]>([]);
  const [removing, setRemoving] = useState<api.Delegation | null>(null);
  const action = useAction();
  const candidates = members
    .filter((m) => !m.is_owner)
    .map((m) => ({ value: m.account_id, label: `@${m.username}` }));
  const mask = picked.reduce((a, p) => a | bit(p), 0n);

  return (
    <section className="ss-card" aria-labelledby="adm-deleg-h">
      <h2 id="adm-deleg-h">{t('admin.staffDelegations.title')}</h2>
      <p className="ss-sub">{t('admin.staffDelegations.description')}</p>
      <div className="adm-actions">
        <Button
          variant="secondary"
          onClick={() => {
            setTarget(null);
            setPicked([]);
            setOpen(true);
          }}
        >
          {t('admin.staffDelegations.add')}
        </Button>
      </div>
      <ActionStatus error={action.error} />
      <Async res={list} label={t('admin.staffDelegations.loading')}>
        {(rows) =>
          rows.length === 0 ? (
            <p className="ss-sub">{t('admin.staffDelegations.empty')}</p>
          ) : (
            <ul className="ss-list" aria-label={t('admin.staffDelegations.listLabel')}>
              {rows.map((d) => (
                <li key={d.account_id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">@{d.username}</div>
                    <div className="ss-meta">
                      {t('admin.staffDelegations.ceiling', {
                        perms:
                          permNames(parsePerms(d.max_permissions))
                            .join(', ')
                            .toLowerCase()
                            .replace(/_/g, ' ') || t('admin.staffDelegations.nothing'),
                      })}{' '}
                      · {fmtMs(d.granted_at_ms)}
                      {d.reason ? ` · ${d.reason}` : ''}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('admin.staffDelegations.removeAria', { username: d.username })}
                    onClick={() => setRemoving(d)}
                  >
                    {t('admin.staffDelegations.remove')}
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Async>

      <ReasonDialog
        open={open}
        title={t('admin.staffDelegations.dialogTitle')}
        description={t('admin.staffDelegations.dialogDescription')}
        confirmLabel={t('admin.staffDelegations.save')}
        valid={!!target && picked.length > 0}
        onCancel={() => setOpen(false)}
        onConfirm={async (reason) => {
          await api.setDelegation(target!, mask.toString(), reason);
          setOpen(false);
          await list.reload();
        }}
      >
        <Select
          aria-label={t('admin.staffDelegations.member')}
          options={candidates}
          value={target}
          onValueChange={setTarget}
        />
        <fieldset className="adm-check-row" style={{ border: 0, padding: 0, margin: 0 }}>
          <legend className="ss-sub">{t('admin.staffDelegations.ceilingLegend')}</legend>
          {CEILING_PERMS.map((p) => (
            <Checkbox
              key={p}
              label={p.toLowerCase().replace(/_/g, ' ')}
              checked={picked.includes(p)}
              onChange={(e) =>
                setPicked(e.target.checked ? [...picked, p] : picked.filter((x) => x !== p))
              }
            />
          ))}
        </fieldset>
      </ReasonDialog>

      <ConfirmDialog
        open={!!removing}
        danger
        title={t('admin.staffDelegations.removeTitle', { username: removing?.username ?? '' })}
        description={t('admin.staffDelegations.removeDescription')}
        confirmLabel={t('admin.staffDelegations.removeConfirm')}
        loading={action.busy}
        onCancel={() => setRemoving(null)}
        onConfirm={() => {
          if (!removing) return;
          void action
            .run(async () => {
              await api.removeDelegation(removing.account_id);
              return true;
            })
            .then(async (ok) => {
              setRemoving(null);
              if (ok) await list.reload();
            });
        }}
      />
    </section>
  );
}
