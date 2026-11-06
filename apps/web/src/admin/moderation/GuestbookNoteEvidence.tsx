// D-507: the evidence card of a reported guestbook note. It shows the snapshot taken at report time (the text, who
// left it, whether it was waiting for the owner, whose page) and, for staff who may act on the case, the button
// that removes the note itself (the server writes the access-log row first; a note already gone answers 404).
// D-528: the removal asks for a reason (the same prompt a hold release uses); D-529: the card learns on the press
// (a success shows "Note removed", a 404 "This note was already removed"), with no lookup per card.
import { Badge, Button } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import * as api from '../../moderation/api';
import { ReasonDialog, fmtMs } from '../ui';

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
  const [asking, setAsking] = useState(false);
  const [outcome, setOutcome] = useState<'removed' | 'gone' | null>(null);
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
      {canRemove && n.id && outcome === null && (
        <Button variant="danger" size="sm" onClick={() => setAsking(true)}>
          {t('admin.moderation.guestbookNote.remove')}
        </Button>
      )}
      {outcome && (
        <span className="ss-meta" role="status">
          {outcome === 'removed'
            ? t('admin.moderation.guestbookNote.removedLine')
            : t('admin.moderation.guestbookNote.alreadyRemoved')}
        </span>
      )}
      <ReasonDialog
        open={asking}
        danger
        title={t('admin.moderation.guestbookNote.dialogTitle')}
        description={t('admin.moderation.guestbookNote.dialogDescription')}
        confirmLabel={t('admin.moderation.guestbookNote.remove')}
        onCancel={() => setAsking(false)}
        onConfirm={async (reason) => {
          try {
            await run(async () => {
              try {
                await api.removeGuestbookNote(n.id ?? '', reason);
                setOutcome('removed');
              } catch (e) {
                if (e instanceof ApiError && e.status === 404) setOutcome('gone');
                else throw e;
              }
            }, t('admin.moderation.guestbookNote.removed'));
          } finally {
            setAsking(false);
          }
        }}
      />
    </div>
  );
}
