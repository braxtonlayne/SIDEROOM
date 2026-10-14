import { SegmentedControl, Switch } from '@sideroom/ui';
import { useT } from '../../../i18n';
import { Card, SectionTitle } from '../parts';
import type { Animation, RoleColors, UserPrefs } from '../prefs';
import type { PrefsFormProps } from './Appearance';

const animationOptions = (t: ReturnType<typeof useT>) => [
  { value: 'always', label: t('settings.accessibility.animAlways') },
  { value: 'hover', label: t('settings.accessibility.animHover') },
  { value: 'never', label: t('settings.accessibility.animNever') },
];

export function AccessibilitySection({ prefs, setPrefs }: PrefsFormProps) {
  const t = useT();
  return (
    <>
      <SectionTitle>{t('settings.accessibility.title')}</SectionTitle>
      <Card id="us-motion" title={t('settings.accessibility.motion')}>
        <SegmentedControl
          label={t('settings.accessibility.reducedMotion')}
          value={prefs.reducedMotion}
          onValueChange={(v) => setPrefs({ reducedMotion: v as UserPrefs['reducedMotion'] })}
          options={[
            { value: 'system', label: t('settings.accessibility.motionSystem') },
            { value: 'on', label: t('settings.accessibility.motionOn') },
            { value: 'off', label: t('settings.accessibility.motionOff') },
          ]}
        />
        <div className="us-stack us-after">
          <SegmentedControl
            label={t('settings.accessibility.stickers')}
            value={prefs.stickerAnimation}
            onValueChange={(v) => setPrefs({ stickerAnimation: v as Animation })}
            options={animationOptions(t)}
          />
          <SegmentedControl
            label={t('settings.accessibility.emoji')}
            value={prefs.emojiAnimation}
            onValueChange={(v) => setPrefs({ emojiAnimation: v as Animation })}
            options={animationOptions(t)}
          />
        </div>
      </Card>
      <Card id="us-contrast" title={t('settings.accessibility.contrast')}>
        <Switch
          label={t('settings.accessibility.highContrast')}
          description={t('settings.accessibility.highContrastDesc')}
          checked={prefs.highContrast}
          onChange={(e) => setPrefs({ highContrast: e.target.checked })}
        />
      </Card>
      <Card id="us-links" title={t('settings.accessibility.links')}>
        <Switch
          label={t('settings.accessibility.underline')}
          description={t('settings.accessibility.underlineDesc')}
          checked={prefs.underlineLinks}
          onChange={(e) => setPrefs({ underlineLinks: e.target.checked })}
        />
      </Card>
      <Card id="us-roles" title={t('settings.accessibility.roleColors')}>
        <SegmentedControl
          label={t('settings.accessibility.showRoleColors')}
          value={prefs.roleColors}
          onValueChange={(v) => setPrefs({ roleColors: v as RoleColors })}
          options={[
            { value: 'name', label: t('settings.accessibility.roleName') },
            { value: 'dot', label: t('settings.accessibility.roleDot') },
            { value: 'off', label: 'Off' },
          ]}
        />
        <p className="us-muted">{t('settings.accessibility.roleHelp')}</p>
      </Card>
      <Card id="us-font" title={t('settings.accessibility.reading')}>
        <div className="us-stack">
          <Switch
            label={t('settings.accessibility.dyslexia')}
            description={t('settings.accessibility.dyslexiaDesc')}
            checked={prefs.dyslexiaFont}
            onChange={(e) => setPrefs({ dyslexiaFont: e.target.checked })}
          />
          <Switch
            label={t('settings.accessibility.sendButton')}
            description={t('settings.accessibility.sendButtonDesc')}
            checked={prefs.showSendButton}
            onChange={(e) => setPrefs({ showSendButton: e.target.checked })}
          />
        </div>
      </Card>
    </>
  );
}
