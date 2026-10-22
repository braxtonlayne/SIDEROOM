// Account drawer additions (ADM-PPL-002, ADM-GRAPH-004): sessions and servers lists (read on request, because each
// read is written to the access log), freeze or unfreeze invites, and moving an account out of its Person.
import { Badge, Button } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { Async, ReasonDialog, fmtMs, useAdminViewer, useResource } from '../ui';

export function SessionsCard({ accountId }: { accountId: string }) {
  const t = useT();
  const res = useResource(`acct-sessions:${accountId}`, () => api.listAccountSessions(accountId));
  return (
    <Async res={res} label={t('admin.accountExtras.loadingSessions')}>
      {(list) =>
        list.length === 0 ? (
          <p className="ss-sub">{t('admin.accountExtras.noSessions')}</p>
        ) : (
          <ul className="ss-list" aria-label={t('admin.accountExtras.sessionsTitle')}>
            {list.map((s) => (
              <li key={s.id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">
                    {s.device_label || s.user_agent || t('admin.accountExtras.unknownDevice')}
                  </div>
                  <div className="ss-meta">
                    {s.ip || t('admin.accounts.unknownAddress')} ·{' '}
                    {t('admin.accountExtras.lastSeen', { when: fmtMs(s.last_seen_at_ms) })} ·{' '}
                    {t('admin.accountExtras.expires', { when: fmtMs(s.expires_at_ms) })}
                  </div>
                </div>
              </li>
            ))}
          </ul>
        )
      }
    </Async>
  );
}

/** The staff-only archive of a deleted account's original identifiers (D-326), read on request. */
export function DeletedArchiveCard({ accountId }: { accountId: string }) {
  const t = useT();
  const [show, setShow] = useState(false);
  return (
    <section className="ss-card" aria-labelledby="adm-archive-h">
      <h2 id="adm-archive-h">{t('admin.accountExtras.archiveTitle')}</h2>
      <p className="ss-sub">{t('admin.accountExtras.archiveHint')}</p>
      {show ? (
        <ArchiveBody accountId={accountId} />
      ) : (
        <Button variant="secondary" onClick={() => setShow(true)}>
          {t('admin.accountExtras.archiveShow')}
        </Button>
      )}
    </section>
  );
}

function ArchiveBody({ accountId }: { accountId: string }) {
  const t = useT();
  const res = useResource(`acct-archive:${accountId}`, () => api.getDeletedArchive(accountId));
  return (
    <Async res={res} label={t('admin.accountExtras.archiveLoading')}>
      {(a) => (
        <dl className="ss-meta">
          <dt>{t('admin.accountExtras.archiveUsername')}</dt>
          <dd>{a.username}</dd>
          <dt>{t('admin.accountExtras.archiveEmail')}</dt>
          <dd>{a.email}</dd>
          <dt>{t('admin.accountExtras.archiveDisplayName')}</dt>
          <dd>{a.display_name ?? ''}</dd>
          <dd>
            {t('admin.accountExtras.archiveDeleted', { when: fmtMs(a.deleted_at_ms) })} ·{' '}
            {t('admin.accountExtras.archiveRelease', { when: fmtMs(a.username_release_at_ms) })}
          </dd>
        </dl>
      )}
    </Async>
  );
}

export function ServersCard({ accountId }: { accountId: string }) {
  const t = useT();
  const res = useResource(`acct-guilds:${accountId}`, () => api.listAccountGuilds(accountId));
  return (
    <Async res={res} label={t('admin.accountExtras.loadingServers')}>
      {(list) =>
        list.length === 0 ? (
          <p className="ss-sub">{t('admin.accountExtras.noServers')}</p>
        ) : (
          <ul className="ss-list" aria-label={t('admin.accountExtras.serversTitle')}>
            {list.map((g) => (
              <li key={g.id} className="ss-row">
                <div className="ss-grow">
                  <div className="ss-name">
                    {g.name} {g.owner && <Badge>{t('admin.accountExtras.owner')}</Badge>}
                  </div>
                  <div className="ss-meta">
                    {t('admin.accountExtras.serverMeta', {
                      members: g.member_count,
                      when: fmtMs(g.joined_at_ms),
                    })}
                  </div>
                </div>
              </li>
            ))}
          </ul>
        )
      }
    </Async>
  );
}

type Pending = 'freeze' | 'unfreeze' | 'unlink' | null;

export function AccountExtras({
  account,
  det,
  onChanged,
}: {
  account: api.AccountSummary;
  det: api.AccountDetail;
  onChanged: (notice: string) => void;
}) {
  const t = useT();
  const viewer = useAdminViewer();
  const [showSessions, setShowSessions] = useState(false);
  const [showServers, setShowServers] = useState(false);
  const [pending, setPending] = useState<Pending>(null);
  const canAccounts = viewer.can('MANAGE_ACCOUNTS');
  const canInvites = viewer.can('MANAGE_INVITES');
  const frozen = det.invites_frozen === true;
  return (
    <>
      {canAccounts && account.state === 'deleted' && <DeletedArchiveCard accountId={account.id} />}
      {canInvites && (
        <section className="ss-card" aria-labelledby="adm-freeze-h">
          <h2 id="adm-freeze-h">{t('admin.accountExtras.freezeTitle')}</h2>
          <p className="ss-sub">
            {frozen ? (
              <Badge variant="warning">{t('admin.accountExtras.frozen')}</Badge>
            ) : (
              t('admin.accountExtras.notFrozen')
            )}
          </p>
          <div className="adm-actions">
            <Button
              variant={frozen ? 'secondary' : 'danger'}
              onClick={() => setPending(frozen ? 'unfreeze' : 'freeze')}
            >
              {frozen ? t('admin.accountExtras.unfreeze') : t('admin.accountExtras.freeze')}
            </Button>
          </div>
        </section>
      )}
      {canAccounts && (
        <>
          <section className="ss-card" aria-labelledby="adm-sess-h">
            <h2 id="adm-sess-h">{t('admin.accountExtras.sessionsTitle')}</h2>
            {showSessions ? (
              <SessionsCard accountId={account.id} />
            ) : (
              <>
                <p className="ss-sub">{t('admin.accountExtras.sessionsLogged')}</p>
                <Button variant="secondary" onClick={() => setShowSessions(true)}>
                  {t('admin.accountExtras.showSessions')}
                </Button>
              </>
            )}
          </section>
          <section className="ss-card" aria-labelledby="adm-srv-h">
            <h2 id="adm-srv-h">{t('admin.accountExtras.serversTitle')}</h2>
            {showServers ? (
              <ServersCard accountId={account.id} />
            ) : (
              <Button variant="secondary" onClick={() => setShowServers(true)}>
                {t('admin.accountExtras.showServers')}
              </Button>
            )}
          </section>
          <div className="adm-actions">
            <Button variant="secondary" onClick={() => setPending('unlink')}>
              {t('admin.accountExtras.unlink')}
            </Button>
          </div>
        </>
      )}
      <ReasonDialog
        open={pending === 'freeze'}
        danger
        title={t('admin.accountExtras.freezeDialog', { username: account.username })}
        description={t('admin.accountExtras.freezeDescription')}
        confirmLabel={t('admin.accountExtras.freeze')}
        onCancel={() => setPending(null)}
        onConfirm={async (reason) => {
          await api.freezeInvites(account.id, reason);
          setPending(null);
          onChanged(t('admin.accountExtras.froze', { username: account.username }));
        }}
      />
      <ReasonDialog
        open={pending === 'unfreeze'}
        title={t('admin.accountExtras.unfreezeDialog', { username: account.username })}
        description={t('admin.accountExtras.unfreezeDescription')}
        confirmLabel={t('admin.accountExtras.unfreeze')}
        onCancel={() => setPending(null)}
        onConfirm={async (reason) => {
          await api.unfreezeInvites(account.id, reason);
          setPending(null);
          onChanged(t('admin.accountExtras.unfroze', { username: account.username }));
        }}
      />
      <ReasonDialog
        open={pending === 'unlink'}
        danger
        title={t('admin.accountExtras.unlinkDialog', { username: account.username })}
        description={t('admin.accountExtras.unlinkDescription')}
        confirmLabel={t('admin.accountExtras.unlink')}
        onCancel={() => setPending(null)}
        onConfirm={async (reason) => {
          const r = await api.unlinkAccount(account.id, reason);
          setPending(null);
          onChanged(t('admin.accountExtras.unlinked', { person: r.person_id }));
        }}
      />
    </>
  );
}
