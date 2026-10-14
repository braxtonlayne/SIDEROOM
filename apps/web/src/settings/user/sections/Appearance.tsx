import { SegmentedControl, Select, Slider, Switch, THEMES, THEME_IDS } from '@sideroom/ui';
import type { ThemeId } from '@sideroom/ui';
import { useLoungeUi } from '../../../lounge/store';
import { Card, SectionTitle } from '../parts';
import {
  FONT_SCALE_RANGE,
  MESSAGE_GAP_RANGE,
  THEME_CHOICES,
  THEMED_SECTIONS,
  ZOOM_RANGE,
  type MessageDisplay,
  type ThemeChoice,
  type ThemedSection,
  type UserPrefs,
} from '../prefs';

const THEME_LABEL: Record<ThemeChoice, string> = {
  system: 'Sync with computer',
  'lounge-dark': 'Dark',
  'lounge-light': 'Light',
  'lounge-ash': 'Ash',
  'lounge-onyx': 'Onyx',
  sideroom: 'Side Room',
};

const SECTION_LABEL: Record<ThemedSection, string> = {
  lounge: 'Lounge',
  cinema: 'Cinema',
  library: 'Library',
  entrance: 'Entrance',
};

const INHERIT = 'inherit';

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
  const sectionBar = useLoungeUi((s) => s.showSectionBar);
  const sectionOptions = [
    { value: INHERIT, label: 'Default for this section' },
    ...THEME_IDS.map((id) => ({ value: id, label: THEMES[id].name })),
  ];
  const setSectionTheme = (section: ThemedSection, value: string) => {
    const next = { ...prefs.sectionThemes };
    if (value === INHERIT) delete next[section];
    else next[section] = value as ThemeId;
    setPrefs({ sectionThemes: next });
  };
  return (
    <>
      <SectionTitle>Appearance</SectionTitle>
      <p className="us-muted">
        Changes preview instantly. Save keeps them on your account, so they follow you to other
        devices.
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
      <Card
        id="us-section-themes"
        title="Per-section themes"
        description="Give a part of SIDE ROOM its own look. The Lounge follows the theme above unless you pick one here."
      >
        <div className="us-stack">
          {THEMED_SECTIONS.map((s) => (
            <div key={s} className="us-select-row">
              <label className="us-muted" id={`us-sec-${s}`}>
                {SECTION_LABEL[s]}
              </label>
              <Select
                aria-labelledby={`us-sec-${s}`}
                options={sectionOptions}
                value={prefs.sectionThemes[s] ?? INHERIT}
                onValueChange={(v) => setSectionTheme(s, v)}
              />
            </div>
          ))}
        </div>
      </Card>
      <Card id="us-server-theming" title="Server themes">
        <Switch
          label="Allow server theming"
          description="Let a server tint the interface with its accent colour while you are in it."
          checked={prefs.allowServerTheming}
          onChange={(e) => setPrefs({ allowServerTheming: e.target.checked })}
        />
        <Switch
          label="Show the section bar"
          description="The strip above the Lounge that switches between Lounge, Cinema and Library. Applies to this browser right away."
          checked={sectionBar}
          onChange={(e) => useLoungeUi.setState({ showSectionBar: e.target.checked })}
        />
      </Card>
      <Card id="us-messages" title="Message display">
        <div className="us-stack">
          <SegmentedControl
            label="Message display"
            value={prefs.messageDisplay}
            onValueChange={(v) => setPrefs({ messageDisplay: v as MessageDisplay })}
            options={[
              { value: 'cozy', label: 'Cozy' },
              { value: 'compact', label: 'Compact' },
            ]}
          />
          <Slider
            label="Space between message groups"
            min={MESSAGE_GAP_RANGE[0]}
            max={MESSAGE_GAP_RANGE[1]}
            step={4}
            value={prefs.messageGap}
            showValue
            formatValue={(v) => `${v}px`}
            onValueChange={(v) => setPrefs({ messageGap: v })}
          />
          <p
            className="us-compact-sample"
            data-compact={prefs.messageDisplay === 'compact' || undefined}
          >
            <strong>Alex</strong> <span className="us-muted">12:04</span>
            <br />
            <span className="us-sample-line">This is how a message looks.</span>
          </p>
        </div>
      </Card>
      <Card id="us-scale" title="Size and color">
        <div className="us-stack">
          <Slider
            label="Chat font scale"
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
    </>
  );
}
