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
  const sample = new Date(2026, 0, 31, 15, 5);
  const locale = prefs.locale === 'auto' ? undefined : prefs.locale;
  const hour12 = prefs.timeFormat === 'auto' ? undefined : prefs.timeFormat === '12h';
  return (
    <>
      <SectionTitle>Language</SectionTitle>
      <Card id="us-lang" title="Language and region">
        <div className="us-stack">
          <label className="us-muted" id="us-lang-label">
            Region and date format
          </label>
          <Select
            aria-labelledby="us-lang-label"
            options={LOCALES}
            value={prefs.locale}
            searchable
            onValueChange={(v) => setPrefs({ locale: v })}
          />
          <Notice variant="info" className="us-gap">
            Dates and times follow this choice. The interface text is English only for now; more
            languages are planned.
          </Notice>
        </div>
      </Card>
      <Card id="us-time" title="Time format">
        <div className="us-stack">
          <SegmentedControl
            label="Time format"
            value={prefs.timeFormat}
            onValueChange={(v) => setPrefs({ timeFormat: v as TimeFormat })}
            options={[
              { value: 'auto', label: 'Automatic' },
              { value: '12h', label: '12-hour' },
              { value: '24h', label: '24-hour' },
            ]}
          />
          <p className="us-muted" aria-live="polite">
            Example: {sample.toLocaleDateString(locale, { dateStyle: 'medium' })},{' '}
            {sample.toLocaleTimeString(locale, { hour: 'numeric', minute: '2-digit', hour12 })}
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
      toast.show({ title: 'Layout reset', duration: 2500 });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      id="us-reset"
      title="Reset UI state"
      description="Forget the server order and folders, collapsed categories, panel sizes, dismissed banners and the last place you were. Your messages and settings stay."
    >
      <Button variant="danger" onClick={() => setOpen(true)}>
        Reset UI state
      </Button>
      {error && !open && (
        <Notice variant="danger" title="Couldn't reset">
          {error}
        </Notice>
      )}
      <ConfirmDialog
        open={open}
        danger
        loading={busy}
        title="Reset UI state?"
        description={error ?? 'The layout goes back to how it was on your first visit.'}
        confirmLabel="Reset"
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
  return (
    <>
      <SectionTitle>Advanced</SectionTitle>
      <Card id="us-dev" title="Developer mode">
        <Switch
          label="Developer mode"
          description="Saved for developer tools as they arrive. Copy ID items are already in every menu."
          checked={prefs.developerMode}
          onChange={(e) => setPrefs({ developerMode: e.target.checked })}
        />
      </Card>
      <ResetCard />
    </>
  );
}
