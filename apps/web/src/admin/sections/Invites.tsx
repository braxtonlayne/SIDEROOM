import { Badge, Button, ConfirmDialog, EmptyState, Notice, Select, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT, type MessageKey } from '../../i18n';
import {
  ActionStatus,
  Async,
  LoadMore,
  ReasonDialog,
  SectionHeader,
  StateBadge,
  fmtMs,
  useAction,
  useAdminViewer,
  usePaged,
  useResource,
} from '../ui';

const INVITE_STATES: { value: string; label: MessageKey }[] = [
  { value: '', label: 'admin.invites.stateAny' },
  { value: 'open', label: 'admin.invites.stateOpen' },
  { value: 'redeemed', label: 'admin.invites.stateRedeemed' },
  { value: 'revoked', label: 'admin.invites.stateRevoked' },
];

export function InvitesSection() {
  const t = useT();
  const stateOptions = INVITE_STATES.map((o) => ({ value: o.value, label: t(o.label) }));
  const viewer = useAdminViewer();
  const [state, setState] = useState('');
  const [issuer, setIssuer] = useState('');
  const [issuerApplied, setIssuerApplied] = useState('');
  const [revoke, setRevoke] = useState<api.InviteRow | null>(null);
  const action = useAction();
  const paged = usePaged<api.InviteRow>(`invites:${state}:${issuerApplied}`, async (after) => {
    const p = await api.listInvites({ state, issuer_id: issuerApplied, after, limit: 50 });
    return { items: p.items, next: p.next_after };
  });

  return (
    <>
      <SectionHeader
        title={t('admin.invites.title')}
        description={t('admin.invites.description')}
      />
      <Notice variant="info" className="ss-status" title={t('admin.invites.creatingTitle')}>
        {t('admin.invites.creatingLead')}{' '}
        <code>sr-cli invite create --issuer &lt;username&gt; --note &quot;why&quot;</code>.{' '}
        {t('admin.invites.creatingEmergencyPrefix')} <em>{t('admin.invites.emergencyWord')}</em>{' '}
        {t('admin.invites.creatingTail')}
      </Notice>
      <form
        className="ss-toolbar"
        role="search"
        aria-label={t('admin.invites.filterForm')}
        onSubmit={(e) => {
          e.preventDefault();
          setIssuerApplied(issuer.trim());
        }}
      >
        <TextInput
          label={t('admin.invites.issuerLabel')}
          value={issuer}
          inputMode="numeric"
          onChange={(e) => setIssuer(e.target.value)}
        />
        <Select
          aria-label={t('admin.invites.filterByState')}
          options={stateOptions}
          value={state}
          onValueChange={setState}
        />
        <Button type="submit">{t('admin.invites.filter')}</Button>
      </form>
      <ActionStatus error={action.error ?? paged.error} done={action.done} />
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={t('admin.invites.emptyTitle')}
          description={t('admin.invites.emptyDescription')}
        />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label={t('admin.invites.tableLabel')}>
            <thead>
              <tr>
                <th scope="col">{t('admin.invites.colCode')}</th>
                <th scope="col">{t('admin.invites.colIssuer')}</th>
                <th scope="col">{t('admin.invites.colState')}</th>
                <th scope="col">{t('admin.invites.colNote')}</th>
                <th scope="col">{t('admin.invites.colCreated')}</th>
                <th scope="col">
                  <span className="adm-vh">{t('admin.invites.colActions')}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {paged.items.map((i) => (
                <tr key={i.id}>
                  <td className="adm-mono">
                    {i.code}{' '}
                    {i.emergency && (
                      <Badge variant="warning">{t('admin.invites.emergencyWord')}</Badge>
                    )}
                  </td>
                  <td className="adm-mono">{i.issuer_account_id}</td>
                  <td>
                    <StateBadge state={i.state} />
                  </td>
                  <td>{i.note}</td>
                  <td>{fmtMs(i.created_at_ms)}</td>
                  <td>
                    {i.state === 'open' && viewer.can('MANAGE_INVITES') && (
                      <Button
                        size="sm"
                        variant="danger"
                        aria-label={t('admin.invites.revokeAria', { code: i.code })}
                        onClick={() => setRevoke(i)}
                      >
                        {t('admin.invites.revoke')}
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <LoadMore paged={paged} />
      <Suspicion />
      <ConfirmDialog
        open={!!revoke}
        danger
        title={t('admin.invites.revokeTitle')}
        description={t('admin.invites.revokeDescription', { code: revoke?.code ?? '' })}
        confirmLabel={t('admin.invites.revokeConfirm')}
        loading={action.busy}
        onCancel={() => setRevoke(null)}
        onConfirm={() => {
          if (!revoke) return;
          void action
            .run(
              async () => {
                await api.revokeInvite(revoke.id);
                return true;
              },
              t('admin.invites.revoked', { code: revoke.code }),
            )
            .then((ok) => {
              if (ok) {
                setRevoke(null);
                paged.reload();
              }
            });
        }}
      />
    </>
  );
}

function Suspicion() {
  const t = useT();
  const res = useResource('admin:suspicion', api.listSuspects);
  const [target, setTarget] = useState<api.SuspectPerson | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  return (
    <section className="ss-card" aria-labelledby="adm-susp-h" style={{ marginTop: 24 }}>
      <h2 id="adm-susp-h">{t('admin.invites.suspicionTitle')}</h2>
      <p className="ss-sub">{t('admin.invites.suspicionIntro')}</p>
      {notice && (
        <Notice variant="success" role="status">
          {notice}
        </Notice>
      )}
      <Async res={res} label={t('admin.invites.loadingSuspicion')}>
        {(list) =>
          list.length === 0 ? (
            <p className="ss-na">{t('admin.invites.suspicionEmpty')}</p>
          ) : (
            <ul className="ss-list" aria-label={t('admin.invites.suspiciousPeople')}>
              {list.map((p) => (
                <li key={p.person_id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">
                      {p.accounts.map((a) => `@${a.username}`).join(', ') ||
                        t('admin.invites.personFallback', { id: p.person_id })}{' '}
                      {p.frozen && (
                        <Badge variant="danger">{t('admin.invites.invitesFrozen')}</Badge>
                      )}
                    </div>
                    <div className="ss-meta">
                      {t('admin.invites.suspicionLevel', { level: p.suspicion_level })}
                    </div>
                  </div>
                  <Button size="sm" variant="secondary" onClick={() => setTarget(p)}>
                    {t('admin.invites.clearSuspicion')}
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <ReasonDialog
        open={!!target}
        title={t('admin.invites.clearSuspicion')}
        description={t('admin.invites.clearDescription')}
        confirmLabel={t('admin.invites.clearSuspicion')}
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          await api.clearSuspicion(target.person_id, reason);
          setNotice(t('admin.invites.cleared'));
          setTarget(null);
          await res.reload();
        }}
      />
    </section>
  );
}
