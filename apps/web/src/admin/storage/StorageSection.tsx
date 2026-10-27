import { Badge, Button, EmptyState, Notice, Select } from '@sideroom/ui';
import { useState } from 'react';
import { useT, type MessageKey } from '../../i18n';
import { LoadMore, ReasonDialog, SectionHeader, fmtMs, usePaged } from '../ui';
import * as api from './api';

const STATUS_KEYS: Record<api.MountStatus, MessageKey> = {
  active: 'admin.storage.status.active',
  failing: 'admin.storage.status.failing',
  disabled: 'admin.storage.status.disabled',
  delisted: 'admin.storage.status.delisted',
};
const OWNER_KEYS: Record<api.AdminStorageMount['owner_kind'], MessageKey> = {
  user: 'admin.storage.ownerKind.user',
  guild: 'admin.storage.ownerKind.guild',
  platform: 'admin.storage.ownerKind.platform',
};

/** Admin > Storage (BYOS-LEG-001): every mount with its owner and status, and the whole-bucket delist. */
export function StorageSection() {
  const t = useT();
  const [status, setStatus] = useState<api.MountStatus | 'any'>('any');
  const [target, setTarget] = useState<{ mount: api.AdminStorageMount; relist: boolean } | null>(
    null,
  );
  const [notice, setNotice] = useState<string | null>(null);
  const paged = usePaged<api.AdminStorageMount>(`storage-mounts:${status}`, async (before) => {
    const rows = await api.listMounts({ status: status === 'any' ? '' : status, before });
    return { items: rows, next: rows.length >= api.PAGE ? rows[rows.length - 1]?.id : null };
  });

  return (
    <>
      <SectionHeader
        title={t('admin.storage.title')}
        description={t('admin.storage.description')}
      />
      <div className="ss-toolbar" role="group" aria-label={t('admin.storage.filters')}>
        <Select
          aria-label={t('admin.storage.statusFilter')}
          options={[
            { value: 'any', label: t('admin.storage.statusAny') },
            ...(Object.keys(STATUS_KEYS) as api.MountStatus[]).map((s) => ({
              value: s,
              label: t(STATUS_KEYS[s]),
            })),
          ]}
          value={status}
          onValueChange={(v) => setStatus(v as api.MountStatus | 'any')}
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
          title={t('admin.storage.emptyTitle')}
          description={t('admin.storage.emptyDescription')}
        />
      )}
      {paged.items.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.storage.listLabel')}>
          {paged.items.map((m) => (
            <li key={m.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {m.label}{' '}
                  <Badge variant={m.status === 'delisted' ? 'danger' : 'neutral'}>
                    {t(STATUS_KEYS[m.status])}
                  </Badge>{' '}
                  {m.delisted_objects > 0 && (
                    <Badge variant="warning">
                      {t('admin.storage.delistedObjects', { count: m.delisted_objects })}
                    </Badge>
                  )}
                </div>
                <div className="ss-meta">
                  {t('admin.storage.meta', {
                    owner: `${t(OWNER_KEYS[m.owner_kind])} ${m.owner_name || m.owner_id}`,
                    provider: m.provider,
                    bucket: m.bucket,
                    hint: m.key_hint,
                    when: fmtMs(m.created_at_ms),
                  })}
                </div>
                {m.status === 'delisted' && (
                  <div className="ss-meta">
                    {t('admin.storage.delistedSince', {
                      when: fmtMs(m.delisted_at_ms),
                      reason: m.delist_reason,
                    })}
                  </div>
                )}
              </div>
              {m.owner_kind !== 'platform' &&
                (m.status === 'delisted' ? (
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('admin.storage.relistAria', { label: m.label })}
                    onClick={() => setTarget({ mount: m, relist: true })}
                  >
                    {t('admin.storage.relistButton')}
                  </Button>
                ) : (
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={t('admin.storage.delistAria', { label: m.label })}
                    onClick={() => setTarget({ mount: m, relist: false })}
                  >
                    {t('admin.storage.delistButton')}
                  </Button>
                ))}
            </li>
          ))}
        </ul>
      )}
      <LoadMore paged={paged} />
      <ReasonDialog
        open={!!target}
        danger={!!target && !target.relist}
        title={t(target?.relist ? 'admin.storage.relistTitle' : 'admin.storage.delistTitle')}
        description={t(
          target?.relist ? 'admin.storage.relistDescription' : 'admin.storage.delistDescription',
        )}
        confirmLabel={t(
          target?.relist ? 'admin.storage.relistButton' : 'admin.storage.delistButton',
        )}
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          const { mount, relist } = target;
          if (relist) await api.relistMount(mount.id, reason);
          else await api.delistMount(mount.id, reason);
          setNotice(
            t(relist ? 'admin.storage.relistedNotice' : 'admin.storage.delistedNotice', {
              label: mount.label,
            }),
          );
          setTarget(null);
          paged.reload();
        }}
      />
    </>
  );
}
