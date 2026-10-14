import { Notice, Switch } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { announce } from '../a11y/announcer';
import { useT } from '../i18n';
import { disablePush, enablePush, pushStatus, type PushStatus } from './push';

/** This device's Web Push toggle (XC-PWA-004). Lives in Settings, Notifications. */
export function WebPushCard() {
  const t = useT();
  const [status, setStatus] = useState<PushStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let live = true;
    void pushStatus().then((s) => live && setStatus(s));
    return () => {
      live = false;
    };
  }, []);

  const toggle = async (on: boolean) => {
    setBusy(true);
    setFailed(false);
    try {
      const next = on ? await enablePush(navigator.platform || 'This device') : await disablePush();
      setStatus(next);
      if (next.kind === 'on') announce(t('pwa.push.enabled'));
      else if (!on) announce(t('pwa.push.disabled'));
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  if (!status) return null;
  if (status.kind === 'unsupported')
    return (
      <Notice variant="info">
        {status.reason === 'no-browser-support' ? t('pwa.push.noBrowser') : t('pwa.push.noServer')}
      </Notice>
    );
  if (status.kind === 'blocked')
    return (
      <Notice variant="warning" title={t('pwa.push.blockedTitle')}>
        {t('pwa.push.blockedBody')}
      </Notice>
    );
  return (
    <div className="us-stack">
      <Switch
        label={t('pwa.push.label')}
        description={t('pwa.push.description')}
        checked={status.kind === 'on'}
        disabled={busy}
        onChange={(e) => void toggle(e.target.checked)}
      />
      {failed && (
        <Notice variant="danger" title={t('pwa.push.errorTitle')}>
          {t('pwa.push.errorBody')}
        </Notice>
      )}
    </div>
  );
}
