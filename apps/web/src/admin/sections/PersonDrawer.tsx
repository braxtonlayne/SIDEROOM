// Person record (ADM-PPL-004): the chain of accounts one human holds, ban state, notes (editable), linking an
// account, suspicion with its log, and the access-log history for the person.
import { Badge, Button, Drawer, Notice, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { Async, ReasonDialog, StateBadge, fmtMs, fmtUs, useAdminViewer, useResource } from '../ui';

export function PersonDrawer({ personId, onClose }: { personId: string; onClose: () => void }) {
  const t = useT();
  const viewer = useAdminViewer();
  const res = useResource(`person:${personId}`, () => api.getPerson(personId));
  const history = useResource(
    `person-log:${personId}:${viewer.can('VIEW_ACCESS_LOG')}`,
    async () =>
      viewer.can('VIEW_ACCESS_LOG') ? api.listAccessLog({ target_id: personId, limit: 20 }) : null,
  );
  const suspicion = useResource(
    `person-susp:${personId}:${viewer.can('MANAGE_INVITES')}`,
    async () => (viewer.can('MANAGE_INVITES') ? api.listSuspicionLog(personId) : null),
  );
  const [pending, setPending] = useState<'clear' | 'unban' | 'notes' | 'link' | null>(null);
  const [notes, setNotes] = useState('');
  const [linkId, setLinkId] = useState('');
  const [notice, setNotice] = useState<string | null>(null);

  return (
    <Drawer open onClose={onClose} title={t('admin.person.title', { id: personId })}>
      <Async res={res} label={t('admin.person.loading')}>
        {(p) => {
          const chain = [...p.accounts].sort((a, b) => a.created_at_ms - b.created_at_ms);
          return (
            <div className="adm-form">
              {notice && (
                <Notice variant="success" role="status">
                  {notice}
                </Notice>
              )}
              <dl className="adm-kv">
                <dt>{t('admin.person.dtState')}</dt>
                <dd>
                  {p.ban && !p.ban.lifted_at_ms ? (
                    <Badge variant="danger">{t('admin.person.banned')}</Badge>
                  ) : (
                    t('admin.person.notBanned')
                  )}
                </dd>
                {p.ban && (
                  <>
                    <dt>{t('admin.person.banDetailLabel')}</dt>
                    <dd>
                      {t('admin.person.banDetail', { category: p.ban.category, note: p.ban.note })}{' '}
                      ({fmtMs(p.ban.created_at_ms)})
                    </dd>
                  </>
                )}
                <dt>{t('admin.person.dtSuspicion')}</dt>
                <dd>{p.suspicion_level}</dd>
                <dt>{t('admin.person.dtNotes')}</dt>
                <dd>{p.notes || t('admin.person.noNotes')}</dd>
              </dl>
              <div className="adm-actions">
                {viewer.can('MANAGE_ACCOUNTS') && (
                  <>
                    <Button
                      variant="secondary"
                      onClick={() => {
                        setNotes(p.notes);
                        setPending('notes');
                      }}
                    >
                      {t('admin.person.editNotes')}
                    </Button>
                    <Button
                      variant="secondary"
                      onClick={() => {
                        setLinkId('');
                        setPending('link');
                      }}
                    >
                      {t('admin.person.linkAccount')}
                    </Button>
                  </>
                )}
                {viewer.can('MANAGE_INVITES') && p.suspicion_level > 0 && (
                  <Button variant="secondary" onClick={() => setPending('clear')}>
                    {t('admin.person.clearSuspicion')}
                  </Button>
                )}
                {viewer.can('PLATFORM_BAN') && p.ban && !p.ban.lifted_at_ms && (
                  <Button variant="secondary" onClick={() => setPending('unban')}>
                    {t('admin.person.unban')}
                  </Button>
                )}
              </div>
              <section className="ss-card" aria-labelledby="adm-chain-h">
                <h2 id="adm-chain-h">{t('admin.person.chainTitle')}</h2>
                <p className="ss-sub">{t('admin.person.chainIntro')}</p>
                <ol className="ss-list" aria-label={t('admin.person.chainTitle')}>
                  {chain.map((a) => (
                    <li key={a.id} className="ss-row">
                      <div className="ss-grow">
                        <div className="ss-name">
                          @{a.username} <StateBadge state={a.state} />
                        </div>
                        <div className="ss-meta">
                          {t('admin.person.chainJoined', { when: fmtMs(a.created_at_ms) })}
                        </div>
                      </div>
                    </li>
                  ))}
                </ol>
              </section>
              {suspicion.data && (
                <section className="ss-card" aria-labelledby="adm-susp-h">
                  <h2 id="adm-susp-h">{t('admin.person.suspicionLogTitle')}</h2>
                  {suspicion.data.length === 0 ? (
                    <p className="ss-sub">{t('admin.person.noSuspicionLog')}</p>
                  ) : (
                    <ul className="ss-list" aria-label={t('admin.person.suspicionLogTitle')}>
                      {suspicion.data.map((e) => (
                        <li key={e.id} className="ss-row">
                          <div className="ss-grow">
                            <div className="ss-name">
                              {t('admin.person.suspicionStep', {
                                delta: e.delta > 0 ? `+${e.delta}` : String(e.delta),
                                level: e.level_after,
                              })}
                            </div>
                            <div className="ss-meta">
                              {fmtMs(e.created_at_ms)} · {e.reason}
                            </div>
                          </div>
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              )}
              {history.data && (
                <section className="ss-card" aria-labelledby="adm-phist-h">
                  <h2 id="adm-phist-h">{t('admin.person.historyTitle')}</h2>
                  {history.data.items.length === 0 ? (
                    <p className="ss-sub">{t('admin.person.noHistory')}</p>
                  ) : (
                    <ul className="ss-list" aria-label={t('admin.person.historyTitle')}>
                      {history.data.items.map((e) => (
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
              <ReasonDialog
                open={pending === 'notes'}
                title={t('admin.person.editNotes')}
                description={t('admin.person.notesDescription')}
                confirmLabel={t('admin.person.saveNotes')}
                reasonOptional
                valid={notes.length <= 4000}
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.setPersonNotes(personId, notes, reason);
                  setPending(null);
                  setNotice(t('admin.person.notesSaved'));
                  await res.reload();
                }}
              >
                <TextArea
                  label={t('admin.person.dtNotes')}
                  value={notes}
                  rows={6}
                  maxLength={4000}
                  onChange={(e) => setNotes(e.target.value)}
                />
              </ReasonDialog>
              <ReasonDialog
                open={pending === 'link'}
                title={t('admin.person.linkAccount')}
                description={t('admin.person.linkDescription')}
                confirmLabel={t('admin.person.linkConfirm')}
                valid={/^\d+$/.test(linkId.trim())}
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.linkAccount(personId, linkId.trim(), reason);
                  setPending(null);
                  setNotice(t('admin.person.linked'));
                  await Promise.all([res.reload(), suspicion.reload()]);
                }}
              >
                <TextInput
                  label={t('admin.person.linkAccountId')}
                  value={linkId}
                  inputMode="numeric"
                  onChange={(e) => setLinkId(e.target.value)}
                />
              </ReasonDialog>
              <ReasonDialog
                open={pending === 'clear'}
                title={t('admin.person.clearSuspicion')}
                description={t('admin.person.clearDescription')}
                confirmLabel={t('admin.person.clearSuspicion')}
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.clearSuspicion(personId, reason);
                  setPending(null);
                  setNotice(t('admin.person.cleared'));
                  await res.reload();
                }}
              />
              <ReasonDialog
                open={pending === 'unban'}
                title={t('admin.person.unban')}
                description={t('admin.person.unbanDescription')}
                confirmLabel={t('admin.person.unban')}
                onCancel={() => setPending(null)}
                onConfirm={async (reason) => {
                  await api.unbanPerson(personId, reason);
                  setPending(null);
                  setNotice(t('admin.person.unbanned'));
                  await res.reload();
                }}
              />
            </div>
          );
        }}
      </Async>
    </Drawer>
  );
}
