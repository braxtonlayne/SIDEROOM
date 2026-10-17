// Packages tab of the Review Queues hub (EXT-PKG-003, REVIEW_PACKAGES): versions of themes, profile layouts,
// packs, recipes and server templates that creators submitted. Staff read the manifest summary and can download
// the zip; a rejection needs a reason the creator can read.
import { Badge, Button, EmptyState, Notice } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import {
  decidePackage,
  packageDownloadUrl,
  pendingPackages,
  type PendingVersion,
} from '../../packages/api';
import { useLoad } from '../../packages/useLoad';
import { ReasonDialog } from '../ui';

type Pending = { row: PendingVersion; decision: 'approve' | 'reject' } | null;

/** One line of what the payload does, for the reviewer. */
function summary(p: PendingVersion, t: ReturnType<typeof useT>): string {
  const pl = p.payload;
  switch (p.kind) {
    case 'theme':
      return t('admin.packages.summaryTheme', {
        count: Object.keys((pl.tokens as object) ?? {}).length,
        scheme: String(pl.scheme ?? ''),
      });
    case 'profile_layout':
      return t('admin.packages.summaryLayout', {
        count: ((pl.doc as { blocks?: unknown[] })?.blocks ?? []).length,
      });
    case 'recipe':
      return t('admin.packages.summaryRecipe', {
        modules: ((pl.features as { id: string }[]) ?? []).map((f) => f.id).join(', '),
      });
    case 'server_template':
      return t('admin.packages.summaryTemplate', {
        channels: (
          (pl.serialized_source_guild as { channels?: unknown[] } | undefined)?.channels ?? []
        ).length,
        roles: ((pl.serialized_source_guild as { roles?: unknown[] } | undefined)?.roles ?? [])
          .length,
      });
    default:
      return t('admin.packages.summaryPack', {
        count: ((pl.items as unknown[]) ?? []).length,
      });
  }
}

export function PackagesReview() {
  const t = useT();
  const list = useLoad<PendingVersion[]>('pkg-pending', pendingPackages);
  const [pending, setPending] = useState<Pending>(null);
  const [notice, setNotice] = useState<string | null>(null);
  return (
    <>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      {list.error && (
        <Notice variant="danger" title={t('admin.packages.loadFailed')}>
          {list.error}
        </Notice>
      )}
      <section className="ss-card" aria-labelledby="pkg-rev-h">
        <h2 id="pkg-rev-h">{t('admin.packages.title')}</h2>
        {list.data && list.data.length === 0 && <EmptyState title={t('admin.packages.empty')} />}
        <ul className="ss-list" aria-label={t('admin.packages.title')}>
          {(list.data ?? []).map((v) => (
            <li key={v.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {v.name} <Badge>{v.version}</Badge> <Badge variant="accent">{v.kind}</Badge>
                </div>
                <div className="ss-meta">
                  {t('admin.packages.by', { author: v.author_name, slug: v.slug })}
                </div>
                <div className="ss-meta">{summary(v, t)}</div>
                {v.warnings.length > 0 && (
                  <ul className="ss-meta" aria-label={t('admin.packages.warnings')}>
                    {v.warnings.map((w, i) => (
                      <li key={i}>{w}</li>
                    ))}
                  </ul>
                )}
                <details>
                  <summary>{t('admin.packages.files', { count: v.files.length })}</summary>
                  <ul className="ss-meta" aria-label={t('admin.packages.filesLabel')}>
                    {v.files.map((f) => (
                      <li key={f.path} className="adm-mono">
                        {f.path} ({f.size} B)
                      </li>
                    ))}
                  </ul>
                </details>
                <a href={packageDownloadUrl(v.id)} download rel="noreferrer">
                  {t('admin.packages.download', { name: v.name })}
                </a>
              </div>
              <Button
                size="sm"
                aria-label={t('admin.packages.approveAria', { name: v.name, version: v.version })}
                onClick={() => setPending({ row: v, decision: 'approve' })}
              >
                {t('admin.packages.approve')}
              </Button>
              <Button
                size="sm"
                variant="danger"
                aria-label={t('admin.packages.rejectAria', { name: v.name, version: v.version })}
                onClick={() => setPending({ row: v, decision: 'reject' })}
              >
                {t('admin.packages.reject')}
              </Button>
            </li>
          ))}
        </ul>
      </section>
      <ReasonDialog
        open={pending !== null}
        title={
          pending?.decision === 'approve'
            ? t('admin.packages.approveTitle')
            : t('admin.packages.rejectTitle')
        }
        confirmLabel={t('admin.packages.confirm')}
        danger={pending?.decision === 'reject'}
        reasonLabel={t('admin.packages.noteLabel')}
        reasonOptional={pending?.decision === 'approve'}
        onCancel={() => setPending(null)}
        onConfirm={async (note) => {
          if (!pending) return;
          await decidePackage(pending.row.id, pending.decision, note);
          list.reload();
          setPending(null);
          setNotice(t('admin.packages.decided'));
        }}
      />
    </>
  );
}
