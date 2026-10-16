import { Button, Modal } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT, type MessageKey } from '../../i18n';
import { THEME_CHOICES, usePrefsStore, type ThemeChoice } from '../../settings/user/prefs';
import { useOpenUserSettings } from '../../settings/user/nav';

/** The platform snowflake epoch (sr-core `SIDEROOM_EPOCH_MS`). */
const EPOCH_MS = 1_767_225_600_000n;
const NEW_ACCOUNT_DAYS = 14;
const SHOW_DELAY_MS = 1500;

/** Whether an account id was minted in the last `days` days; false for anything that is not a snowflake. */
export function isNewAccount(id: string | undefined, now = Date.now(), days = NEW_ACCOUNT_DAYS) {
  if (!id || !/^\d+$/.test(id)) return false;
  const created = Number((BigInt(id) >> 22n) + EPOCH_MS);
  return now - created >= 0 && now - created <= days * 86_400_000;
}

const THEME_LABEL: Record<ThemeChoice, MessageKey> = {
  system: 'settings.appearance.themeSystem',
  'lounge-dark': 'settings.appearance.themeDark',
  'lounge-light': 'settings.appearance.themeLight',
  'lounge-ash': 'settings.appearance.themeAsh',
  'lounge-onyx': 'settings.appearance.themeOnyx',
  sideroom: 'settings.appearance.themeSideRoom',
};

interface Me {
  id: string;
  mfa_enabled?: boolean;
}

/**
 * First-run welcome (ENT-UI-022): shown once to a new account. A theme choice, a nudge to add a picture, a nudge
 * to turn on two-step sign-in when it is off, then it is marked done in the account's synced preferences
 * (`welcomeDone`), so it never returns on another device. Suggested servers wait for a discovery API.
 */
export function Welcome() {
  const t = useT();
  const prefs = usePrefsStore((s) => s.prefs);
  const save = usePrefsStore((s) => s.save);
  const openSettings = useOpenUserSettings();
  const [me, setMe] = useState<Me | null>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    if (prefs.welcomeDone) return;
    let live = true;
    apiFetch<Me>('/users/@me', { retries: 0 }).then(
      (m) => live && setMe(m),
      () => undefined,
    );
    // Wait a moment so the account copy of the preferences can arrive first.
    const timer = setTimeout(() => live && setReady(true), SHOW_DELAY_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [prefs.welcomeDone]);

  const open = ready && !prefs.welcomeDone && !!me && isNewAccount(me.id);
  const done = () => save({ ...usePrefsStore.getState().prefs, welcomeDone: true });
  const go = (section: 'profile' | 'account') => {
    done();
    openSettings(section);
  };
  return (
    <Modal
      open={open}
      onClose={done}
      title={t('lounge.welcome.title')}
      size="sm"
      footer={<Button onClick={done}>{t('lounge.welcome.done')}</Button>}
    >
      <div className="lng-welcome">
        <p>{t('lounge.welcome.intro')}</p>
        <section aria-labelledby="lng-welcome-theme">
          <h3 id="lng-welcome-theme">{t('lounge.welcome.themeHeading')}</h3>
          <div className="lng-welcome-row" role="group" aria-labelledby="lng-welcome-theme">
            {(['system', ...THEME_CHOICES] as ThemeChoice[]).map((c) => (
              <Button
                key={c}
                size="sm"
                variant={prefs.theme === c ? 'primary' : 'secondary'}
                aria-pressed={prefs.theme === c}
                onClick={() => save({ ...usePrefsStore.getState().prefs, theme: c })}
              >
                {t(THEME_LABEL[c])}
              </Button>
            ))}
          </div>
        </section>
        <section aria-labelledby="lng-welcome-avatar">
          <h3 id="lng-welcome-avatar">{t('lounge.welcome.avatarHeading')}</h3>
          <p className="lng-muted">{t('lounge.welcome.avatarText')}</p>
          <Button size="sm" variant="secondary" onClick={() => go('profile')}>
            {t('lounge.welcome.avatarButton')}
          </Button>
        </section>
        {me && !me.mfa_enabled && (
          <section aria-labelledby="lng-welcome-2fa">
            <h3 id="lng-welcome-2fa">{t('lounge.welcome.mfaHeading')}</h3>
            <p className="lng-muted">{t('lounge.welcome.mfaText')}</p>
            <Button size="sm" variant="secondary" onClick={() => go('account')}>
              {t('lounge.welcome.mfaButton')}
            </Button>
          </section>
        )}
      </div>
    </Modal>
  );
}
