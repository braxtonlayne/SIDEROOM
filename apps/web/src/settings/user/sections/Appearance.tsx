import { SegmentedControl, Select, Slider, Switch, THEMES, THEME_IDS } from '@sideroom/ui';
import type { ThemeId } from '@sideroom/ui';
import { useT, type MessageKey } from '../../../i18n';
import { useLoungeUi } from '../../../lounge/store';
import { ThemeStore } from '../../../packages/ThemeStore';
import { Card, SectionTitle } from '../parts';
import {
  FONT_SCALE_RANGE,
  MESSAGE_GAP_RANGE,
  THEME_CHOICES,
  THEMED_SECTIONS,
  ZOOM_RANGE,
  type MessageDisplay,
  type NavMode,
  type ThemeChoice,
  type ThemedSection,
  type UserPrefs,
} from '../prefs';

const THEME_LABEL: Record<ThemeChoice, MessageKey> = {
  system: 'settings.appearance.themeSystem',
  'lounge-dark': 'settings.appearance.themeDark',
  'lounge-light': 'settings.appearance.themeLight',
  'lounge-ash': 'settings.appearance.themeAsh',
  'lounge-onyx': 'settings.appearance.themeOnyx',
  sideroom: 'settings.appearance.themeSideRoom',
};

const SECTION_LABEL: Record<ThemedSection, MessageKey> = {
  lounge: 'settings.appearance.sectionLounge',
  cinema: 'settings.appearance.sectionCinema',
  library: 'settings.appearance.sectionLibrary',
  entrance: 'settings.appearance.sectionEntrance',
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
  const t = useT();
  const sectionBar = useLoungeUi((s) => s.showSectionBar);
  const sectionOptions = [
    { value: INHERIT, label: t('settings.appearance.sectionDefault') },
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
      <SectionTitle>{t('settings.appearance.title')}</SectionTitle>
      <p className="us-muted">{t('settings.appearance.intro')}</p>
      <Card id="us-theme" title={t('settings.appearance.theme')}>
        <div className="us-themes" role="group" aria-label={t('settings.appearance.theme')}>
          {(['system', ...THEME_CHOICES] as ThemeChoice[]).map((c) => (
            <button
              key={c}
              type="button"
              className="us-theme"
              aria-pressed={prefs.theme === c}
              onClick={() => setPrefs({ theme: c })}
            >
              <Swatch choice={c} />
              <span className="us-theme-name">{t(THEME_LABEL[c])}</span>
            </button>
          ))}
        </div>
      </Card>
      <Card
        id="us-section-themes"
        title={t('settings.appearance.perSection')}
        description={t('settings.appearance.perSectionDesc')}
      >
        <div className="us-stack">
          {THEMED_SECTIONS.map((s) => (
            <div key={s} className="us-select-row">
              <label className="us-muted" id={`us-sec-${s}`}>
                {t(SECTION_LABEL[s])}
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
      <Card
        id="us-theme-store"
        title={t('settings.themeStore.title')}
        description={t('settings.themeStore.description')}
      >
        <ThemeStore />
      </Card>
      <Card id="us-server-theming" title={t('settings.appearance.serverThemes')}>
        <Switch
          label={t('settings.appearance.allowServer')}
          description={t('settings.appearance.allowServerDesc')}
          checked={prefs.allowServerTheming}
          onChange={(e) => setPrefs({ allowServerTheming: e.target.checked })}
        />
        <Switch
          label={t('settings.appearance.sectionBar')}
          description={t('settings.appearance.sectionBarDesc')}
          checked={sectionBar}
          onChange={(e) => useLoungeUi.setState({ showSectionBar: e.target.checked })}
        />
      </Card>
      <Card
        id="us-nav-sections"
        title={t('settings.storageNav.title')}
        description={t('settings.storageNav.description')}
      >
        <div className="us-stack">
          {(['navCinema', 'navLibrary'] as const).map((key) => (
            <div key={key}>
              <SegmentedControl
                label={
                  key === 'navCinema'
                    ? t('settings.storageNav.cinema')
                    : t('settings.storageNav.library')
                }
                value={prefs[key]}
                onValueChange={(v) => setPrefs({ [key]: v as NavMode })}
                options={[
                  { value: 'show', label: t('settings.storageNav.show') },
                  { value: 'hide', label: t('settings.storageNav.hide') },
                  { value: 'auto', label: t('settings.storageNav.auto') },
                ]}
              />
            </div>
          ))}
        </div>
      </Card>
      <Card id="us-messages" title={t('settings.appearance.messageDisplay')}>
        <div className="us-stack">
          <SegmentedControl
            label={t('settings.appearance.messageDisplay')}
            value={prefs.messageDisplay}
            onValueChange={(v) => setPrefs({ messageDisplay: v as MessageDisplay })}
            options={[
              { value: 'cozy', label: t('settings.appearance.cozy') },
              { value: 'compact', label: t('settings.appearance.compact') },
            ]}
          />
          <Switch
            label={t('settings.appearance.compactAvatars')}
            description={t('settings.appearance.compactAvatarsDesc')}
            checked={prefs.compactAvatars}
            disabled={prefs.messageDisplay !== 'compact'}
            onChange={(e) => setPrefs({ compactAvatars: e.target.checked })}
          />
          <Slider
            label={t('settings.appearance.gap')}
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
            <strong>{t('settings.appearance.sampleName')}</strong>{' '}
            <span className="us-muted">12:04</span>
            <br />
            <span className="us-sample-line">{t('settings.appearance.sample')}</span>
          </p>
        </div>
      </Card>
      <Card id="us-scale" title={t('settings.appearance.sizeColor')}>
        <div className="us-stack">
          <Slider
            label={t('settings.appearance.fontScale')}
            min={FONT_SCALE_RANGE[0]}
            max={FONT_SCALE_RANGE[1]}
            step={5}
            value={prefs.fontScale}
            showValue
            formatValue={(v) => `${v}%`}
            onValueChange={(v) => setPrefs({ fontScale: v })}
          />
          <Slider
            label={t('settings.appearance.zoom')}
            min={ZOOM_RANGE[0]}
            max={ZOOM_RANGE[1]}
            step={10}
            value={prefs.zoom}
            showValue
            formatValue={(v) => `${v}%`}
            onValueChange={(v) => setPrefs({ zoom: v })}
          />
          <Slider
            label={t('settings.appearance.saturation')}
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
