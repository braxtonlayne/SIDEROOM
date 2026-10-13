import { Button, Drawer, EmptyState, Notice, Select, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import * as api from '../api';
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

const STATE_OPTIONS = [
  { value: '', label: 'Any state' },
  { value: 'active', label: 'Active' },
  { value: 'deactivated', label: 'Deactivated' },
  { value: 'platform_banned', label: 'Platform-banned' },
  { value: 'deleted', label: 'Deleted' },
];

export function AccountsSection() {
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
        title="Accounts"
        description="Search by username, display name, email fragment or exact account id. Opening an account is logged."
      />
      <form className="ss-toolbar" onSubmit={submit} role="search" aria-label="Account search">
        <div className="ss-search">
          <TextInput
            label="Search accounts"
            type="search"
            value={draft.q}
            placeholder="Username, email or id"
            onChange={(e) => setDraft({ ...draft, q: e.target.value })}
          />
        </div>
        <Select
          aria-label="Filter by state"
          options={STATE_OPTIONS}
          value={draft.state}
          onValueChange={(state) => {
            setDraft({ ...draft, state });
            setApplied({ q: draft.q.trim(), state });
          }}
        />
        <Button type="submit">Search</Button>
      </form>
      {paged.error && (
        <Notice variant="danger" className="ss-status" role="alert">
          {paged.error}
        </Notice>
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title="No accounts match"
          description="Try a different search or state filter."
        />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label="Accounts">
            <thead>
              <tr>
                <th scope="col">Account</th>
                <th scope="col">Email</th>
                <th scope="col">State</th>
                <th scope="col">Joined</th>
                <th scope="col">
                  <span className="adm-vh">Actions</span>
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
                    {!a.email_verified && <div className="ss-meta">Unverified</div>}
                  </td>
                  <td>
                    <StateBadge state={a.state} />
                  </td>
                  <td>{fmtMs(a.created_at_ms)}</td>
                  <td>
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={`View ${a.username}`}
                      onClick={() => setOpen(a.id)}
                    >
                      View
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
    <Drawer open onClose={onClose} title={d ? `@${d.account.username}` : 'Account'}>
      <Async res={res} label="Loading account">
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
                <dt>State</dt>
                <dd>
                  <StateBadge state={a.state} />
                  {det.person_banned && ' (Person banned)'}
                </dd>
                <dt>Display name</dt>
                <dd>{a.display_name || 'None'}</dd>
                <dt>Email</dt>
                <dd>
                  {a.email} {a.email_verified ? '' : '(unverified)'}
                </dd>
                <dt>Account id</dt>
                <dd className="adm-mono">{a.id}</dd>
                <dt>Person id</dt>
                <dd className="adm-mono">{a.person_id}</dd>
                <dt>Joined</dt>
                <dd>{fmtMs(a.created_at_ms)}</dd>
                <dt>State changed</dt>
                <dd>{fmtMs(a.state_changed_at_ms)}</dd>
                <dt>Two-factor</dt>
                <dd>{det.mfa_enabled ? 'On' : 'Off'}</dd>
                <dt>Active sessions</dt>
                <dd>{det.sessions_active}</dd>
                <dt>Invited by</dt>
                <dd>{det.invited_by ? `@${det.invited_by.username}` : 'Nobody (root account)'}</dd>
                <dt>Invites</dt>
                <dd>
                  {det.invites_issued} issued, {det.invites_redeemed} redeemed
                </dd>
                <dt>Suspicion</dt>
                <dd>{det.suspicion_level}</dd>
                <dt>Invite credits</dt>
                <dd>{credits.data ? credits.data.balance : det.credit_balance}</dd>
              </dl>

              {viewer.can('MANAGE_ACCOUNTS') && (
                <div className="adm-actions" role="group" aria-label="Account actions">
                  {a.state === 'active' && (
                    <Button variant="danger" onClick={() => setPending({ kind: 'suspend' })}>
                      Suspend
                    </Button>
                  )}
                  {a.state === 'deactivated' && (
                    <Button onClick={() => setPending({ kind: 'unsuspend' })}>Reactivate</Button>
                  )}
                  <Button variant="secondary" onClick={() => setPending({ kind: 'logout' })}>
                    Revoke sessions
                  </Button>
                  <Button variant="secondary" onClick={() => setPending({ kind: 'reset2fa' })}>
                    Reset two-factor
                  </Button>
                </div>
              )}
              {viewer.can('PLATFORM_BAN') && a.state !== 'platform_banned' && (
                <div className="adm-actions" role="group" aria-label="Platform ban">
                  <Button
                    variant="danger"
                    onClick={() => setPending({ kind: 'ban', category: 'standard' })}
                  >
                    Platform-ban (standard)
                  </Button>
                  <Button
                    variant="danger"
                    onClick={() => setPending({ kind: 'ban', category: 'serious' })}
                  >
                    Platform-ban (serious)
                  </Button>
                </div>
              )}

              {tree.data && (
                <section className="ss-card" aria-labelledby="adm-tree-h">
                  <h2 id="adm-tree-h">Invite lineage</h2>
                  <p className="ss-sub">
                    {tree.data.ancestors.length > 0
                      ? tree.data.ancestors.map((n) => `@${n.username}`).join(' → ') +
                        ` → @${tree.data.account.username}`
                      : 'Root of the invite tree.'}
                  </p>
                  <p className="ss-sub">
                    {tree.data.stats.descendants} invited downstream,{' '}
                    {tree.data.stats.banned_descendants} banned; {tree.data.stats.open} open codes.
                  </p>
                </section>
              )}
              {credits.data && (
                <section className="ss-card" aria-labelledby="adm-cred-h">
                  <h2 id="adm-cred-h">Invite credits</h2>
                  <div className="adm-actions">
                    <Button variant="secondary" onClick={() => setPending({ kind: 'credits' })}>
                      Grant or revoke credits
                    </Button>
                  </div>
                  {credits.data.entries.length === 0 ? (
                    <p className="ss-sub">No credit history.</p>
                  ) : (
                    <ul className="ss-list" aria-label="Credit ledger">
                      {credits.data.entries.slice(0, 10).map((e) => (
                        <li key={e.id} className="ss-row">
                          <div className="ss-grow">
                            <div className="ss-name">{e.delta > 0 ? `+${e.delta}` : e.delta}</div>
                            <div className="ss-meta">
                              {e.reason || 'No reason'} · {fmtMs(e.created_at_ms)}
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
                  <h2 id="adm-login-h">Recent sign-ins</h2>
                  <ul className="ss-list">
                    {det.recent_logins.map((l, i) => (
                      <li key={i} className="ss-row">
                        <div className="ss-grow">
                          <div className="ss-name">
                            {l.success ? 'Signed in' : 'Failed attempt'}
                          </div>
                          <div className="ss-meta">
                            {fmtMs(l.at_ms)} · {l.ip || 'unknown address'}
                          </div>
                        </div>
                      </li>
                    ))}
                  </ul>
                </section>
              )}

              <ReasonDialog
                open={pending?.kind === 'suspend'}
                title={`Suspend @${a.username}`}
                description="The account is deactivated and its sessions end. Staff accounts can only be suspended by the Owner."
                confirmLabel="Suspend account"
                danger
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.suspendAccount(id, reason);
                  await finish(`@${a.username} was suspended.`);
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'unsuspend'}
                title={`Reactivate @${a.username}`}
                description="Refused if the account cap has been reached."
                confirmLabel="Reactivate account"
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.unsuspendAccount(id, reason);
                  await finish(`@${a.username} was reactivated.`);
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'logout'}
                title={`Revoke every session of @${a.username}`}
                description="They are signed out everywhere and must log in again."
                confirmLabel="Revoke sessions"
                danger
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  const r = await api.forceLogout(id, reason);
                  await finish(`${r.sessions_revoked} session(s) revoked.`);
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'reset2fa'}
                title={`Reset two-factor for @${a.username}`}
                description="Removes the authenticator, recovery codes and passkeys, and ends all sessions. Use only after verifying the owner of the account."
                confirmLabel="Reset two-factor"
                danger
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.reset2fa(id, reason);
                  await finish('Two-factor was reset and sessions ended.');
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'ban'}
                title={`Platform-ban the Person behind @${a.username}`}
                description={
                  pending?.kind === 'ban' && pending.category === 'serious'
                    ? 'A serious ban raises suspicion on whoever invited this Person and may freeze their invites.'
                    : 'Every account of this Person is banned and signed out.'
                }
                confirmLabel="Ban"
                danger
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  if (pending?.kind !== 'ban') return;
                  const r = await api.banAccount(id, pending.category, reason);
                  await finish(`${r.accounts_banned} account(s) banned.`);
                }}
              />
              <ReasonDialog
                open={pending?.kind === 'credits'}
                title={`Invite credits for @${a.username}`}
                description="Use a positive number to grant and a negative number to revoke."
                confirmLabel="Apply"
                valid={Number.isInteger(Number(delta)) && Number(delta) !== 0}
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  const r = await api.adjustCredits(id, Number(delta), reason);
                  await finish(`Balance is now ${r.balance}.`);
                }}
              >
                <TextInput
                  label="Change"
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
