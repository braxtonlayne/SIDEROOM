import {
  Button,
  ConfirmDialog,
  Modal,
  Notice,
  SettingsLayout,
  ThemeProvider,
  ToastProvider,
  useMediaQuery,
  useToast,
} from '@sideroom/ui';
import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import pkg from '../../../package.json';
import { ApiError } from '../../api/client';
import { errorText, getMe, logout, useResource } from './api';
import { USER_SETTINGS_SECTIONS, type UserSettingsSectionId } from './nav';
import { applyPrefs, prefsEqual, usePrefsStore, useSavedPrefs, type UserPrefs } from './prefs';
import { usePrefsSync } from './prefsSync';
import type { SettingsEntry } from './searchIndex';
import { AccessibilitySection } from './sections/Accessibility';
import { AccountSection } from './sections/Account';
import { ActivitySection } from './sections/Activity';
import { AdvancedSection, LanguageSection } from './sections/App';
import { VoiceVideoSection } from './sections/VoiceVideo';
import { AppearanceSection } from './sections/Appearance';
import { AuthorizedAppsSection } from './sections/AuthorizedApps';
import { ConnectionsSection } from './sections/Connections';
import { DevicesSection } from './sections/Devices';
import { KeybindsSection } from './sections/Keybinds';
import { NotificationsSection } from './sections/Notifications';
import { PrivacySection } from './sections/Privacy';
import { ProfileSection, useProfileForm } from './sections/Profile';
import { TextImagesSection } from './sections/TextImages';
import { SettingsSearch } from './SettingsSearch';
import { WHATS_NEW } from './whatsNew';

export interface UserSettingsProps {
  section: UserSettingsSectionId;
  onSelect: (section: UserSettingsSectionId) => void;
  onClose: () => void;
  /** The session is gone (log out, log out everywhere, or a 401): leave for the sign-in page. */
  onSignedOut: () => void;
  /** Render in flow instead of covering the viewport (tests). */
  inline?: boolean;
}

function Body({
  section,
  onSelect,
  onClose,
  onSignedOut,
  inline,
  draft,
  setDraft,
}: UserSettingsProps & { draft: UserPrefs; setDraft: Dispatch<SetStateAction<UserPrefs>> }) {
  const toast = useToast();
  const saved = useSavedPrefs();
  const savePrefs = usePrefsStore((s) => s.save);
  const profileSeen = useRef(false);
  if (section === 'profile') profileSeen.current = true;
  const profile = useProfileForm(profileSeen.current);
  const me = useResource(getMe);
  const [saving, setSaving] = useState(false);
  const [confirmOut, setConfirmOut] = useState(false);
  const [outBusy, setOutBusy] = useState(false);
  const [outError, setOutError] = useState<string | null>(null);
  const [news, setNews] = useState(false);
  const [scrollTo, setScrollTo] = useState<string | null>(null);

  // A 401 means the session ended (expired, revoked elsewhere): go to sign-in.
  useEffect(() => {
    if (me.error instanceof ApiError && me.error.status === 401) onSignedOut();
  }, [me.error, onSignedOut]);

  // Search picked a card: once its section is on screen, bring the card into view.
  useEffect(() => {
    if (!scrollTo) return;
    const t = setTimeout(() => {
      document.getElementById(scrollTo)?.scrollIntoView?.({ block: 'start' });
      setScrollTo(null);
    }, 0);
    return () => clearTimeout(t);
  }, [scrollTo, section]);
  const onSearchPick = (e: SettingsEntry) => {
    onSelect(e.section);
    setScrollTo(e.card ?? null);
  };

  const prefsDirty = !prefsEqual(saved, draft);
  const dirty = prefsDirty || profile.dirty;

  const onSave = async () => {
    setSaving(true);
    try {
      if (prefsDirty) savePrefs(draft);
      if (profile.dirty) await profile.save();
      toast.show({ title: 'Settings saved', duration: 2000 });
    } catch (e) {
      toast.show({
        title: "Couldn't save everything",
        description: e instanceof Error ? e.message : undefined,
        variant: 'danger',
      });
    } finally {
      setSaving(false);
    }
  };
  const onReset = () => {
    setDraft(saved);
    profile.reset();
  };
  const setPrefs = (patch: Partial<UserPrefs>) => setDraft((d) => ({ ...d, ...patch }));

  const doLogout = async () => {
    setOutBusy(true);
    setOutError(null);
    try {
      await logout();
      onSignedOut();
    } catch (e) {
      setOutError(errorText(e));
      setOutBusy(false);
    }
  };

  const prefsProps = { prefs: draft, setPrefs };
  return (
    <>
      <SettingsLayout
        inline={inline}
        title="User Settings"
        sections={USER_SETTINGS_SECTIONS.map((s) => ({ id: s.id, label: s.label, group: s.group }))}
        activeId={section}
        onSelect={(id) => onSelect(id as UserSettingsSectionId)}
        onClose={onClose}
        dirty={dirty}
        saving={saving}
        onSave={() => void onSave()}
        onReset={onReset}
        sidebarHeader={<SettingsSearch onPick={onSearchPick} />}
        sidebarFooter={
          <>
            <Button variant="secondary" onClick={() => setNews(true)}>
              What&apos;s New
            </Button>
            <Button variant="danger" className="us-logout" onClick={() => setConfirmOut(true)}>
              Log Out
            </Button>
            <p className="us-version">SIDE ROOM {pkg.version}</p>
          </>
        }
      >
        {section === 'account' && <AccountSection me={me} onSignedOut={onSignedOut} />}
        {section === 'profile' && <ProfileSection form={profile} />}
        {section === 'privacy' && <PrivacySection />}
        {section === 'activity' && <ActivitySection {...prefsProps} />}
        {section === 'apps' && <AuthorizedAppsSection />}
        {section === 'devices' && <DevicesSection onSignedOut={onSignedOut} />}
        {section === 'connections' && <ConnectionsSection />}
        {section === 'appearance' && <AppearanceSection {...prefsProps} />}
        {section === 'accessibility' && <AccessibilitySection {...prefsProps} />}
        {section === 'text' && <TextImagesSection {...prefsProps} />}
        {section === 'notifications' && <NotificationsSection {...prefsProps} />}
        {section === 'voice' && <VoiceVideoSection />}
        {section === 'keybinds' && <KeybindsSection {...prefsProps} />}
        {section === 'language' && <LanguageSection {...prefsProps} />}
        {section === 'advanced' && <AdvancedSection {...prefsProps} />}
      </SettingsLayout>
      <Modal open={news} onClose={() => setNews(false)} title="What's New" size="md">
        {WHATS_NEW.map((n) => (
          <section key={n.title}>
            <h3>{n.title}</h3>
            <ul>
              {n.items.map((i) => (
                <li key={i}>{i}</li>
              ))}
            </ul>
          </section>
        ))}
      </Modal>
      <ConfirmDialog
        open={confirmOut}
        danger
        loading={outBusy}
        title="Log out?"
        description={
          outError ? undefined : 'You will need to sign in again to use SIDE ROOM on this device.'
        }
        confirmLabel="Log Out"
        onConfirm={() => void doLogout()}
        onCancel={() => {
          setConfirmOut(false);
          setOutError(null);
        }}
      />
      {confirmOut && outError && (
        <Notice variant="danger" title="Couldn't log out" role="alert">
          {outError}
        </Notice>
      )}
    </>
  );
}

/** Full-screen User Settings (LNG-USET-001). The theme preview follows the unsaved draft. */
export function UserSettings(props: UserSettingsProps) {
  usePrefsSync();
  const saved = useSavedPrefs();
  const [draft, setDraft] = useState<UserPrefs>(saved);
  // The account copy can arrive after first paint: follow it until the person starts editing.
  const touched = useRef(false);
  const setDraftTracked: Dispatch<SetStateAction<UserPrefs>> = (v) => {
    touched.current = true;
    setDraft(v);
  };
  useEffect(() => {
    if (!touched.current) setDraft(saved);
  }, [saved]);
  const dark = useMediaQuery('(prefers-color-scheme: dark)');
  const applied = applyPrefs(draft, dark);
  return (
    <ThemeProvider
      section="lounge"
      userGlobal={applied.userGlobal}
      perSection={applied.perSection}
      allowServerTheming={applied.allowServerTheming}
      appearance={applied.appearance}
      style={applied.style}
      {...applied.data}
    >
      <ToastProvider>
        <Body {...props} draft={draft} setDraft={setDraftTracked} />
      </ToastProvider>
    </ThemeProvider>
  );
}
