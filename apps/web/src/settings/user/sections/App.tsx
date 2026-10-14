import {
  Button,
  ConfirmDialog,
  Notice,
  SegmentedControl,
  Select,
  Switch,
  useToast,
} from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { useLoungeUi } from '../../../lounge/store';
import { rest, useStore } from '../../../store';
import { errorText } from '../api';
import { Card, SectionTitle } from '../parts';
import type { TimeFormat } from '../prefs';
import type { PrefsFormProps } from './Appearance';

/**
 * Regional formats for dates and times. The interface text itself is English only until the translation
 * framework lands; the choice is stored now so it applies as soon as a language exists.
 */
export const LOCALES = [
  { value: 'auto', label: 'Same as my browser' },
  { value: 'en', label: 'English' },
  { value: 'en-GB', label: 'English (United Kingdom)' },
  { value: 'en-US', label: 'English (United States)' },
  { value: 'de', label: 'Deutsch' },
  { value: 'es', label: 'Español' },
  { value: 'fr', label: 'Français' },
  { value: 'it', label: 'Italiano' },
  { value: 'nl', label: 'Nederlands' },
  { value: 'pl', label: 'Polski' },
  { value: 'pt-BR', label: 'Português (Brasil)' },
  { value: 'sv', label: 'Svenska' },
  { value: 'tr', label: 'Türkçe' },
  { value: 'ru', label: 'Русский' },
  { value: 'ja', label: '日本語' },
  { value: 'ko', label: '한국어' },
  { value: 'zh-CN', label: '中文 (简体)' },
];

export function LanguageSection({ prefs, setPrefs }: PrefsFormProps) {
  const t = useT();
  const sample = new Date(2026, 0, 31, 15, 5);
  const locale = prefs.locale === 'auto' ? undefined : prefs.locale;
  const hour12 = prefs.timeFormat === 'auto' ? undefined : prefs.timeFormat === '12h';
  return (
    <>
      <SectionTitle>{t('settings.app.languageTitle')}</SectionTitle>
      <Card id="us-lang" title={t('settings.app.languageRegion')}>
        <div className="us-stack">
          <label className="us-muted" id="us-lang-label">
            {t('settings.app.regionLabel')}
          </label>
          <Select
            aria-labelledby="us-lang-label"
            options={LOCALES}
            value={prefs.locale}
            searchable
            onValueChange={(v) => setPrefs({ locale: v })}
          />
          <Notice variant="info" className="us-gap">
            {t('settings.app.languageNotice')}
          </Notice>
        </div>
      </Card>
      <Card id="us-time" title={t('settings.app.timeFormat')}>
        <div className="us-stack">
          <SegmentedControl
            label={t('settings.app.timeFormat')}
            value={prefs.timeFormat}
            onValueChange={(v) => setPrefs({ timeFormat: v as TimeFormat })}
            options={[
              { value: 'auto', label: t('settings.app.timeAuto') },
              { value: '12h', label: t('settings.app.time12') },
              { value: '24h', label: t('settings.app.time24') },
            ]}
          />
          <p className="us-muted" aria-live="polite">
            {t('settings.app.timeExample', {
              date: sample.toLocaleDateString(locale, { dateStyle: 'medium' }),
              time: sample.toLocaleTimeString(locale, {
                hour: 'numeric',
                minute: '2-digit',
                hour12,
              }),
            })}
          </p>
        </div>
      </Card>
    </>
  );
}

/** Layout state this browser remembers (and the account copy of it). */
const UI_DEFAULTS = {
  lastLocation: null,
  lastChannelByGuild: {},
  collapsedCategories: {},
  openFolders: {},
  railLayout: null,
  showSectionBar: true,
  memberListOpen: true,
  hideMutedChannels: {},
  rightPanelWidth: 240,
  recentSwitcher: [],
  dismissedBanners: {},
  welcomeDone: {},
};
/** Keys of the settings blob that hold layout (see `lounge/shell/userSettingsSync.ts`). */
const LAYOUT_KEYS = [
  'guild_positions',
  'guild_folders',
  'collapsed_categories',
  'last_channel_by_guild',
  'right_panel_width',
];

function ResetCard() {
  const t = useT();
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reset = async () => {
    setBusy(true);
    setError(null);
    try {
      // Drop the account copy first: a layout left there would come back on the next load.
      const saved = await rest.patchUserSettings(
        Object.fromEntries(LAYOUT_KEYS.map((k) => [k, null])),
      );
      useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', saved);
      useLoungeUi.setState(UI_DEFAULTS);
      setOpen(false);
      toast.show({ title: t('settings.app.layoutReset'), duration: 2500 });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      id="us-reset"
      title={t('settings.app.resetTitle')}
      description={t('settings.app.resetDesc')}
    >
      <Button variant="danger" onClick={() => setOpen(true)}>
        {t('settings.app.resetTitle')}
      </Button>
      {error && !open && (
        <Notice variant="danger" title={t('settings.app.resetError')}>
          {error}
        </Notice>
      )}
      <ConfirmDialog
        open={open}
        danger
        loading={busy}
        title={t('settings.app.resetConfirmTitle')}
        description={error ?? t('settings.app.resetConfirmDesc')}
        confirmLabel={t('settings.app.resetConfirm')}
        onConfirm={() => void reset()}
        onCancel={() => {
          setOpen(false);
          setError(null);
        }}
      />
    </Card>
  );
}

export function AdvancedSection({ prefs, setPrefs }: PrefsFormProps) {
  const t = useT();
  return (
    <>
      <SectionTitle>{t('settings.app.advancedTitle')}</SectionTitle>
      <Card id="us-dev" title={t('settings.app.devMode')}>
        <Switch
          label={t('settings.app.devMode')}
          description={t('settings.app.devModeDesc')}
          checked={prefs.developerMode}
          onChange={(e) => setPrefs({ developerMode: e.target.checked })}
        />
      </Card>
      <ResetCard />
    </>
  );
}
