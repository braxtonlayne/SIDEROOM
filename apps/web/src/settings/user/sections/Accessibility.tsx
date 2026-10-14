import { SegmentedControl, Switch } from '@sideroom/ui';
import { Card, SectionTitle } from '../parts';
import type { Animation, RoleColors, UserPrefs } from '../prefs';
import type { PrefsFormProps } from './Appearance';

const ANIMATION = [
  { value: 'always', label: 'Always' },
  { value: 'hover', label: 'On hover or focus' },
  { value: 'never', label: 'Never' },
];

export function AccessibilitySection({ prefs, setPrefs }: PrefsFormProps) {
  return (
    <>
      <SectionTitle>Accessibility</SectionTitle>
      <Card id="us-motion" title="Motion">
        <SegmentedControl
          label="Reduced motion"
          value={prefs.reducedMotion}
          onValueChange={(v) => setPrefs({ reducedMotion: v as UserPrefs['reducedMotion'] })}
          options={[
            { value: 'system', label: 'Sync with system' },
            { value: 'on', label: 'On' },
            { value: 'off', label: 'Off' },
          ]}
        />
        <div className="us-stack us-after">
          <SegmentedControl
            label="Play animated stickers"
            value={prefs.stickerAnimation}
            onValueChange={(v) => setPrefs({ stickerAnimation: v as Animation })}
            options={ANIMATION}
          />
          <SegmentedControl
            label="Play animated emoji"
            value={prefs.emojiAnimation}
            onValueChange={(v) => setPrefs({ emojiAnimation: v as Animation })}
            options={ANIMATION}
          />
        </div>
      </Card>
      <Card id="us-contrast" title="Contrast">
        <Switch
          label="High contrast"
          description="Uses full-strength text and stronger borders everywhere."
          checked={prefs.highContrast}
          onChange={(e) => setPrefs({ highContrast: e.target.checked })}
        />
      </Card>
      <Card id="us-links" title="Links">
        <Switch
          label="Always underline links"
          description="Links are underlined without hovering, so they don't rely on colour alone."
          checked={prefs.underlineLinks}
          onChange={(e) => setPrefs({ underlineLinks: e.target.checked })}
        />
      </Card>
      <Card id="us-roles" title="Role colors">
        <SegmentedControl
          label="Show role colors"
          value={prefs.roleColors}
          onValueChange={(v) => setPrefs({ roleColors: v as RoleColors })}
          options={[
            { value: 'name', label: 'Next to names' },
            { value: 'dot', label: 'As a dot' },
            { value: 'off', label: 'Off' },
          ]}
        />
        <p className="us-muted">
          &quot;Next to names&quot; colours the name itself; &quot;As a dot&quot; keeps the name in
          the normal text colour, which reads better for low-contrast role colours.
        </p>
      </Card>
      <Card id="us-font" title="Reading">
        <div className="us-stack">
          <Switch
            label="Dyslexia-friendly text"
            description="A clearer typeface with wider letter and word spacing."
            checked={prefs.dyslexiaFont}
            onChange={(e) => setPrefs({ dyslexiaFont: e.target.checked })}
          />
          <Switch
            label="Show the send-message button"
            description="A button next to the message box, for touch screens and switch access."
            checked={prefs.showSendButton}
            onChange={(e) => setPrefs({ showSendButton: e.target.checked })}
          />
        </div>
      </Card>
    </>
  );
}
