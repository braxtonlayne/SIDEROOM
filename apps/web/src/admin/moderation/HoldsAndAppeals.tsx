// The holds list (MOD-HOLD-004; spec 9.4) and the appeals list (MOD-APL-005; D-215).
import { Badge, Button, EmptyState, Notice, Switch } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import * as api from '../../moderation/api';
import { ApiProblem, ReasonDialog, fmtMs, useResource } from '../ui';

export function HoldsTab({ onOpenCase }: { onOpenCase: (id: string) => void }) {
  const t = useT();
  const [activeOnly, setActiveOnly] = useState(true);
  const [target, setTarget] = useState<api.HoldView | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const res = useResource(`holds:${activeOnly}`, () =>
    api.listHolds({ active_only: activeOnly, limit: 100 }),
  );
  const keep = async (h: api.HoldView) => {
    setError(null);
    try {
      await api.markHoldReviewed(h.id);
      setNotice(t('admin.moderation.holds.keptNotice'));
      await res.reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : t('admin.moderation.case.failed'));
    }
  };
  return (
    <>
      <div className="ss-toolbar">
        <Switch
          label={t('admin.moderation.holds.onlyActive')}
          checked={activeOnly}
          onChange={(e) => setActiveOnly(e.target.checked)}
        />
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <ApiProblem error={error ?? res.error} />
      {res.data && res.data.length === 0 && (
        <EmptyState
          title={t('admin.moderation.holds.emptyTitle')}
          description={t('admin.moderation.holds.emptyDescription')}
        />
      )}
      {res.data && res.data.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.moderation.holds.listLabel')}>
          {res.data.map((h) => (
            <li key={h.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {h.released_at_ms ? (
                    <Badge>{t('admin.moderation.hold.released')}</Badge>
                  ) : (
                    <Badge variant="warning">{t('admin.moderation.hold.inForce')}</Badge>
                  )}{' '}
                  {h.scope_kind ?? ''} {h.scope_id ?? ''}
                  {h.review_due && (
                    <>
                      {' '}
                      <Badge variant="danger">{t('admin.moderation.hold.reviewDue')}</Badge>
                    </>
                  )}
                </div>
                <div className="ss-meta">
                  {h.reason} ·{' '}
                  {t('admin.moderation.hold.size', {
                    messages: h.messages,
                    attachments: h.attachments,
                    submissions: h.submissions,
                  })}{' '}
                  · {t('admin.moderation.holds.placed', { when: fmtMs(h.created_at_ms) })}
                </div>
              </div>
              {h.case_id && (
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={t('admin.moderation.queue.openAria', { id: h.case_id })}
                  onClick={() => onOpenCase(h.case_id!)}
                >
                  {t('admin.moderation.queue.open')}
                </Button>
              )}
              {!h.released_at_ms && (
                <>
                  {h.review_due && (
                    <Button
                      size="sm"
                      variant="secondary"
                      aria-label={t('admin.moderation.holds.keepAria', { id: h.id })}
                      onClick={() => void keep(h)}
                    >
                      {t('admin.moderation.holds.keep')}
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('admin.moderation.hold.releaseAria', { id: h.id })}
                    onClick={() => setTarget(h)}
                  >
                    {t('admin.moderation.hold.release')}
                  </Button>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
      <ReasonDialog
        open={!!target}
        title={t('admin.moderation.dialog.releaseTitle')}
        description={t('admin.moderation.dialog.releaseDescription')}
        confirmLabel={t('admin.moderation.dialog.releaseConfirm')}
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          await api.releaseHold(target.id, reason);
          setTarget(null);
          setNotice(t('admin.moderation.dialog.releasedNotice'));
          await res.reload();
        }}
      />
    </>
  );
}

export function AppealsTab({ onOpenCase }: { onOpenCase: (id: string) => void }) {
  const t = useT();
  const [openOnly, setOpenOnly] = useState(true);
  const res = useResource(`appeals:${openOnly}`, () =>
    api.listAppeals({ open_only: openOnly, limit: 100 }),
  );
  return (
    <>
      <div className="ss-toolbar">
        <Switch
          label={t('admin.moderation.appeals.onlyOpen')}
          checked={openOnly}
          onChange={(e) => setOpenOnly(e.target.checked)}
        />
      </div>
      <ApiProblem error={res.error} />
      {res.data && res.data.length === 0 && (
        <EmptyState
          title={t('admin.moderation.appeals.emptyTitle')}
          description={t('admin.moderation.appeals.emptyDescription')}
        />
      )}
      {res.data && res.data.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.moderation.appeals.listLabel')}>
          {res.data.map((a) => (
            <li key={a.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  <Badge variant={a.state === 'open' ? 'accent' : 'neutral'}>
                    {t('admin.moderation.appeal.state', { state: a.state })}
                  </Badge>{' '}
                  {t('admin.moderation.appeals.account', { id: a.appellant_account_id })}
                </div>
                <div>{a.message}</div>
                <div className="ss-meta">{fmtMs(a.created_at_ms)}</div>
              </div>
              {a.case_id && (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={t('admin.moderation.queue.openAria', { id: a.case_id })}
                  onClick={() => onOpenCase(a.case_id!)}
                >
                  {t('admin.moderation.queue.open')}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
