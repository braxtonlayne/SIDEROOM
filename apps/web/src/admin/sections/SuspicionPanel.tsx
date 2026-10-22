// Suspicion panel of the Person page (MOD-SUS-001, -003, -004; docs/specs/moderation-compliance.md section 10):
// the history, the banned accounts below the Person with their ban reasons, the invite path to the root, the
// automatic cases and (for staff who may see sign-in data) the evasion signals. Clearing stays the Person page's
// own action; this panel shows that it happened and why. Nothing here bans, freezes or hides anyone.
import { Badge, Button, Notice } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import * as api from '../../moderation/api';
import { SignalRow } from '../moderation/SignalRow';
import { STATE_KEYS } from '../moderation/labels';
import { Async, ApiProblem, errorText, fmtMs, useAdminViewer, useResource } from '../ui';

export function SuspicionPanel({ personId, version }: { personId: string; version: number }) {
  const t = useT();
  const viewer = useAdminViewer();
  const res = useResource(`susp-panel:${personId}:${version}`, () =>
    api.getSuspicionPanel(personId),
  );
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const canScan = viewer.can('MANAGE_ACCOUNTS');

  const scan = async () => {
    setError(null);
    setNotice(null);
    try {
      const out = await api.scanEvasion(personId);
      setNotice(t('admin.suspicion.scanned', { n: out.raised }));
      await res.reload();
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <Async res={res}>
      {(p) => (
        <section className="ss-card" aria-labelledby="adm-susp-h">
          <h2 id="adm-susp-h">{t('admin.suspicion.title')}</h2>
          <p className="ss-sub">
            {t('admin.suspicion.summary', {
              level: p.level,
              freeze: p.freeze_threshold,
              escalate: p.escalate_threshold,
            })}{' '}
            {p.invites_paused && <Badge variant="warning">{t('admin.suspicion.paused')}</Badge>}
          </p>
          <p className="ss-sub">{t('admin.suspicion.neverAuto')}</p>

          {p.cases.length > 0 && (
            <>
              <h3>{t('admin.suspicion.casesTitle')}</h3>
              <ul className="ss-list" aria-label={t('admin.suspicion.casesTitle')}>
                {p.cases.map((c) => (
                  <li key={c.id} className="ss-row">
                    <div className="ss-grow">
                      <div className="ss-name">
                        {t(
                          c.auto_kind === 'suspicion_escalation'
                            ? 'admin.suspicion.caseEscalation'
                            : 'admin.suspicion.caseFreeze',
                          { id: c.id },
                        )}{' '}
                        <Badge>{t(STATE_KEYS[c.state])}</Badge>
                      </div>
                      <div className="ss-meta">
                        {fmtMs(c.opened_at_ms)}
                        {c.assignee_id
                          ? ` · ${t('admin.moderation.queue.assignedTo', { id: c.assignee_id })}`
                          : ` · ${t('admin.moderation.queue.unassigned')}`}
                        {c.linked_case_id
                          ? ` · ${t('admin.suspicion.linkedCase', { id: c.linked_case_id })}`
                          : ''}
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
            </>
          )}

          <h3>{t('admin.suspicion.historyTitle')}</h3>
          {p.events.length === 0 ? (
            <p className="ss-sub">{t('admin.suspicion.noHistory')}</p>
          ) : (
            <ul className="ss-list" aria-label={t('admin.suspicion.historyTitle')}>
              {p.events.map((e) => (
                <li key={e.id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">
                      {e.kind === 'cleared'
                        ? t('admin.suspicion.cleared', { level: e.level_after })
                        : e.kind === 'reregistration_refused'
                          ? t('admin.suspicion.refused')
                          : t('admin.suspicion.raised', {
                              delta: `+${e.delta}`,
                              level: e.level_after,
                            })}
                      {e.source_account ? ` · @${e.source_account.username}` : ''}
                    </div>
                    <div className="ss-meta">
                      {fmtMs(e.created_at_ms)} · {e.reason}
                      {e.case_id ? ` · ${t('admin.suspicion.linkedCase', { id: e.case_id })}` : ''}
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          )}

          <h3>{t('admin.suspicion.descendantsTitle')}</h3>
          {p.banned_descendants.length === 0 ? (
            <p className="ss-sub">{t('admin.suspicion.noDescendants')}</p>
          ) : (
            <ul className="ss-list" aria-label={t('admin.suspicion.descendantsTitle')}>
              {p.banned_descendants.map((d) => (
                <li key={d.account.id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">
                      @{d.account.username}{' '}
                      <Badge variant={d.ban_category === 'serious' ? 'danger' : undefined}>
                        {d.ban_category === 'serious'
                          ? t('admin.suspicion.banSerious')
                          : t('admin.suspicion.banStandard')}
                      </Badge>
                      {d.lifted && (
                        <>
                          {' '}
                          <Badge>{t('admin.suspicion.banLifted')}</Badge>
                        </>
                      )}
                    </div>
                    <div className="ss-meta">
                      {t('admin.suspicion.depth', { n: d.depth })}
                      {d.reason_category
                        ? ` · ${t('admin.suspicion.reason', { reason: d.reason_category })}`
                        : ''}
                      {` · ${fmtMs(d.banned_at_ms)}`}
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          )}

          <h3>{t('admin.suspicion.pathTitle')}</h3>
          <ul className="ss-list" aria-label={t('admin.suspicion.pathTitle')}>
            {p.invite_paths.map((path) => (
              <li key={path.account_id} className="ss-row">
                <div className="ss-grow">
                  {path.nodes.length <= 1
                    ? t('admin.suspicion.pathRoot', {
                        name: path.nodes[0]?.username ?? path.account_id,
                      })
                    : path.nodes.map((n) => `@${n.username}`).join(' → ')}
                </div>
              </li>
            ))}
          </ul>

          {p.signals && (
            <>
              <h3>{t('admin.suspicion.signalsTitle')}</h3>
              <p className="ss-sub">{t('admin.suspicion.signalsIntro')}</p>
              <ApiProblem error={error} />
              {notice && (
                <Notice variant="success" role="status" className="ss-status">
                  {notice}
                </Notice>
              )}
              {p.signals.length === 0 ? (
                <p className="ss-sub">{t('admin.suspicion.noSignals')}</p>
              ) : (
                <ul className="ss-list" aria-label={t('admin.suspicion.signalsTitle')}>
                  {p.signals.map((s) => (
                    <SignalRow
                      key={s.id}
                      signal={s}
                      canReview={canScan}
                      onChanged={() => void res.reload()}
                    />
                  ))}
                </ul>
              )}
              {canScan && (
                <div className="adm-actions">
                  <Button variant="secondary" onClick={() => void scan()}>
                    {t('admin.suspicion.scan')}
                  </Button>
                </div>
              )}
            </>
          )}
        </section>
      )}
    </Async>
  );
}
