// The second level inside The Study: the shelves by source, the Cinema (media) and the Library (reading).
import { Link } from '@tanstack/react-router';
import { useT } from '../i18n';

export function StudyAreas({
  current,
  className,
  linkClassName,
}: {
  current: 'shelves' | 'cinema' | 'library';
  className?: string;
  linkClassName: string;
}) {
  const t = useT();
  return (
    <nav aria-label={t('study.rooms.studyAreas')} className={className} data-testid="study-areas">
      <Link
        to="/study"
        className={linkClassName}
        aria-current={current === 'shelves' ? 'page' : undefined}
      >
        {t('study.rooms.shelves')}
      </Link>
      <Link
        to="/cinema"
        className={linkClassName}
        aria-current={current === 'cinema' ? 'page' : undefined}
      >
        {t('study.rooms.cinema')}
      </Link>
      <Link
        to="/library"
        className={linkClassName}
        aria-current={current === 'library' ? 'page' : undefined}
      >
        {t('study.rooms.library')}
      </Link>
    </nav>
  );
}
