import { Notice, Switch } from '@sideroom/ui';
import { useT } from '../../../i18n';
import { Card, SectionTitle } from '../parts';
import type { PrefsFormProps } from './Appearance';

/**
 * Streamer Mode (LNG-USET-055): one switch that hides what should not appear on a stream (your email, invite
 * links) and silences sounds. The options only apply while the master switch is on, once saved.
 */
export function StreamerSection({ prefs, setPrefs }: PrefsFormProps) {
  const t = useT();
  const off = !prefs.streamerMode;
  return (
    <>
      <SectionTitle>{t('settings.streamer.title')}</SectionTitle>
      <Card id="us-streamer" title={t('settings.streamer.cardTitle')}>
        <div className="us-stack">
          <Switch
            label={t('settings.streamer.enable')}
            description={t('settings.streamer.enableDesc')}
            checked={prefs.streamerMode}
            onChange={(e) => setPrefs({ streamerMode: e.target.checked })}
          />
          <Switch
            label={t('settings.streamer.hidePersonal')}
            description={t('settings.streamer.hidePersonalDesc')}
            checked={prefs.streamerHidePersonal}
            disabled={off}
            onChange={(e) => setPrefs({ streamerHidePersonal: e.target.checked })}
          />
          <Switch
            label={t('settings.streamer.hideInvites')}
            description={t('settings.streamer.hideInvitesDesc')}
            checked={prefs.streamerHideInvites}
            disabled={off}
            onChange={(e) => setPrefs({ streamerHideInvites: e.target.checked })}
          />
          <Switch
            label={t('settings.streamer.silence')}
            description={t('settings.streamer.silenceDesc')}
            checked={prefs.streamerSilence}
            disabled={off}
            onChange={(e) => setPrefs({ streamerSilence: e.target.checked })}
          />
        </div>
        <Notice variant="info" className="us-gap">
          {t('settings.streamer.note')}
        </Notice>
      </Card>
    </>
  );
}
