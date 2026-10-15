import { Icon, Tooltip } from '@sideroom/ui';
import { useT } from '../../i18n';
import type { BadgeView } from './badges';

/** The badge row of a profile; each badge has a tooltip and an accessible name. */
export function Badges({ badges }: { badges: BadgeView[] }) {
  const t = useT();
  if (badges.length === 0) return null;
  return (
    <ul className="prf-badges" aria-label={t('lounge.badges.list')}>
      {badges.map((b) => (
        <li key={b.key}>
          <Tooltip content={b.description} placement="top" delay={100}>
            <span
              className="prf-badge"
              data-kind={b.kind}
              tabIndex={0}
              role="img"
              aria-label={b.label}
            >
              {b.text ? (
                <span className="prf-badge-text">{b.text}</span>
              ) : (
                <Icon name={b.icon} size={16} />
              )}
            </span>
          </Tooltip>
        </li>
      ))}
    </ul>
  );
}
