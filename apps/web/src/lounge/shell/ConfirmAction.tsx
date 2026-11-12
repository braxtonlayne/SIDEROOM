import { ConfirmDialog } from '@sideroom/ui';
import { useT } from '../../i18n';
import { failWith } from '../actionError';
import { blockUser, removeRelationship } from './api';

/** SEC-AUD-147 (D-638): what asks first. Cancel has the first focus and Escape cancels (`ConfirmDialog`). */
export type ConfirmKind = 'block' | 'removeFriend' | 'endPoll' | 'endStage';

export function ConfirmAction({
  kind,
  name,
  onConfirm,
  onCancel,
}: {
  kind: ConfirmKind | null;
  name?: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  // Keep the last kind while the dialog closes so its text does not blink.
  const k = kind ?? 'block';
  const who = name ?? '';
  const title =
    k === 'block'
      ? t('lounge.confirm.blockTitle', { name: who })
      : k === 'removeFriend'
        ? t('lounge.confirm.removeFriendTitle', { name: who })
        : k === 'endPoll'
          ? t('lounge.confirm.endPollTitle')
          : t('lounge.confirm.endStageTitle');
  const body =
    k === 'block'
      ? t('lounge.confirm.blockBody')
      : k === 'removeFriend'
        ? t('lounge.confirm.removeFriendBody')
        : k === 'endPoll'
          ? t('lounge.confirm.endPollBody')
          : t('lounge.confirm.endStageBody');
  const confirm =
    k === 'block'
      ? t('lounge.confirm.blockConfirm')
      : k === 'removeFriend'
        ? t('lounge.confirm.removeFriendConfirm')
        : k === 'endPoll'
          ? t('lounge.confirm.endPollConfirm')
          : t('lounge.confirm.endStageConfirm');
  return (
    <ConfirmDialog
      open={kind !== null}
      danger
      title={title}
      description={body}
      confirmLabel={confirm}
      onCancel={onCancel}
      onConfirm={onConfirm}
    />
  );
}

/** The shell's host for a confirm opened from a menu (`openDialog({ kind: 'confirmRelationship', ... })`). */
export function RelationshipConfirmDialog({
  action,
  userId,
  name,
  onClose,
}: {
  action: 'block' | 'removeFriend';
  userId: string;
  name: string;
  onClose: () => void;
}) {
  return (
    <ConfirmAction
      kind={action}
      name={name}
      onCancel={onClose}
      onConfirm={() => {
        onClose();
        // The gateway updates the lists; a failure is a toast, as it was before the confirm (SEC-AUD-036).
        if (action === 'block') void blockUser(userId).catch(failWith('lounge.friends.blockError'));
        else void removeRelationship(userId).catch(failWith('lounge.friends.removeError'));
      }}
    />
  );
}
