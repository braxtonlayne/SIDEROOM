import { Switch } from '@sideroom/ui';
import { Card, SectionTitle, Unavailable } from '../parts';
import type { PrefsFormProps } from './Appearance';

/**
 * Activity Privacy (LNG-USET-017, 018): what SIDE ROOM may show friends and shared servers about what you
 * watch and read. The choices live in the account's preferences (`user_prefs`); nothing publishes activity
 * yet, so they take effect when Cinema and Library start sharing it.
 */
export function ActivitySection({ prefs, setPrefs }: PrefsFormProps) {
  const off = !prefs.shareActivity;
  return (
    <>
      <SectionTitle>Activity Privacy</SectionTitle>
      <Card
        id="us-activity"
        title="Share what I watch and read"
        description="Friends and people in your shared servers can see it on your profile and next to your name."
      >
        <div className="us-stack">
          <Switch
            label="Share my activity"
            description="The master switch. Off hides everything below."
            checked={prefs.shareActivity}
            onChange={(e) => setPrefs({ shareActivity: e.target.checked })}
          />
          <Switch
            label="Share what I'm watching"
            checked={prefs.shareWatching}
            disabled={off}
            onChange={(e) => setPrefs({ shareWatching: e.target.checked })}
          />
          <Switch
            label="Share what I'm reading"
            checked={prefs.shareReading}
            disabled={off}
            onChange={(e) => setPrefs({ shareReading: e.target.checked })}
          />
          <Switch
            label="Hide age-restricted titles"
            description="Never show a title that is marked 18+, even when sharing is on."
            checked={prefs.hideAgeRestrictedActivity}
            disabled={off}
            onChange={(e) => setPrefs({ hideAgeRestrictedActivity: e.target.checked })}
          />
        </div>
      </Card>
      <Card id="us-activity-status" title="Status">
        <div className="us-stack">
          <Switch
            label="Show my current activity as my status"
            checked={prefs.activityAsStatus}
            disabled={off}
            onChange={(e) => setPrefs({ activityAsStatus: e.target.checked })}
          />
          <Switch
            label="Share detected activities"
            description="Include activities from connected apps once connections are available."
            checked={prefs.shareDetectedActivities}
            disabled={off}
            onChange={(e) => setPrefs({ shareDetectedActivities: e.target.checked })}
          />
        </div>
      </Card>
      <Card id="us-history" title="History">
        <Switch
          label="Pause my watch and reading history"
          description="Stop recording new entries. What is already recorded stays until you clear it."
          checked={prefs.pauseHistory}
          onChange={(e) => setPrefs({ pauseHistory: e.target.checked })}
        />
        <Unavailable>
          Nothing publishes your activity yet, and the watch and reading history lives with Cinema
          and Library. These choices are saved to your account now and apply as soon as those
          features share activity.
        </Unavailable>
      </Card>
    </>
  );
}
