import { Switch } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { saveActivity, useActivitySettings, type ActivitySettings } from '../activitySettings';
import { Card, SectionTitle, Unavailable } from '../parts';
import { CinemaDefaultsCard, CinemaHistoryCard, LibraryDefaultsCard } from './ActivityHistory';
import type { PrefsFormProps } from './Appearance';

/**
 * Activity Privacy (LNG-USET-017, 018): what SIDE ROOM may show friends and shared servers about what you
 * watch and read. The five sharing choices live on the server in the settings blob (`activity.*`, G-58,
 * D-190) and take effect at once: the gateway, trending, ratings and the profile read them. The status
 * choices stay in the browser preferences and are saved with the usual Save changes bar.
 */
export function ActivitySection({ prefs, setPrefs }: PrefsFormProps) {
  const t = useT();
  const activity = useActivitySettings();
  const [failed, setFailed] = useState(false);
  const toggle = (key: keyof ActivitySettings) => (e: { target: { checked: boolean } }) => {
    setFailed(false);
    saveActivity({ [key]: e.target.checked }).catch(() => setFailed(true));
  };
  return (
    <>
      <SectionTitle>{t('settings.activity.title')}</SectionTitle>
      <Card
        id="us-activity"
        title={t('settings.activity.shareTitle')}
        description={t('settings.activity.shareDesc')}
      >
        <div className="us-stack">
          <Switch
            label={t('settings.activity.shareWatching')}
            checked={activity.share_cinema}
            onChange={toggle('share_cinema')}
          />
          <Switch
            label={t('settings.activity.shareTv')}
            checked={activity.share_tv}
            onChange={toggle('share_tv')}
          />
          <Switch
            label={t('settings.activity.shareReading')}
            checked={activity.share_library}
            onChange={toggle('share_library')}
          />
          <Switch
            label={t('settings.activity.hideAge')}
            description={t('settings.activity.hideAgeDesc')}
            checked={activity.hide_age_restricted}
            onChange={toggle('hide_age_restricted')}
          />
          {failed && <p role="alert">{t('settings.activity.saveFailed')}</p>}
        </div>
      </Card>
      <Card id="us-activity-status" title={t('settings.activity.status')}>
        <div className="us-stack">
          <Switch
            label={t('settings.activity.asStatus')}
            checked={prefs.activityAsStatus}
            onChange={(e) => setPrefs({ activityAsStatus: e.target.checked })}
          />
          <Switch
            label={t('settings.activity.detected')}
            description={t('settings.activity.detectedDesc')}
            checked={prefs.shareDetectedActivities}
            onChange={(e) => setPrefs({ shareDetectedActivities: e.target.checked })}
          />
        </div>
      </Card>
      <Card id="us-history" title={t('settings.activity.history')}>
        <Switch
          label={t('settings.activity.pause')}
          description={t('settings.activity.pauseDesc')}
          checked={activity.paused}
          onChange={toggle('paused')}
        />
        <Unavailable>{t('settings.activity.unavailable')}</Unavailable>
      </Card>
      <CinemaHistoryCard />
      <CinemaDefaultsCard />
      <LibraryDefaultsCard />
    </>
  );
}
