import { Switch } from '@sideroom/ui';
import { useT } from '../../../i18n';
import { Card, SectionTitle, Unavailable } from '../parts';
import type { PrefsFormProps } from './Appearance';

/**
 * Activity Privacy (LNG-USET-017, 018): what SIDE ROOM may show friends and shared servers about what you
 * watch and read. The choices live in the account's preferences (`user_prefs`); nothing publishes activity
 * yet, so they take effect when Cinema and Library start sharing it.
 */
export function ActivitySection({ prefs, setPrefs }: PrefsFormProps) {
  const t = useT();
  const off = !prefs.shareActivity;
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
            label={t('settings.activity.shareMine')}
            description={t('settings.activity.shareMineDesc')}
            checked={prefs.shareActivity}
            onChange={(e) => setPrefs({ shareActivity: e.target.checked })}
          />
          <Switch
            label={t('settings.activity.shareWatching')}
            checked={prefs.shareWatching}
            disabled={off}
            onChange={(e) => setPrefs({ shareWatching: e.target.checked })}
          />
          <Switch
            label={t('settings.activity.shareReading')}
            checked={prefs.shareReading}
            disabled={off}
            onChange={(e) => setPrefs({ shareReading: e.target.checked })}
          />
          <Switch
            label={t('settings.activity.hideAge')}
            description={t('settings.activity.hideAgeDesc')}
            checked={prefs.hideAgeRestrictedActivity}
            disabled={off}
            onChange={(e) => setPrefs({ hideAgeRestrictedActivity: e.target.checked })}
          />
        </div>
      </Card>
      <Card id="us-activity-status" title={t('settings.activity.status')}>
        <div className="us-stack">
          <Switch
            label={t('settings.activity.asStatus')}
            checked={prefs.activityAsStatus}
            disabled={off}
            onChange={(e) => setPrefs({ activityAsStatus: e.target.checked })}
          />
          <Switch
            label={t('settings.activity.detected')}
            description={t('settings.activity.detectedDesc')}
            checked={prefs.shareDetectedActivities}
            disabled={off}
            onChange={(e) => setPrefs({ shareDetectedActivities: e.target.checked })}
          />
        </div>
      </Card>
      <Card id="us-history" title={t('settings.activity.history')}>
        <Switch
          label={t('settings.activity.pause')}
          description={t('settings.activity.pauseDesc')}
          checked={prefs.pauseHistory}
          onChange={(e) => setPrefs({ pauseHistory: e.target.checked })}
        />
        <Unavailable>{t('settings.activity.unavailable')}</Unavailable>
      </Card>
    </>
  );
}
