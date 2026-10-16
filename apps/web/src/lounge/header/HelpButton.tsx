import { DropdownMenu, Icon } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState, type ComponentProps } from 'react';
import { useT } from '../../i18n';
import { ShortcutSheet } from '../ShortcutSheet';
import { useSupportEnabled } from '../shell/useSupportEnabled';

/** Header "?" menu (LNG-CHN-018): the keyboard shortcut sheet, and the support page when the operator turned it on. */
export function HelpButton({ iconSize = 20 }: { iconSize?: ComponentProps<typeof Icon>['size'] }) {
  const t = useT();
  const navigate = useNavigate();
  const supportOn = useSupportEnabled();
  const [sheet, setSheet] = useState(false);
  return (
    <>
      <DropdownMenu
        label={t('lounge.channelHeader.help')}
        items={[
          { label: t('lounge.helpMenu.shortcuts'), onSelect: () => setSheet(true) },
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
            className="lng-icon-btn"
            aria-label={t('lounge.channelHeader.help')}
          >
            <Icon name="info" size={iconSize} />
          </button>
        }
      />
      <ShortcutSheet open={sheet} onClose={() => setSheet(false)} />
    </>
  );
}
