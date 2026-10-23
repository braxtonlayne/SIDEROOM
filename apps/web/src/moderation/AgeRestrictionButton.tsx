// Staff buttons on the Library series page and the Cinema title page: mark the item 18+ or take the mark off
// (MOD-AGE-001; docs/specs/moderation-compliance.md section 7). They call `POST /moderation/age/restrictions`, which
// needs a reason and writes the access log. The server is the boundary: the button only shows for the bits that
// route accepts (`MANAGE_CASES`, `DELETE_CONTENT`, or the review bit of the kind), so it never offers a refusal.
import { Button, Modal, Notice, TextArea } from '@sideroom/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { getMe } from '../admin/api';
import { hasAny, parsePerms, type AdminPerm } from '../admin/permissions';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { setAgeRestricted } from './api';

export type AgeItemKind = 'library_series' | 'cinema_title';

const BITS: Record<AgeItemKind, AdminPerm[]> = {
  library_series: ['MANAGE_CASES', 'DELETE_CONTENT', 'REVIEW_READING'],
  cinema_title: ['MANAGE_CASES', 'DELETE_CONTENT', 'REVIEW_MEDIA'],
};

/** Whether the signed-in person may mark items of this kind. A failed read (not staff, signed out) means no. */
export function useMayMarkAge(kind: AgeItemKind): boolean {
  const q = useQuery({
    queryKey: ['moderation', 'staff-me'],
    queryFn: getMe,
    staleTime: 5 * 60_000,
    retry: false,
    refetchOnWindowFocus: false,
  });
  return q.data ? hasAny(parsePerms(q.data.platform_permissions), ...BITS[kind]) : false;
}

export function AgeRestrictionButton({
  kind,
  id,
  restricted,
  onChanged,
}: {
  kind: AgeItemKind;
  id: string;
  /** The item's current mark. */
  restricted: boolean;
  /** Called after the server accepted the change, so the page can refetch the item. */
  onChanged: () => void;
}) {
  const t = useT();
  const may = useMayMarkAge(kind);
  const [open, setOpen] = useState(false);
  const [reason, setReason] = useState('');
  const save = useMutation({
    mutationFn: () =>
      setAgeRestricted({ kind, id, restricted: !restricted, reason: reason.trim() }),
    onSuccess: () => {
      setOpen(false);
      setReason('');
      onChanged();
    },
  });
  if (!may) return null;
  const close = () => {
    if (save.isPending) return;
    setOpen(false);
    save.reset();
  };
  const error = save.error;
  return (
    <>
      <Button variant="secondary" onClick={() => setOpen(true)}>
        {restricted ? t('moderation.ageButton.unmark') : t('moderation.ageButton.mark')}
      </Button>
      <Modal
        open={open}
        onClose={close}
        title={
          restricted ? t('moderation.ageButton.unmarkTitle') : t('moderation.ageButton.markTitle')
        }
        description={
          restricted ? t('moderation.ageButton.unmarkHelp') : t('moderation.ageButton.markHelp')
        }
        size="sm"
        footer={
          <>
            <Button variant="ghost" onClick={close} disabled={save.isPending}>
              {t('moderation.ageButton.cancel')}
            </Button>
            <Button
              disabled={reason.trim().length === 0}
              loading={save.isPending}
              onClick={() => save.mutate()}
            >
              {restricted
                ? t('moderation.ageButton.unmarkConfirm')
                : t('moderation.ageButton.markConfirm')}
            </Button>
          </>
        }
      >
        <form
          className="mod-report-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (reason.trim() && !save.isPending) save.mutate();
          }}
        >
          <TextArea
            label={t('moderation.ageButton.reasonLabel')}
            value={reason}
            rows={3}
            maxLength={500}
            onChange={(e) => setReason(e.target.value)}
          />
          {error && (
            <Notice variant="danger" role="alert">
              {error instanceof ApiError && error.status === 403
                ? t('moderation.ageButton.errorDenied')
                : error instanceof ApiError && error.status === 404
                  ? t('moderation.ageButton.errorGone')
                  : t('moderation.ageButton.errorGeneric')}
            </Notice>
          )}
        </form>
      </Modal>
    </>
  );
}
