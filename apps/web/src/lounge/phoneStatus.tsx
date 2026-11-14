// SEC-AUD-206 (D-702): somebody who is online on a phone and nowhere else shows a phone glyph in the status dot's place.
import { Icon } from '@sideroom/ui';
import { t } from '../i18n';
import type { PresenceStatus } from '../store/types';

/**
 * The props an `Avatar` takes for a person's status: the glyph and its name for a phone-only person, the plain
 * status otherwise. Spread into every Avatar that draws a person's status, so they all agree.
 */
export function statusProps(
  p: { status: PresenceStatus; phone?: boolean | undefined } | undefined,
  fallback: PresenceStatus = 'offline',
) {
  const status = p?.status ?? fallback;
  if (!p?.phone || status !== 'online') return { status };
  return {
    status,
    statusIcon: <Icon name="phone" size={16} />,
    statusLabel: t('lounge.presence.phone'),
  };
}
