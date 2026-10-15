import { Button, Checkbox, Drawer, Notice, Select, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import * as api from '../api';
import * as donations from '../donations/api';
import { useT, type MessageKey } from '../../i18n';
import { DataTable, type Column } from '../DataTable';
import { PersonDrawer } from './PersonDrawer';
import {
  Async,
  ReasonDialog,
  SectionHeader,
  StateBadge,
  fmtMs,
  fmtUs,
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
  const columns: Column<api.AccountSummary>[] = [
    {
      id: 'account',
      header: t('admin.accounts.colAccount'),
      locked: true,
      csv: (a) => a.username,
      cell: (a) => (
        <>
          <strong>{a.display_name || a.username}</strong>
          <div className="ss-meta">@{a.username}</div>
        </>
      ),
    },
    {
      id: 'email',
      header: t('admin.accounts.colEmail'),
      csv: (a) => a.email,
      cell: (a) => (
        <>
          {a.email}
          {!a.email_verified && <div className="ss-meta">{t('admin.accounts.unverified')}</div>}
        </>
      ),
    },
    {
      id: 'state',
      header: t('admin.accounts.colState'),
      csv: (a) => a.state,
      cell: (a) => <StateBadge state={a.state} />,
    },
    {
      id: 'joined',
      header: t('admin.accounts.colJoined'),
      csv: (a) => new Date(a.created_at_ms).toISOString(),
      cell: (a) => fmtMs(a.created_at_ms),
    },
    {
      id: 'id',
      header: t('admin.accounts.colId'),
      defaultHidden: true,
      csv: (a) => a.id,
      cell: (a) => <span className="adm-mono">{a.id}</span>,
    },
    {
      id: 'person',
      header: t('admin.accounts.colPerson'),
      defaultHidden: true,
      csv: (a) => a.person_id,
      cell: (a) => <span className="adm-mono">{a.person_id}</span>,
    },
  ];
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
      <DataTable
        tableId="accounts"
        label={t('admin.accounts.tableLabel')}
        columns={columns}
        paged={paged}
        rowKey={(a) => a.id}
        emptyTitle={t('admin.accounts.emptyTitle')}
        emptyDescription={t('admin.accounts.emptyDescription')}
        actions={(a) => (
          <Button
            size="sm"
            variant="secondary"
            aria-label={t('admin.accounts.viewAria', { username: a.username })}
            onClick={() => setOpen(a.id)}
          >
            {t('admin.accounts.view')}
          </Button>
        )}
        filters={
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
        }
      />
      {open && (
        <AccountDrawer id={open} onClose={() => setOpen(null)} onChanged={() => paged.reload()} />
      )}
    </>
  );
}

const BADGE_KEY: Record<string, MessageKey> = {
  early_member: 'admin.accounts.badgeEarlyMember',
  contributor: 'admin.accounts.badgeContributor',
  bug_hunter: 'admin.accounts.badgeBugHunter',
  honorary: 'admin.accounts.badgeHonorary',
};
/** Staff-grantable badges set in `accounts.flags` (bits 8 to 11). */
const heldBadges = (flags: number) =>
  api.STAFF_BADGES.filter((b) => Math.floor(flags / 2 ** b.bit) % 2 === 1).map(
    (b) => b.name as string,
  );

type Pending =
  | { kind: 'suspend' }
  | { kind: 'unsuspend' }
  | { kind: 'logout' }
  | { kind: 'reset2fa' }
  | { kind: 'rename' }
  | { kind: 'email' }
  | { kind: 'badges' }
  | { kind: 'forcePassword' }
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
  const entitlements = useResource(`ent:${id}:${viewer.can('MANAGE_DONATIONS')}`, async () =>
    viewer.can('MANAGE_DONATIONS') ? donations.listGrants({ user_id: id }) : null,
  );
  const audit = useResource(`audit:${id}:${viewer.can('VIEW_ACCESS_LOG')}`, async () =>
    viewer.can('VIEW_ACCESS_LOG') ? api.listAccessLog({ target_id: id, limit: 10 }) : null,
  );
  const [personOpen, setPersonOpen] = useState(false);
  const [pending, setPending] = useState<Pending>(null);
  const [delta, setDelta] = useState('1');
  const [username, setUsername] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [email, setEmail] = useState('');
  const [badgeSet, setBadgeSet] = useState<string[]>([]);
  // The ban dialog: the reason category presets "serious" (D-211); staff can change it.
  const [banReason, setBanReason] = useState<api.BanReason | ''>('');
  const [banSerious, setBanSerious] = useState(false);
  const openBan = (serious: boolean) => {
    setBanReason('');
    setBanSerious(serious);
    setPending({ kind: 'ban', category: serious ? 'serious' : 'standard' });
  };
  const banReasonOptions = [
    { value: '', label: t('admin.accounts.banReasonNone') },
    { value: 'illegal_content', label: t('admin.accounts.banReason.illegalContent') },
    { value: 'child_safety', label: t('admin.accounts.banReason.childSafety') },
    { value: 'fraud', label: t('admin.accounts.banReason.fraud') },
    { value: 'platform_abuse', label: t('admin.accounts.banReason.platformAbuse') },
    { value: 'harassment', label: t('admin.accounts.banReason.harassment') },
    { value: 'ban_evasion', label: t('admin.accounts.banReason.banEvasion') },
  ];
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
                <dt>{t('admin.accounts.dtFlags')}</dt>
                <dd>
                  {a.flags}
                  {heldBadges(a.flags).length > 0 &&
                    ` (${heldBadges(a.flags)
                      .map((b) => t(BADGE_KEY[b]!))
                      .join(', ')})`}
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
                  <Button
                    variant="secondary"
                    onClick={() => {
                      setUsername(a.username);
                      setDisplayName(a.display_name ?? '');
                      setPending({ kind: 'rename' });
                    }}
                  >
                    {t('admin.accounts.rename')}
                  </Button>
                  <Button
                    variant="secondary"
                    onClick={() => {
                      setEmail(a.email);
                      setPending({ kind: 'email' });
                    }}
                  >
                    {t('admin.accounts.changeEmail')}
                  </Button>
                  <Button
                    variant="secondary"
                    onClick={() => {
                      setBadgeSet(heldBadges(a.flags));
                      setPending({ kind: 'badges' });
                    }}
                  >
                    {t('admin.accounts.badges')}
                  </Button>
                  <Button variant="danger" onClick={() => setPending({ kind: 'forcePassword' })}>
                    {t('admin.accounts.forcePasswordReset')}
                  </Button>
                </div>
              )}
              {viewer.can('PLATFORM_BAN') && a.state !== 'platform_banned' && (
                <div className="adm-actions" role="group" aria-label={t('admin.accounts.banGroup')}>
                  <Button variant="danger" onClick={() => openBan(false)}>
                    {t('admin.accounts.banStandard')}
                  </Button>
                  <Button variant="danger" onClick={() => openBan(true)}>
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
              <div className="adm-actions">
                <Button variant="secondary" onClick={() => setPersonOpen(true)}>
                  {t('admin.accounts.openPerson')}
                </Button>
              </div>
              {entitlements.data && (
                <section className="ss-card" aria-labelledby="adm-ent-h">
                  <h2 id="adm-ent-h">{t('admin.accounts.entitlementsTitle')}</h2>
                  {entitlements.data.items.length === 0 ? (
                    <p className="ss-sub">{t('admin.accounts.noEntitlements')}</p>
                  ) : (
                    <ul className="ss-list" aria-label={t('admin.accounts.entitlementsTitle')}>
                      {entitlements.data.items.map((g) => (
                        <li key={g.id} className="ss-row">
                          <div className="ss-grow">
                            <div className="ss-name">
                              {g.kind} <StateBadge state={g.revoked_at_ms ? 'revoked' : 'active'} />
                            </div>
                            <div className="ss-meta">
                              {g.permanent
                                ? t('admin.accounts.entitlementPermanent')
                                : g.ends_at_ms
                                  ? t('admin.accounts.entitlementUntil', {
                                      when: fmtMs(g.ends_at_ms),
                                    })
                                  : ''}
                            </div>
                          </div>
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              )}
              {audit.data && (
                <section className="ss-card" aria-labelledby="adm-audit-h">
                  <h2 id="adm-audit-h">{t('admin.accounts.auditTitle')}</h2>
                  {audit.data.items.length === 0 ? (
                    <p className="ss-sub">{t('admin.accounts.noAudit')}</p>
                  ) : (
                    <ul className="ss-list" aria-label={t('admin.accounts.auditTitle')}>
                      {audit.data.items.map((e) => (
                        <li key={e.seq} className="ss-row">
                          <div className="ss-grow">
                            <div className="ss-name">{e.action}</div>
                            <div className="ss-meta">
                              {fmtUs(e.at_us)}
                              {e.reason ? ` · ${e.reason}` : ''}
                            </div>
                          </div>
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              )}
              {personOpen && (
                <PersonDrawer personId={a.person_id} onClose={() => setPersonOpen(false)} />
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
                open={pending?.kind === 'rename'}
                title={t('admin.accounts.renameTitle', { username: a.username })}
                description={t('admin.accounts.renameDescription')}
                confirmLabel={t('admin.accounts.rename')}
                valid={
                  username.trim().length > 0 &&
                  (username.trim() !== a.username || displayName.trim() !== (a.display_name ?? ''))
                }
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.renameAccount(
                    id,
                    {
                      ...(username.trim() !== a.username ? { username: username.trim() } : {}),
                      ...(displayName.trim() !== (a.display_name ?? '')
                        ? { display_name: displayName.trim() }
                        : {}),
                    },
                    reason,
                  );
                  await finish(t('admin.accounts.renamed'));
                }}
              >
                <TextInput
                  label={t('admin.accounts.newUsername')}
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                />
                <TextInput
                  label={t('admin.accounts.newDisplayName')}
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                />
              </ReasonDialog>
              <ReasonDialog
                open={pending?.kind === 'email'}
                title={t('admin.accounts.emailTitle', { username: a.username })}
                description={t('admin.accounts.emailDescription')}
                confirmLabel={t('admin.accounts.changeEmail')}
                danger
                valid={/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()) && email.trim() !== a.email}
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.changeAccountEmail(id, email.trim(), reason);
                  await finish(t('admin.accounts.emailChanged'));
                }}
              >
                <TextInput
                  label={t('admin.accounts.newEmail')}
                  type="email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                />
              </ReasonDialog>
              <ReasonDialog
                open={pending?.kind === 'badges'}
                title={t('admin.accounts.badgesTitle', { username: a.username })}
                description={t('admin.accounts.badgesDescription')}
                confirmLabel={t('admin.accounts.saveBadges')}
                valid={api.STAFF_BADGES.some(
                  (b) => badgeSet.includes(b.name) !== heldBadges(a.flags).includes(b.name),
                )}
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  const held = heldBadges(a.flags);
                  await api.setAccountBadges(
                    id,
                    badgeSet.filter((b) => !held.includes(b)),
                    held.filter((b) => !badgeSet.includes(b)),
                    reason,
                  );
                  await finish(t('admin.accounts.badgesSaved'));
                }}
              >
                <fieldset className="adm-check-row" style={{ border: 0, padding: 0, margin: 0 }}>
                  <legend className="ss-sub">{t('admin.accounts.badgesLegend')}</legend>
                  {api.STAFF_BADGES.map((b) => (
                    <Checkbox
                      key={b.name}
                      label={t(BADGE_KEY[b.name]!)}
                      checked={badgeSet.includes(b.name)}
                      onChange={(e) =>
                        setBadgeSet(
                          e.target.checked
                            ? [...badgeSet, b.name]
                            : badgeSet.filter((x) => x !== b.name),
                        )
                      }
                    />
                  ))}
                </fieldset>
              </ReasonDialog>
              <ReasonDialog
                open={pending?.kind === 'forcePassword'}
                title={t('admin.accounts.forcePasswordTitle', { username: a.username })}
                description={t('admin.accounts.forcePasswordDescription')}
                confirmLabel={t('admin.accounts.forcePasswordReset')}
                danger
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  const r = await api.forcePasswordReset(id, reason);
                  await finish(
                    t('admin.accounts.forcePasswordDone', { count: r.sessions_revoked }),
                  );
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'ban'}
                title={t('admin.accounts.banTitle', { username: a.username })}
                description={
                  banSerious
                    ? t('admin.accounts.banSeriousDescription')
                    : t('admin.accounts.banStandardDescription')
                }
                confirmLabel={t('admin.accounts.banConfirm')}
                danger
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  if (pending?.kind !== 'ban') return;
                  const r = await api.banAccount(
                    id,
                    banSerious ? 'serious' : 'standard',
                    reason,
                    banReason || undefined,
                  );
                  await finish(t('admin.accounts.banned', { count: r.accounts_banned }));
                }}
              >
                <Select
                  aria-label={t('admin.accounts.banReasonLabel')}
                  options={banReasonOptions}
                  value={banReason}
                  onValueChange={(v) => {
                    setBanReason(v as api.BanReason | '');
                    const preset = api.BAN_REASONS.find((r) => r.value === v);
                    if (preset) setBanSerious(preset.serious);
                  }}
                />
                <Checkbox
                  label={t('admin.accounts.banSeriousToggle')}
                  description={t('admin.accounts.banSeriousToggleHint')}
                  checked={banSerious}
                  onChange={(e) => setBanSerious(e.target.checked)}
                />
              </ReasonDialog>
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
