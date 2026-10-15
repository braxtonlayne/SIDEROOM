import { Button, Icon, Modal, Tooltip } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { isIosSafari, promptInstall, usePwaState } from '../../pwa/state';

/**
 * Rail button that takes the place of Discord's "Download Apps" (LNG-RAIL-015): installs the PWA through the
 * browser's prompt, or shows the Add to Home Screen steps on iOS Safari. Hidden once installed or when the browser
 * offers neither.
 */
export function InstallButton() {
  const t = useT();
  const pwa = usePwaState();
  const [steps, setSteps] = useState(false);
  const ios = isIosSafari();
  if (pwa.installed || (!pwa.canInstall && !ios)) return null;
  const click = () => {
    if (pwa.canInstall) void promptInstall();
    else setSteps(true);
  };
  return (
    <li>
      <div className="lng-rail-item">
        <Tooltip content={t('lounge.guildRail.install')} placement="right" delay={150}>
          <button
            type="button"
            className="lng-guild lng-rail-action"
            aria-label={t('lounge.guildRail.install')}
            onClick={click}
          >
            <Icon name="download" size={24} />
          </button>
        </Tooltip>
      </div>
      {steps && (
        <Modal
          open
          onClose={() => setSteps(false)}
          size="sm"
          title={t('pwa.install.ios.title')}
          footer={
            <Button onClick={() => setSteps(false)}>{t('lounge.guildRail.installDone')}</Button>
          }
        >
          <ol>
            <li>{t('pwa.install.ios.step1')}</li>
            <li>{t('pwa.install.ios.step2')}</li>
            <li>{t('pwa.install.ios.step3')}</li>
          </ol>
        </Modal>
      )}
    </li>
  );
}
