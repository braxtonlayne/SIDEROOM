import {
  Button,
  ConfirmDialog,
  Notice,
  SettingsLayout,
  ThemeProvider,
  ToastProvider,
  useToast,
} from '@sideroom/ui';
import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import { ApiError } from '../../api/client';
import { errorText, getMe, logout, useResource } from './api';
import { USER_SETTINGS_SECTIONS, type UserSettingsSectionId } from './nav';
import { applyPrefs, prefsEqual, usePrefsStore, useSavedPrefs, type UserPrefs } from './prefs';
import { useMediaQuery } from '@sideroom/ui';
import { AccountSection } from './sections/Account';
import { AdvancedSection, LanguageSection } from './sections/App';
import { KeybindsSection } from './sections/Keybinds';
import { VoiceVideoSection } from './sections/VoiceVideo';
import { AppearanceSection } from './sections/Appearance';
import { NotificationsSection } from './sections/Notifications';
import { PrivacySection } from './sections/Privacy';
import { ProfileSection, useProfileForm } from './sections/Profile';

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

  // A 401 means the session ended (expired, revoked elsewhere): go to sign-in.
  useEffect(() => {
    if (me.error instanceof ApiError && me.error.status === 401) onSignedOut();
  }, [me.error, onSignedOut]);

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
        sidebarFooter={
          <Button variant="danger" className="us-logout" onClick={() => setConfirmOut(true)}>
            Log Out
          </Button>
        }
      >
        {section === 'account' && <AccountSection me={me} onSignedOut={onSignedOut} />}
        {section === 'profile' && <ProfileSection form={profile} />}
        {section === 'privacy' && <PrivacySection />}
        {section === 'appearance' && <AppearanceSection {...prefsProps} />}
        {section === 'notifications' && <NotificationsSection {...prefsProps} />}
        {section === 'voice' && <VoiceVideoSection />}
        {section === 'keybinds' && <KeybindsSection />}
        {section === 'language' && <LanguageSection />}
        {section === 'advanced' && <AdvancedSection {...prefsProps} />}
      </SettingsLayout>
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
  const saved = useSavedPrefs();
  const [draft, setDraft] = useState<UserPrefs>(saved);
  const dark = useMediaQuery('(prefers-color-scheme: dark)');
  const applied = applyPrefs(draft, dark);
  return (
    <ThemeProvider
      section="lounge"
      userGlobal={applied.userGlobal}
      appearance={applied.appearance}
      style={applied.style}
      data-message-display={applied.messageDisplay}
    >
      <ToastProvider>
        <Body {...props} draft={draft} setDraft={setDraft} />
      </ToastProvider>
    </ThemeProvider>
  );
}
