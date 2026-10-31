// STU-NAV-001: the primary rail after the Entrance is three rooms: The Lounge, The Arcade and The Study. The Study
// holds the Cinema (media and file browser) and the Library (readers); its own pages keep their second-level nav.
import { Link } from '@tanstack/react-router';
import { useT } from '../i18n';
import { carryCall } from '../voice/carry';
import { useNavVisibility } from '../storage/navModes';
import { useEmulatorOn } from '../emu/SwitchedOff';

export type RoomId = 'lounge' | 'arcade' | 'study';

/** The Study opens on the Cinema unless the member hid that tab (their navigation preference). */
export function studyTarget(nav: { cinema: boolean; library: boolean }): '/cinema' | '/library' {
  return !nav.cinema && nav.library ? '/library' : '/cinema';
}

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
  const nav = useNavVisibility();
  const arcadeOn = useEmulatorOn();
  const cur = (id: RoomId) => (current === id ? ('page' as const) : undefined);
  return (
    <nav aria-label={t('study.rooms.label')} className={className} data-testid="rooms-nav">
      <Link to="/channels" className={linkClassName} aria-current={cur('lounge')}>
        {t('study.rooms.lounge')}
      </Link>
      {arcadeOn && (
        <Link
          to="/games"
          className={linkClassName}
          aria-current={cur('arcade')}
          onClick={carryCall}
        >
          {t('study.rooms.arcade')}
        </Link>
      )}
      <Link to={studyTarget(nav)} className={linkClassName} aria-current={cur('study')}>
        {t('study.rooms.study')}
      </Link>
    </nav>
  );
}
