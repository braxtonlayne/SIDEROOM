// Directory tab of the Review Queues hub (ADM-COM-001, MANAGE_DIRECTORY): the Server Directory queue with
// request detail, a preview of the listing as members will see it (banner, no join), approve / reject with a
// note, remove and feature. The category list is a constant of the server; hiding a category is the
// `directory.hidden_categories` platform setting, so there is no categories manager here.
import { Avatar, Badge, Button, EmptyState, Modal, Notice } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { cdnUrl } from '../../store/cdn';
import { ActionStatus, Async, ReasonDialog, useAction, useResource } from '../ui';

const STATUSES: api.DirectoryStatus[] = ['pending', 'approved', 'rejected', 'removed'];

type Decision = { row: api.DirectoryListing; kind: 'approve' | 'reject' | 'remove' };

export function DirectoryQueue() {
  const t = useT();
  const [status, setStatus] = useState<api.DirectoryStatus>('pending');
  const res = useResource(`admin:directory:${status}`, () => api.listDirectoryQueue(status));
  const [deciding, setDeciding] = useState<Decision | null>(null);
  const [previewing, setPreviewing] = useState<api.DirectoryListing | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const action = useAction();

  const feature = (row: api.DirectoryListing) =>
    void action
      .run(async () => {
        await api.decideDirectoryListing(row.guild_id, {
          decision: 'approve',
          featured: !row.featured,
        });
        return true;
      })
      .then((ok) => {
        if (ok) void res.reload();
      });

  return (
    <>
      <p className="ss-sub">{t('admin.directoryQueue.intro')}</p>
      <div className="adm-actions" role="group" aria-label={t('admin.directoryQueue.statusLabel')}>
        {STATUSES.map((s) => (
          <Button
            key={s}
            size="sm"
            variant={s === status ? 'primary' : 'secondary'}
            aria-pressed={s === status}
            onClick={() => {
              setNotice(null);
              setStatus(s);
            }}
          >
            {t(`admin.directoryQueue.status.${s}`)}
          </Button>
        ))}
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <ActionStatus error={action.error} />
      <Async res={res} label={t('admin.directoryQueue.loading')}>
        {(rows) =>
          rows.length === 0 ? (
            <EmptyState title={t(`admin.directoryQueue.empty.${status}`)} />
          ) : (
            <ul className="ss-list" aria-label={t('admin.directoryQueue.listLabel')}>
              {rows.map((r) => (
                <li key={r.guild_id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">
                      {r.guild_name ?? r.guild_id} <Badge>{r.category}</Badge>
                      {r.featured && (
                        <Badge variant="accent">{t('admin.directoryQueue.featured')}</Badge>
                      )}
                      {!r.community_enabled && (
                        <Badge variant="warning">{t('admin.directoryQueue.communityOff')}</Badge>
                      )}
                    </div>
                    <div className="ss-meta">
                      {r.description || t('admin.directoryQueue.noDescription')}
                    </div>
                    <div className="ss-meta">
                      {t('admin.directoryQueue.meta', {
                        members: r.approximate_member_count,
                        date: r.requested_at.slice(0, 10),
                      })}
                    </div>
                    {r.decision_note && (
                      <div className="ss-meta">
                        {t('admin.directoryQueue.note', { note: r.decision_note })}
                      </div>
                    )}
                  </div>
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('admin.directoryQueue.previewAria', {
                      name: r.guild_name ?? r.guild_id,
                    })}
                    onClick={() => setPreviewing(r)}
                  >
                    {t('admin.directoryQueue.preview')}
                  </Button>
                  {r.status === 'pending' && (
                    <>
                      <Button
                        size="sm"
                        aria-label={t('admin.directoryQueue.approveAria', {
                          name: r.guild_name ?? r.guild_id,
                        })}
                        onClick={() => setDeciding({ row: r, kind: 'approve' })}
                      >
                        {t('admin.directoryQueue.approve')}
                      </Button>
                      <Button
                        size="sm"
                        variant="danger"
                        aria-label={t('admin.directoryQueue.rejectAria', {
                          name: r.guild_name ?? r.guild_id,
                        })}
                        onClick={() => setDeciding({ row: r, kind: 'reject' })}
                      >
                        {t('admin.directoryQueue.reject')}
                      </Button>
                    </>
                  )}
                  {r.status === 'approved' && (
                    <>
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-pressed={r.featured}
                        aria-label={t(
                          r.featured
                            ? 'admin.directoryQueue.unfeatureAria'
                            : 'admin.directoryQueue.featureAria',
                          { name: r.guild_name ?? r.guild_id },
                        )}
                        onClick={() => feature(r)}
                      >
                        {r.featured
                          ? t('admin.directoryQueue.unfeature')
                          : t('admin.directoryQueue.feature')}
                      </Button>
                      <Button
                        size="sm"
                        variant="danger"
                        aria-label={t('admin.directoryQueue.removeAria', {
                          name: r.guild_name ?? r.guild_id,
                        })}
                        onClick={() => setDeciding({ row: r, kind: 'remove' })}
                      >
                        {t('admin.directoryQueue.remove')}
                      </Button>
                    </>
                  )}
                </li>
              ))}
            </ul>
          )
        }
      </Async>

      {previewing && <PreviewDialog row={previewing} onClose={() => setPreviewing(null)} />}
      <ReasonDialog
        open={deciding !== null}
        title={
          deciding
            ? t(`admin.directoryQueue.${deciding.kind}Title`, {
                name: deciding.row.guild_name ?? deciding.row.guild_id,
              })
            : ''
        }
        description={t('admin.directoryQueue.noteDescription')}
        confirmLabel={deciding ? t(`admin.directoryQueue.${deciding.kind}`) : ''}
        danger={deciding?.kind !== 'approve'}
        reasonOptional
        reasonLabel={t('admin.directoryQueue.noteLabel')}
        onCancel={() => setDeciding(null)}
        onConfirm={async (note) => {
          if (!deciding) return;
          await api.decideDirectoryListing(deciding.row.guild_id, {
            decision: deciding.kind,
            ...(note ? { note } : {}),
          });
          setDeciding(null);
          setNotice(t(`admin.directoryQueue.${deciding.kind}Done`));
          void res.reload();
        }}
      />
    </>
  );
}

/** The directory server page in preview mode: the draft as members will see it, with a banner and no Join. */
function PreviewDialog({ row, onClose }: { row: api.DirectoryListing; onClose: () => void }) {
  const t = useT();
  const res = useResource(`admin:directory-preview:${row.guild_id}`, () =>
    api.previewDirectoryListing(row.guild_id),
  );
  return (
    <Modal
      open
      onClose={onClose}
      title={t('admin.directoryQueue.previewTitle', { name: row.guild_name ?? row.guild_id })}
      size="md"
      footer={<Button onClick={onClose}>{t('admin.directoryQueue.close')}</Button>}
    >
      <Notice variant="info" role="note" className="ss-status">
        {t('admin.directoryQueue.previewBanner')}
      </Notice>
      <Async res={res} label={t('admin.directoryQueue.loading')}>
        {(g) => {
          const banner = cdnUrl('icons', g.id, g.banner, { size: 512 });
          return (
            <article className="lng-dir-card" aria-label={g.name}>
              <div
                className="lng-dir-banner"
                aria-hidden="true"
                style={banner ? { backgroundImage: `url(${banner})` } : undefined}
              />
              <div className="lng-dir-body">
                <Avatar name={g.name} src={cdnUrl('icons', g.id, g.icon, { size: 64 })} size={56} />
                <h3 className="lng-dir-name">
                  {g.name} <Badge>{g.category}</Badge>
                </h3>
                <p className="lng-dir-desc">
                  {g.description || t('admin.directoryQueue.noDescription')}
                </p>
                <p className="lng-muted lng-dir-meta">
                  {t('admin.directoryQueue.members', { count: g.approximate_member_count })}
                </p>
              </div>
            </article>
          );
        }}
      </Async>
    </Modal>
  );
}
