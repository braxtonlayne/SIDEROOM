// STU-NAV-001: the primary rail after the Entrance is three rooms: The Lounge, The Arcade and The Study. The Study
// opens on its shelves by source (`/study`, STU-SHF-001); Media (the Cinema) and Reading (the Library) are its second
// level (`StudyAreas`), and their own pages keep their nav.
import { Link } from '@tanstack/react-router';
import { useT } from '../i18n';
import { PreviewToggle } from '../showroom/PreviewToggle';
import { carryCall } from '../voice/carry';

export type RoomId = 'lounge' | 'arcade' | 'study';

/** Where the Study link goes: the shelves by source. */
export const STUDY_PATH = '/study';

/** The three rooms as a `<nav>`; `current` carries `aria-current`. */
export function RoomsNav({
  current,
  className,
  linkClassName,
}: {
  current: RoomId;
  className?: string;
  linkClassName: string;
}) {
  const t = useT();
  const cur = (id: RoomId) => (current === id ? ('page' as const) : undefined);
  return (
    <nav aria-label={t('study.rooms.label')} className={className} data-testid="rooms-nav">
      <Link to="/channels" className={linkClassName} aria-current={cur('lounge')}>
        {t('study.rooms.lounge')}
      </Link>
      {/* The Arcade (ARC-ROOM-001) always exists; the emulator switch only governs the emulator card inside it. */}
      <Link to="/games" className={linkClassName} aria-current={cur('arcade')} onClick={carryCall}>
        {t('study.rooms.arcade')}
      </Link>
      <Link to={STUDY_PATH} className={linkClassName} aria-current={cur('study')}>
        {t('study.rooms.study')}
      </Link>
      {/* UX-SHOW-001: shown only where the deployment switch `platform.showroom` is on. */}
      <PreviewToggle />
    </nav>
  );
}
