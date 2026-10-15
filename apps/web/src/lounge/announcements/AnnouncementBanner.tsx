import { Notice, type NoticeVariant } from '@sideroom/ui';
import { useT } from '../../i18n';
import {
  canDismiss,
  useActiveAnnouncements,
  type AnnouncementSeverity,
  type PlatformAnnouncement,
} from './useActiveAnnouncements';

const VARIANT: Record<AnnouncementSeverity, NoticeVariant> = {
  info: 'info',
  warning: 'warning',
  critical: 'danger',
};

/** The notices themselves, for a caller that already owns the data (and for tests). */
export function AnnouncementList({
  announcements,
  onDismiss,
}: {
  announcements: PlatformAnnouncement[];
  onDismiss: (id: string) => void;
}) {
  const t = useT();
  if (announcements.length === 0) return null;
  return (
    <section className="lng-announcements" aria-label={t('lounge.announcements.label')}>
      {announcements.map((a) => (
        <Notice
          key={a.id}
          variant={VARIANT[a.severity] ?? 'info'}
          title={a.title}
          {...(canDismiss(a) ? { onDismiss: () => onDismiss(a.id) } : {})}
        >
          {a.body}
        </Notice>
      ))}
    </section>
  );
}

/**
 * Platform announcement banner (ADM-PLT-009): polls `GET /announcements/active` and shows each notice in its
 * severity colour. Mount it once in the signed-in shell; it renders nothing while there is no notice.
 */
export function AnnouncementBanner({ pollMs, enabled }: { pollMs?: number; enabled?: boolean }) {
  const { announcements, dismiss } = useActiveAnnouncements(pollMs, enabled);
  return <AnnouncementList announcements={announcements} onDismiss={dismiss} />;
}
