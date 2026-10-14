import { Badge, Button, EmptyState, Notice, Switch } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { LoadMore, ReasonDialog, SectionHeader, fmtMs, usePaged } from '../ui';

export function BansSection() {
  const t = useT();
  const [activeOnly, setActiveOnly] = useState(true);
  const [target, setTarget] = useState<api.BanRecord | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const paged = usePaged<api.BanRecord>(`bans:${activeOnly}`, async (after) => {
    const p = await api.listBans({ active_only: activeOnly, after, limit: 50 });
    return { items: p.items, next: p.next_after };
  });

  return (
    <>
      <SectionHeader title={t('admin.bans.title')} description={t('admin.bans.description')} />
      <div className="ss-toolbar">
        <Switch
          label={t('admin.bans.onlyActive')}
          checked={activeOnly}
          onChange={(e) => setActiveOnly(e.target.checked)}
        />
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      {paged.error && (
        <Notice variant="danger" role="alert" className="ss-status">
          {paged.error}
        </Notice>
      )}
      {!paged.error && !paged.loading && paged.items.length === 0 && (
        <EmptyState
          title={t('admin.bans.emptyTitle')}
          description={t('admin.bans.emptyDescription')}
        />
      )}
      {paged.items.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.bans.listLabel')}>
          {paged.items.map((b) => (
            <li key={b.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {t('admin.bans.person')} <span className="adm-mono">{b.person_id}</span>{' '}
                  <Badge variant={b.category === 'serious' ? 'danger' : 'warning'}>
                    {b.category}
                  </Badge>{' '}
                  {b.lifted_at_ms ? (
                    <Badge>{t('admin.bans.lifted')}</Badge>
                  ) : (
                    <Badge variant="danger">{t('admin.bans.active')}</Badge>
                  )}
                </div>
                <div className="ss-meta">
                  {b.note || t('admin.bans.noNote')} ·{' '}
                  {t('admin.bans.banned', { when: fmtMs(b.created_at_ms) })}
                  {b.lifted_at_ms
                    ? ` · ${t('admin.bans.liftedAt', { when: fmtMs(b.lifted_at_ms) })}`
                    : ''}
                </div>
              </div>
              {!b.lifted_at_ms && (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={t('admin.bans.liftAria', { id: b.person_id })}
                  onClick={() => setTarget(b)}
                >
                  {t('admin.bans.liftButton')}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      <LoadMore paged={paged} />
      <ReasonDialog
        open={!!target}
        title={t('admin.bans.liftTitle')}
        description={t('admin.bans.liftDescription')}
        confirmLabel={t('admin.bans.liftButton')}
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          const r = await api.unbanPerson(target.person_id, reason);
          setNotice(
            t('admin.bans.liftedNotice', {
              reactivated: r.accounts_reactivated,
              left: r.accounts_left_deactivated,
            }),
          );
          setTarget(null);
          paged.reload();
        }}
      />
    </>
  );
}
