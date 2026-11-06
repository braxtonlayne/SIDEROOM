// D-507: the evidence card of a reported guestbook note. It shows the snapshot taken at report time (the text, who
// left it, whether it was waiting for the owner, whose page) and, for staff who may act on the case, the button
// that removes the note itself (the server writes the access-log row first; a note already gone answers 404).
import { Badge, Button } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import * as api from '../../moderation/api';
import { fmtMs } from '../ui';

interface NoteSnapshot {
  id?: string;
  body?: string;
  anonymous?: boolean;
  author_id?: string | null;
  author_username?: string | null;
  profile_username?: string | null;
  status?: string;
  created_at_ms?: number;
}

export function GuestbookNoteEvidence({
  payload,
  canRemove,
  run,
}: {
  payload: Record<string, unknown>;
  canRemove: boolean;
  run: (fn: () => Promise<unknown>, done?: string) => Promise<void>;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const n = payload as NoteSnapshot;
  const who =
    n.anonymous || !n.author_id
      ? t('admin.moderation.guestbookNote.visitor')
      : t('admin.moderation.guestbookNote.author', { name: n.author_username ?? n.author_id });
  return (
    <div className="ss-row">
      <div className="ss-grow">
        <div className="ss-name">
          {who}{' '}
          <Badge variant={n.status === 'pending' ? 'warning' : 'neutral'}>
            {n.status === 'pending'
              ? t('admin.moderation.guestbookNote.pending')
              : t('admin.moderation.guestbookNote.approved')}
          </Badge>
        </div>
        <div>{n.body || t('admin.moderation.case.noText')}</div>
        <div className="ss-meta">
          {t('admin.moderation.guestbookNote.page', { name: n.profile_username ?? '' })}
          {typeof n.created_at_ms === 'number' ? ` · ${fmtMs(n.created_at_ms)}` : ''}
        </div>
      </div>
      {canRemove && n.id && (
        <Button
          variant="danger"
          size="sm"
          loading={busy}
          onClick={() => {
            setBusy(true);
            void run(
              () => api.removeGuestbookNote(n.id ?? ''),
              t('admin.moderation.guestbookNote.removed'),
            ).finally(() => setBusy(false));
          }}
        >
          {t('admin.moderation.guestbookNote.remove')}
        </Button>
      )}
    </div>
  );
}
