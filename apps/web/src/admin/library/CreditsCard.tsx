// Contributor credit adjustments (LIB-ADM-002) and the comment report queue: curators withdraw the
// credits of a submission for cause (D-074), and hide or dismiss reported comments.
import { Badge, Button, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { ApiProblem, FormDialog, fmtMs, useResource } from '../ui';
import * as api from './api';

export function CreditsCard() {
  const t = useT();
  const [draft, setDraft] = useState('');
  const [sid, setSid] = useState<string | null>(null);
  return (
    <section className="ss-card" aria-labelledby="lib-credits-h">
      <h2 id="lib-credits-h">{t('admin.library.creditsTitle')}</h2>
      <p className="ss-meta">{t('admin.library.creditsHint')}</p>
      <form
        className="ss-toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          setSid(draft.trim() || null);
        }}
      >
        <TextInput
          label={t('admin.library.submissionId')}
          inputMode="numeric"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <Button type="submit" disabled={!/^\d+$/.test(draft.trim())}>
          {t('admin.library.loadCredits')}
        </Button>
      </form>
      {sid && <Credits key={sid} submissionId={sid} />}
    </section>
  );
}

function Credits({ submissionId }: { submissionId: string }) {
  const t = useT();
  const res = useResource(`lib-credits:${submissionId}`, () => api.listCredits(submissionId));
  const [open, setOpen] = useState(false);
  const [reason, setReason] = useState('');
  const [error, setError] = useState<string | null>(null);
  const credits = res.data ?? [];
  const live = credits.filter((c) => !c.revoked_at_ms).length;
  return (
    <div>
      <ApiProblem error={error ?? res.error} />
      {credits.length === 0 ? (
        !res.loading && <p className="ss-na">{t('admin.library.noCredits')}</p>
      ) : (
        <ul className="ss-list" aria-label={t('admin.library.creditList')}>
          {credits.map((c) => (
            <li key={c.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {c.label || c.ref_id}{' '}
                  {c.revoked_at_ms ? (
                    <Badge variant="danger">{t('admin.library.revoked')}</Badge>
                  ) : (
                    <Badge variant="success">{t('admin.library.active')}</Badge>
                  )}
                </div>
                <div className="ss-meta">
                  {c.section} · {fmtMs(c.granted_at_ms)}
                  {c.revoke_reason ? ` · ${c.revoke_reason}` : ''}
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
      <div className="adm-actions">
        <Button variant="danger" disabled={live === 0} onClick={() => setOpen(true)}>
          {t('admin.library.revokeCredits')}
        </Button>
      </div>
      <FormDialog
        open={open}
        danger
        title={t('admin.library.revokeCredits')}
        description={t('admin.library.revokeDescription', { n: live })}
        confirmLabel={t('admin.library.revoke')}
        valid={reason.trim().length > 0}
        onCancel={() => setOpen(false)}
        onConfirm={async () => {
          setError(null);
          try {
            await api.revokeCredits(submissionId, reason.trim());
            setOpen(false);
            setReason('');
            await res.reload();
          } catch (e) {
            setError(api.errorText(e));
          }
        }}
      >
        <TextArea
          label={t('admin.library.revokeReason')}
          value={reason}
          rows={3}
          onChange={(e) => setReason(e.target.value)}
        />
      </FormDialog>
    </div>
  );
}

export function CommentReportsCard() {
  const t = useT();
  const res = useResource('lib-comment-reports', () => api.listCommentReports());
  const [error, setError] = useState<string | null>(null);
  const act = async (id: string, hide: boolean) => {
    setError(null);
    try {
      await api.resolveCommentReport(id, hide);
      await res.reload();
    } catch (e) {
      setError(api.errorText(e));
    }
  };
  const items = res.data?.items ?? [];
  return (
    <section className="ss-card" aria-labelledby="lib-reports-h">
      <h2 id="lib-reports-h">{t('admin.library.reportsTitle')}</h2>
      <ApiProblem error={error ?? res.error} />
      {items.length === 0 ? (
        !res.loading && <p className="ss-na">{t('admin.library.noReports')}</p>
      ) : (
        <ul className="ss-list" aria-label={t('admin.library.reportList')}>
          {items.map((r) => (
            <li key={r.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">{r.comment?.body ?? t('admin.library.commentGone')}</div>
                <div className="ss-meta">
                  {r.comment?.author_name ? `${r.comment.author_name} · ` : ''}
                  {r.reason} · {fmtMs(r.created_at_ms)}
                </div>
              </div>
              <Button size="sm" variant="danger" onClick={() => void act(r.id, true)}>
                {t('admin.library.hideComment')}
              </Button>
              <Button size="sm" variant="secondary" onClick={() => void act(r.id, false)}>
                {t('admin.library.dismiss')}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
