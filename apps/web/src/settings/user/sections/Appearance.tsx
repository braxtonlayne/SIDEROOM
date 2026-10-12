import { SegmentedControl, Slider, THEMES } from '@sideroom/ui';
import { Card, SectionTitle } from '../parts';
import {
  FONT_SCALE_RANGE,
  THEME_CHOICES,
  ZOOM_RANGE,
  type MessageDisplay,
  type ThemeChoice,
  type UserPrefs,
} from '../prefs';

const THEME_LABEL: Record<ThemeChoice, string> = {
  system: 'Sync with computer',
  'lounge-dark': 'Dark',
  'lounge-light': 'Light',
  'lounge-ash': 'Ash',
  'lounge-onyx': 'Onyx',
};

export interface PrefsFormProps {
  prefs: UserPrefs;
  setPrefs: (patch: Partial<UserPrefs>) => void;
}

function Swatch({ choice }: { choice: ThemeChoice }) {
  if (choice === 'system')
    return (
      <span className="us-theme-swatch" aria-hidden="true">
        <span style={{ background: THEMES['lounge-dark'].tokens['--ui-bg'] }} />
        <span style={{ background: THEMES['lounge-light'].tokens['--ui-bg'] }} />
      </span>
    );
  const t = THEMES[choice].tokens;
  return (
    <span className="us-theme-swatch" aria-hidden="true">
      <span style={{ background: t['--ui-bg'] }} />
      <span style={{ background: t['--ui-bg-raised'] }} />
      <span style={{ background: t['--ui-accent'] }} />
    </span>
  );
}

export function AppearanceSection({ prefs, setPrefs }: PrefsFormProps) {
  return (
    <>
      <SectionTitle>Appearance</SectionTitle>
      <p className="us-muted">
        Changes preview instantly. Save keeps them in this browser; the server has no settings store
        yet, so they don&apos;t follow you to other devices.
      </p>
      <Card id="us-theme" title="Theme">
        <div className="us-themes" role="group" aria-label="Theme">
          {(['system', ...THEME_CHOICES] as ThemeChoice[]).map((c) => (
            <button
              key={c}
              type="button"
              className="us-theme"
              aria-pressed={prefs.theme === c}
              onClick={() => setPrefs({ theme: c })}
            >
              <Swatch choice={c} />
              <span className="us-theme-name">{THEME_LABEL[c]}</span>
            </button>
          ))}
        </div>
      </Card>
      <Card id="us-messages" title="Message display">
        <SegmentedControl
          label="Message display"
          value={prefs.messageDisplay}
          onValueChange={(v) => setPrefs({ messageDisplay: v as MessageDisplay })}
          options={[
            { value: 'cozy', label: 'Cozy' },
            { value: 'compact', label: 'Compact' },
          ]}
        />
        <p
          className="us-compact-sample"
          data-compact={prefs.messageDisplay === 'compact' || undefined}
        >
          <strong>Alex</strong> <span className="us-muted">12:04</span>
          <br />
          <span className="us-sample-line">This is how a message looks.</span>
        </p>
      </Card>
      <Card id="us-scale" title="Size and color">
        <div className="us-stack">
          <Slider
            label="Font scale"
            min={FONT_SCALE_RANGE[0]}
            max={FONT_SCALE_RANGE[1]}
            step={5}
            value={prefs.fontScale}
            showValue
            formatValue={(v) => `${v}%`}
            onValueChange={(v) => setPrefs({ fontScale: v })}
          />
          <Slider
            label="Zoom"
            min={ZOOM_RANGE[0]}
            max={ZOOM_RANGE[1]}
            step={10}
            value={prefs.zoom}
            showValue
            formatValue={(v) => `${v}%`}
            onValueChange={(v) => setPrefs({ zoom: v })}
          />
          <Slider
            label="Saturation"
            min={0}
            max={100}
            step={5}
            value={prefs.saturation}
            showValue
            formatValue={(v) => `${v}%`}
            onValueChange={(v) => setPrefs({ saturation: v })}
          />
        </div>
      </Card>
      <Card id="us-motion" title="Accessibility">
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
      </Card>
    </>
  );
}
