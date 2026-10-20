// "Copyright notice TK-n" / "Illegal-content notice TK-n" on a case that was opened for a takedown notice (D-334).
// It opens the notice page when the viewer may (MANAGE_TAKEDOWNS); otherwise it is a plain badge.
import { Badge, Button } from '@sideroom/ui';
import { useT } from '../../i18n';
import type { CaseSummary } from '../../moderation/api';

export function TakedownBadge({
  takedown,
  onOpen,
}: {
  takedown: NonNullable<CaseSummary['takedown']>;
  onOpen?: ((noticeId: string) => void) | undefined;
}) {
  const t = useT();
  const label = t(
    takedown.kind === 'copyright'
      ? 'admin.moderation.takedownBadge.copyright'
      : 'admin.moderation.takedownBadge.illegal',
    { id: takedown.notice_id },
  );
  if (!onOpen) return <Badge variant="accent">{label}</Badge>;
  return (
    <Button
      size="sm"
      variant="ghost"
      aria-label={t('admin.moderation.takedownBadge.open', { id: takedown.notice_id })}
      onClick={() => onOpen(takedown.notice_id)}
    >
      {label}
    </Button>
  );
}
