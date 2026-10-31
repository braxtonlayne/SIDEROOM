// OPS-INV-001: the invite controls at the top of Admin > Invites: Lock the Doors, the quotas, the last pass of
// the invites.prune job and the invite subtrees whose first member never activated.
import { Badge, Button, Notice } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { Async, ReasonDialog, fmtMs, useAdminViewer, useResource } from '../ui';

export function InviteControlsCard() {
  const t = useT();
  const viewer = useAdminViewer();
  const res = useResource('admin:invite-controls', api.getInviteControls);
  const [dialog, setDialog] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const canLock = viewer.can('MANAGE_FEATURES');

  return (
    <section className="ss-card" aria-labelledby="adm-doors-h">
      <h2 id="adm-doors-h">{t('admin.invites.doorsTitle')}</h2>
      <p className="ss-sub">{t('admin.invites.doorsIntro')}</p>
      {notice && (
        <Notice variant="success" role="status">
          {notice}
        </Notice>
      )}
      <Async res={res} label={t('admin.invites.doorsLoading')}>
        {(c) => (
          <>
            <div className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {t('admin.invites.doorsState')}{' '}
                  {c.locked ? (
                    <Badge variant="danger">{t('admin.invites.doorsClosed')}</Badge>
                  ) : (
                    <Badge variant="success">{t('admin.invites.doorsOpen')}</Badge>
                  )}
                </div>
                <div className="ss-meta">
                  {c.locked ? t('admin.invites.doorsClosedHelp') : t('admin.invites.doorsOpenHelp')}
                </div>
              </div>
              {canLock ? (
                <Button variant={c.locked ? 'secondary' : 'danger'} onClick={() => setDialog(true)}>
                  {c.locked ? t('admin.invites.doorsUnlock') : t('admin.invites.doorsLock')}
                </Button>
              ) : (
                <span className="ss-meta">{t('admin.invites.doorsNeedFeatures')}</span>
              )}
            </div>
            <dl className="ss-meta" aria-label={t('admin.invites.quotasLabel')}>
              <dt>{t('admin.invites.quotaCredits')}</dt>
              <dd>{c.default_credits}</dd>
              <dt>{t('admin.invites.quotaOutstanding')}</dt>
              <dd>{c.max_outstanding}</dd>
              <dt>{t('admin.invites.quotaDays')}</dt>
              <dd>{c.unused_code_days}</dd>
            </dl>
            <p className="ss-sub">{t('admin.invites.quotasEdit')}</p>
            <p className="ss-meta" role="status">
              {c.last_prune
                ? t('admin.invites.lastPrune', {
                    when: fmtMs(c.last_prune.ran_at_ms),
                    revoked: c.last_prune.codes_revoked,
                    marked: c.last_prune.roots_marked,
                    graph: c.last_prune.graph_rows,
                  })
                : t('admin.invites.neverPruned')}
            </p>
            <ReasonDialog
              open={dialog}
              danger={!c.locked}
              title={c.locked ? t('admin.invites.unlockTitle') : t('admin.invites.lockTitle')}
              description={
                c.locked ? t('admin.invites.unlockDescription') : t('admin.invites.lockDescription')
              }
              confirmLabel={
                c.locked ? t('admin.invites.doorsUnlock') : t('admin.invites.doorsLock')
              }
              onCancel={() => setDialog(false)}
              onConfirm={async (reason) => {
                const next = await api.setInvitesLocked(!c.locked, reason);
                res.set(next);
                setNotice(next.locked ? t('admin.invites.locked') : t('admin.invites.unlocked'));
                setDialog(false);
              }}
            />
          </>
        )}
      </Async>
      <InactiveSubtrees />
    </section>
  );
}

function InactiveSubtrees() {
  const t = useT();
  const res = useResource('admin:invite-inactive', () => api.listInactiveInviteRoots(50));
  return (
    <div>
      <h3 id="adm-inactive-h">{t('admin.invites.inactiveTitle')}</h3>
      <p className="ss-sub">{t('admin.invites.inactiveIntro')}</p>
      <Async res={res} label={t('admin.invites.inactiveLoading')}>
        {(d) =>
          d.items.length === 0 ? (
            <p className="ss-na">{t('admin.invites.inactiveEmpty')}</p>
          ) : (
            <>
              <ul className="ss-list" aria-labelledby="adm-inactive-h">
                {d.items.map((r) => (
                  <li key={r.account_id} className="ss-row">
                    <div className="ss-grow">
                      <div className="ss-name">@{r.username}</div>
                      <div className="ss-meta">
                        {t('admin.invites.inactiveRow', {
                          size: r.subtree_size,
                          when: fmtMs(r.marked_at_ms),
                        })}
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
              {d.total > d.items.length && (
                <p className="ss-meta">
                  {t('admin.invites.inactiveMore', { shown: d.items.length, total: d.total })}
                </p>
              )}
            </>
          )
        }
      </Async>
    </div>
  );
}
