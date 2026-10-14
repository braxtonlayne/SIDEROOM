import { Button, Drawer, EmptyState, Notice, Select, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import * as api from '../api';
import { useT, type MessageKey } from '../../i18n';
import {
  Async,
  LoadMore,
  ReasonDialog,
  SectionHeader,
  StateBadge,
  fmtMs,
  useAdminViewer,
  usePaged,
  useResource,
} from '../ui';

const STATE_OPTIONS: { value: string; label: MessageKey }[] = [
  { value: '', label: 'admin.accounts.stateAny' },
  { value: 'active', label: 'admin.accounts.stateActive' },
  { value: 'deactivated', label: 'admin.accounts.stateDeactivated' },
  { value: 'platform_banned', label: 'admin.accounts.statePlatformBanned' },
  { value: 'deleted', label: 'admin.accounts.stateDeleted' },
];

export function AccountsSection() {
  const t = useT();
  const stateOptions = STATE_OPTIONS.map((o) => ({ value: o.value, label: t(o.label) }));
  const [draft, setDraft] = useState({ q: '', state: '' });
  const [applied, setApplied] = useState({ q: '', state: '' });
  const [open, setOpen] = useState<string | null>(null);
  const paged = usePaged<api.AccountSummary>(
    `accounts:${applied.q}:${applied.state}`,
    async (after) => {
      const p = await api.searchAccounts({ ...applied, after, limit: 50 });
      return { items: p.items, next: p.next_after };
    },
  );
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setApplied({ q: draft.q.trim(), state: draft.state });
  };

  return (
    <>
      <SectionHeader
        title={t('admin.accounts.title')}
        description={t('admin.accounts.description')}
      />
      <form
        className="ss-toolbar"
        onSubmit={submit}
        role="search"
        aria-label={t('admin.accounts.searchForm')}
      >
        <div className="ss-search">
          <TextInput
            label={t('admin.accounts.searchLabel')}
            type="search"
            value={draft.q}
            placeholder={t('admin.accounts.searchPlaceholder')}
            onChange={(e) => setDraft({ ...draft, q: e.target.value })}
          />
        </div>
        <Select
          aria-label={t('admin.accounts.filterByState')}
          options={stateOptions}
          value={draft.state}
          onValueChange={(state) => {
            setDraft({ ...draft, state });
            setApplied({ q: draft.q.trim(), state });
          }}
        />
        <Button type="submit">{t('admin.accounts.search')}</Button>
      </form>
      {paged.error && (
        <Notice variant="danger" className="ss-status" role="alert">
          {paged.error}
        </Notice>
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={t('admin.accounts.emptyTitle')}
          description={t('admin.accounts.emptyDescription')}
        />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label={t('admin.accounts.tableLabel')}>
            <thead>
              <tr>
                <th scope="col">{t('admin.accounts.colAccount')}</th>
                <th scope="col">{t('admin.accounts.colEmail')}</th>
                <th scope="col">{t('admin.accounts.colState')}</th>
                <th scope="col">{t('admin.accounts.colJoined')}</th>
                <th scope="col">
                  <span className="adm-vh">{t('admin.accounts.colActions')}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {paged.items.map((a) => (
                <tr key={a.id}>
                  <td>
                    <strong>{a.display_name || a.username}</strong>
                    <div className="ss-meta">@{a.username}</div>
                  </td>
                  <td>
                    {a.email}
                    {!a.email_verified && (
                      <div className="ss-meta">{t('admin.accounts.unverified')}</div>
                    )}
                  </td>
                  <td>
                    <StateBadge state={a.state} />
                  </td>
                  <td>{fmtMs(a.created_at_ms)}</td>
                  <td>
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={t('admin.accounts.viewAria', { username: a.username })}
                      onClick={() => setOpen(a.id)}
                    >
                      {t('admin.accounts.view')}
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <LoadMore paged={paged} />
      {open && (
        <AccountDrawer id={open} onClose={() => setOpen(null)} onChanged={() => paged.reload()} />
      )}
    </>
  );
}

type Pending =
  | { kind: 'suspend' }
  | { kind: 'unsuspend' }
  | { kind: 'logout' }
  | { kind: 'reset2fa' }
  | { kind: 'ban'; category: api.BanCategory }
  | { kind: 'credits' }
  | null;

function AccountDrawer({
  id,
  onClose,
  onChanged,
}: {
  id: string;
  onClose: () => void;
  onChanged: () => void;
}) {
  const t = useT();
  const viewer = useAdminViewer();
  const res = useResource(`account:${id}`, () => api.getAccount(id));
  const credits = useResource(`credits:${id}:${viewer.can('MANAGE_INVITES')}`, async () =>
    viewer.can('MANAGE_INVITES') ? api.getCredits(id) : null,
  );
  const tree = useResource(`tree:${id}:${viewer.can('MANAGE_INVITES')}`, async () =>
    viewer.can('MANAGE_INVITES') ? api.getInviteTree(id) : null,
  );
  const [pending, setPending] = useState<Pending>(null);
  const [delta, setDelta] = useState('1');
  const d = res.data;

  const [notice, setNotice] = useState<string | null>(null);
  const finish = async (text: string) => {
    setNotice(text);
    setPending(null);
    await Promise.all([res.reload(), credits.reload()]);
    onChanged();
  };

  return (
    <Drawer
      open
      onClose={onClose}
      title={d ? `@${d.account.username}` : t('admin.accounts.drawerTitle')}
    >
      <Async res={res} label={t('admin.accounts.loadingAccount')}>
        {(det) => {
          const a = det.account;
          return (
            <div className="adm-form">
              {notice && (
                <Notice variant="success" role="status">
                  {notice}
                </Notice>
              )}
              <dl className="adm-kv">
                <dt>{t('admin.accounts.dtState')}</dt>
                <dd>
                  <StateBadge state={a.state} />
                  {det.person_banned && ` ${t('admin.accounts.personBanned')}`}
                </dd>
                <dt>{t('admin.accounts.dtDisplayName')}</dt>
                <dd>{a.display_name || t('admin.accounts.none')}</dd>
                <dt>{t('admin.accounts.dtEmail')}</dt>
                <dd>
                  {a.email} {a.email_verified ? '' : t('admin.accounts.emailUnverified')}
                </dd>
                <dt>{t('admin.accounts.dtAccountId')}</dt>
                <dd className="adm-mono">{a.id}</dd>
                <dt>{t('admin.accounts.dtPersonId')}</dt>
                <dd className="adm-mono">{a.person_id}</dd>
                <dt>{t('admin.accounts.dtJoined')}</dt>
                <dd>{fmtMs(a.created_at_ms)}</dd>
                <dt>{t('admin.accounts.dtStateChanged')}</dt>
                <dd>{fmtMs(a.state_changed_at_ms)}</dd>
                <dt>{t('admin.accounts.dtTwoFactor')}</dt>
                <dd>{det.mfa_enabled ? t('admin.accounts.on') : t('admin.accounts.off')}</dd>
                <dt>{t('admin.accounts.dtActiveSessions')}</dt>
                <dd>{det.sessions_active}</dd>
                <dt>{t('admin.accounts.dtInvitedBy')}</dt>
                <dd>
                  {det.invited_by ? `@${det.invited_by.username}` : t('admin.accounts.rootAccount')}
                </dd>
                <dt>{t('admin.accounts.dtInvites')}</dt>
                <dd>
                  {t('admin.accounts.invitesSummary', {
                    issued: det.invites_issued,
                    redeemed: det.invites_redeemed,
                  })}
                </dd>
                <dt>{t('admin.accounts.dtSuspicion')}</dt>
                <dd>{det.suspicion_level}</dd>
                <dt>{t('admin.accounts.dtInviteCredits')}</dt>
                <dd>{credits.data ? credits.data.balance : det.credit_balance}</dd>
              </dl>

              {viewer.can('MANAGE_ACCOUNTS') && (
                <div
                  className="adm-actions"
                  role="group"
                  aria-label={t('admin.accounts.actionsGroup')}
                >
                  {a.state === 'active' && (
                    <Button variant="danger" onClick={() => setPending({ kind: 'suspend' })}>
                      {t('admin.accounts.suspend')}
                    </Button>
                  )}
                  {a.state === 'deactivated' && (
                    <Button onClick={() => setPending({ kind: 'unsuspend' })}>
                      {t('admin.accounts.reactivate')}
                    </Button>
                  )}
                  <Button variant="secondary" onClick={() => setPending({ kind: 'logout' })}>
                    {t('admin.accounts.revokeSessions')}
                  </Button>
                  <Button variant="secondary" onClick={() => setPending({ kind: 'reset2fa' })}>
                    {t('admin.accounts.resetTwoFactor')}
                  </Button>
                </div>
              )}
              {viewer.can('PLATFORM_BAN') && a.state !== 'platform_banned' && (
                <div className="adm-actions" role="group" aria-label={t('admin.accounts.banGroup')}>
                  <Button
                    variant="danger"
                    onClick={() => setPending({ kind: 'ban', category: 'standard' })}
                  >
                    {t('admin.accounts.banStandard')}
                  </Button>
                  <Button
                    variant="danger"
                    onClick={() => setPending({ kind: 'ban', category: 'serious' })}
                  >
                    {t('admin.accounts.banSerious')}
                  </Button>
                </div>
              )}

              {tree.data && (
                <section className="ss-card" aria-labelledby="adm-tree-h">
                  <h2 id="adm-tree-h">{t('admin.accounts.lineageTitle')}</h2>
                  <p className="ss-sub">
                    {tree.data.ancestors.length > 0
                      ? tree.data.ancestors.map((n) => `@${n.username}`).join(' → ') +
                        ` → @${tree.data.account.username}`
                      : t('admin.accounts.lineageRoot')}
                  </p>
                  <p className="ss-sub">
                    {t('admin.accounts.lineageStats', {
                      descendants: tree.data.stats.descendants,
                      banned: tree.data.stats.banned_descendants,
                      open: tree.data.stats.open,
                    })}
                  </p>
                </section>
              )}
              {credits.data && (
                <section className="ss-card" aria-labelledby="adm-cred-h">
                  <h2 id="adm-cred-h">{t('admin.accounts.creditsTitle')}</h2>
                  <div className="adm-actions">
                    <Button variant="secondary" onClick={() => setPending({ kind: 'credits' })}>
                      {t('admin.accounts.grantCredits')}
                    </Button>
                  </div>
                  {credits.data.entries.length === 0 ? (
                    <p className="ss-sub">{t('admin.accounts.noCreditHistory')}</p>
                  ) : (
                    <ul className="ss-list" aria-label={t('admin.accounts.creditLedger')}>
                      {credits.data.entries.slice(0, 10).map((e) => (
                        <li key={e.id} className="ss-row">
                          <div className="ss-grow">
                            <div className="ss-name">{e.delta > 0 ? `+${e.delta}` : e.delta}</div>
                            <div className="ss-meta">
                              {e.reason || t('admin.accounts.noReason')} · {fmtMs(e.created_at_ms)}
                            </div>
                          </div>
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              )}
              {det.recent_logins.length > 0 && (
                <section className="ss-card" aria-labelledby="adm-login-h">
                  <h2 id="adm-login-h">{t('admin.accounts.recentSignIns')}</h2>
                  <ul className="ss-list">
                    {det.recent_logins.map((l, i) => (
                      <li key={i} className="ss-row">
                        <div className="ss-grow">
                          <div className="ss-name">
                            {l.success
                              ? t('admin.accounts.signedIn')
                              : t('admin.accounts.failedAttempt')}
                          </div>
                          <div className="ss-meta">
                            {fmtMs(l.at_ms)} · {l.ip || t('admin.accounts.unknownAddress')}
                          </div>
                        </div>
                      </li>
                    ))}
                  </ul>
                </section>
              )}

              <ReasonDialog
                open={pending?.kind === 'suspend'}
                title={t('admin.accounts.suspendTitle', { username: a.username })}
                description={t('admin.accounts.suspendDescription')}
                confirmLabel={t('admin.accounts.suspendConfirm')}
                danger
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.suspendAccount(id, reason);
                  await finish(t('admin.accounts.suspended', { username: a.username }));
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'unsuspend'}
                title={t('admin.accounts.reactivateTitle', { username: a.username })}
                description={t('admin.accounts.reactivateDescription')}
                confirmLabel={t('admin.accounts.reactivateConfirm')}
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.unsuspendAccount(id, reason);
                  await finish(t('admin.accounts.reactivated', { username: a.username }));
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'logout'}
                title={t('admin.accounts.logoutTitle', { username: a.username })}
                description={t('admin.accounts.logoutDescription')}
                confirmLabel={t('admin.accounts.revokeSessions')}
                danger
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  const r = await api.forceLogout(id, reason);
                  await finish(t('admin.accounts.sessionsRevoked', { count: r.sessions_revoked }));
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'reset2fa'}
                title={t('admin.accounts.reset2faTitle', { username: a.username })}
                description={t('admin.accounts.reset2faDescription')}
                confirmLabel={t('admin.accounts.resetTwoFactor')}
                danger
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.reset2fa(id, reason);
                  await finish(t('admin.accounts.reset2faDone'));
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'ban'}
                title={t('admin.accounts.banTitle', { username: a.username })}
                description={
                  pending?.kind === 'ban' && pending.category === 'serious'
                    ? t('admin.accounts.banSeriousDescription')
                    : t('admin.accounts.banStandardDescription')
                }
                confirmLabel={t('admin.accounts.banConfirm')}
                danger
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  if (pending?.kind !== 'ban') return;
                  const r = await api.banAccount(id, pending.category, reason);
                  await finish(t('admin.accounts.banned', { count: r.accounts_banned }));
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'credits'}
                title={t('admin.accounts.creditsDialogTitle', { username: a.username })}
                description={t('admin.accounts.creditsDialogDescription')}
                confirmLabel={t('admin.accounts.apply')}
                valid={Number.isInteger(Number(delta)) && Number(delta) !== 0}
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  const r = await api.adjustCredits(id, Number(delta), reason);
                  await finish(t('admin.accounts.balanceNow', { balance: r.balance }));
                }}
              >
                <TextInput
                  label={t('admin.accounts.change')}
                  type="number"
                  value={delta}
                  onChange={(e) => setDelta(e.target.value)}
                />
              </ReasonDialog>
            </div>
          );
        }}
      </Async>
    </Drawer>
  );
}
