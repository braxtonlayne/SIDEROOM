import { DropdownMenu, Icon } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState, type ComponentProps } from 'react';
import { useT } from '../../i18n';
import { useNewsBadge } from '../../lib/whatsNew';
import { ShortcutSheet } from '../ShortcutSheet';
import { reopenFirstRun } from '../shell/firstRun';
import { useSupportEnabled } from '../shell/useSupportEnabled';

/**
 * Header "?" menu (LNG-CHN-018): the keyboard shortcut sheet, the handbook, what's new (with a one-time dot when
 * the version changed, RDY-ONB-003), the welcome again (RDY-ONB-001), and the support page when the operator
 * turned it on.
 */
export function HelpButton({ iconSize = 20 }: { iconSize?: ComponentProps<typeof Icon>['size'] }) {
  const t = useT();
  const navigate = useNavigate();
  const supportOn = useSupportEnabled();
  const news = useNewsBadge();
  const [sheet, setSheet] = useState(false);
  return (
    <>
      <DropdownMenu
        label={t('lounge.channelHeader.help')}
        items={[
          { label: t('lounge.helpMenu.shortcuts'), onSelect: () => setSheet(true) },
          { label: t('lounge.helpMenu.help'), onSelect: () => void navigate({ to: '/help' }) },
          {
            label: t('lounge.helpMenu.whatsNew'),
            onSelect: () => void navigate({ to: '/help/whats-new' }),
          },
          { label: t('lounge.helpMenu.welcome'), onSelect: () => void reopenFirstRun() },
          ...(supportOn
            ? [
                {
                  label: t('lounge.helpMenu.support'),
                  onSelect: () => void navigate({ to: '/support' }),
                },
              ]
            : []),
        ]}
        trigger={
          <button
            type="button"
            className="lng-icon-btn lng-help-trigger"
            aria-label={news ? t('lounge.helpMenu.whatsNewBadge') : t('lounge.channelHeader.help')}
            data-news={news ? '' : undefined}
          >
            <Icon name="info" size={iconSize} />
            {news && <span className="lng-help-dot" aria-hidden="true" />}
          </button>
        }
      />
      <ShortcutSheet open={sheet} onClose={() => setSheet(false)} />
    </>
  );
}
