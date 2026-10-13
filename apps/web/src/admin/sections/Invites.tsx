import { Badge, Button, ConfirmDialog, EmptyState, Notice, Select, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
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

const INVITE_STATES = [
  { value: '', label: 'Any state' },
  { value: 'open', label: 'Open' },
  { value: 'redeemed', label: 'Redeemed' },
  { value: 'revoked', label: 'Revoked' },
];

export function InvitesSection() {
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
        title="Invites"
        description="Every platform invite. Invite codes are secrets of their issuers; they are shown here only to staff with Manage Invites."
      />
      <Notice variant="info" className="ss-status" title="Creating invites">
        Invites are issued from the command line, which bypasses credit rules:{' '}
        <code>sr-cli invite create --issuer &lt;username&gt; --note &quot;why&quot;</code>.
        Emergency invites show an <em>emergency</em> tag below. Credits are granted per account from
        Accounts.
      </Notice>
      <form
        className="ss-toolbar"
        role="search"
        aria-label="Filter invites"
        onSubmit={(e) => {
          e.preventDefault();
          setIssuerApplied(issuer.trim());
        }}
      >
        <TextInput
          label="Issuer account id"
          value={issuer}
          inputMode="numeric"
          onChange={(e) => setIssuer(e.target.value)}
        />
        <Select
          aria-label="Filter by invite state"
          options={INVITE_STATES}
          value={state}
          onValueChange={setState}
        />
        <Button type="submit">Filter</Button>
      </form>
      <ActionStatus error={action.error ?? paged.error} done={action.done} />
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState title="No invites" description="Nothing matches this filter." />
      )}
      {paged.items.length > 0 && (
        <div className="adm-table-wrap">
          <table className="adm-table" aria-label="Invites">
            <thead>
              <tr>
                <th scope="col">Code</th>
                <th scope="col">Issuer</th>
                <th scope="col">State</th>
                <th scope="col">Note</th>
                <th scope="col">Created</th>
                <th scope="col">
                  <span className="adm-vh">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {paged.items.map((i) => (
                <tr key={i.id}>
                  <td className="adm-mono">
                    {i.code} {i.emergency && <Badge variant="warning">emergency</Badge>}
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
                        aria-label={`Revoke invite ${i.code}`}
                        onClick={() => setRevoke(i)}
                      >
                        Revoke
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
        title="Revoke this invite"
        description={`The code ${revoke?.code ?? ''} stops working immediately. This cannot be undone.`}
        confirmLabel="Revoke invite"
        loading={action.busy}
        onCancel={() => setRevoke(null)}
        onConfirm={() => {
          if (!revoke) return;
          void action
            .run(async () => {
              await api.revokeInvite(revoke.id);
              return true;
            }, `Invite ${revoke.code} was revoked.`)
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
  const res = useResource('admin:suspicion', api.listSuspects);
  const [target, setTarget] = useState<api.SuspectPerson | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  return (
    <section className="ss-card" aria-labelledby="adm-susp-h" style={{ marginTop: 24 }}>
      <h2 id="adm-susp-h">Suspicion</h2>
      <p className="ss-sub">
        People who invited banned accounts. At the freeze threshold their invite creation is frozen.
      </p>
      {notice && (
        <Notice variant="success" role="status">
          {notice}
        </Notice>
      )}
      <Async res={res} label="Loading suspicion">
        {(list) =>
          list.length === 0 ? (
            <p className="ss-na">Nobody has raised suspicion.</p>
          ) : (
            <ul className="ss-list" aria-label="Suspicious people">
              {list.map((p) => (
                <li key={p.person_id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">
                      {p.accounts.map((a) => `@${a.username}`).join(', ') ||
                        `Person ${p.person_id}`}{' '}
                      {p.frozen && <Badge variant="danger">invites frozen</Badge>}
                    </div>
                    <div className="ss-meta">Suspicion level {p.suspicion_level}</div>
                  </div>
                  <Button size="sm" variant="secondary" onClick={() => setTarget(p)}>
                    Clear suspicion
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <ReasonDialog
        open={!!target}
        title="Clear suspicion"
        description="Resets the level to zero."
        confirmLabel="Clear suspicion"
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          await api.clearSuspicion(target.person_id, reason);
          setNotice('Suspicion was cleared.');
          setTarget(null);
          await res.reload();
        }}
      />
    </section>
  );
}
