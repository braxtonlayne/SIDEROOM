import { Button, Modal } from '@sideroom/ui';
import { lazy, Suspense, useEffect, useMemo, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT, type MessageKey } from '../../i18n';
import { isIosSafari, promptInstall, usePwaState } from '../../pwa/state';
import { THEME_CHOICES, usePrefsStore, type ThemeChoice } from '../../settings/user/prefs';
import { useOpenUserSettings } from '../../settings/user/nav';
import { useLoungeUi } from '../store';
import { saveFirstRunDone, useFirstRunUi } from './firstRun';
import { getGuide, getOnboarding } from './joinPipeline';

// The two-step sign-in wizard of Settings > My Account, loaded only when someone asks for it here.
const TotpWizard = lazy(() =>
  import('../../settings/user/sections/Account').then((m) => ({ default: m.TotpWizard })),
);

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
  /** `false` for an account that has not closed or finished the first run (RDY-ONB-001). */
  first_run_done?: boolean;
}

type StepId = 'intro' | 'look' | 'security' | 'install' | 'notify' | 'server';

interface HomeServer {
  id: string;
  name: string;
  guide: boolean;
  questions: boolean;
}

/**
 * The server the person landed in (the first on their list), when its managers wrote a guide or onboarding
 * questions (LNG-SSET community onboarding). Looked up only while the welcome is open; any failure means "none".
 */
function useHomeServer(active: boolean): HomeServer | null {
  const [server, setServer] = useState<HomeServer | null>(null);
  useEffect(() => {
    if (!active) return;
    let live = true;
    void (async () => {
      try {
        const guilds = await apiFetch<{ id: string; name?: string }[]>('/users/@me/guilds', {
          retries: 0,
        });
        const home = Array.isArray(guilds) ? guilds[0] : undefined;
        if (!home) return;
        const [guide, onboarding] = await Promise.allSettled([
          getGuide(home.id),
          getOnboarding(home.id),
        ]);
        const g = guide.status === 'fulfilled' ? guide.value : null;
        const o = onboarding.status === 'fulfilled' ? onboarding.value : null;
        const hasGuide =
          !!g?.enabled && (!!g.welcome_message || g.todos.length > 0 || g.resources.length > 0);
        const hasQuestions = !!o?.enabled && o.prompts.some((p) => p.in_onboarding);
        if (live && (hasGuide || hasQuestions))
          setServer({
            id: home.id,
            name: home.name ?? '',
            guide: hasGuide,
            questions: hasQuestions,
          });
      } catch {
        // Not signed in, offline or an old server: the step is simply left out.
      }
    })();
    return () => {
      live = false;
    };
  }, [active]);
  return server;
}

function safeSession(key: string): boolean {
  try {
    return sessionStorage.getItem(key) === '1';
  } catch {
    return false;
  }
}

/**
 * Platform first run (RDY-ONB-001, ENT-UI-022): a short, skippable welcome for an account whose `first_run_done`
 * is false. What the three sections are, the appearance, a two-step sign-in nudge with the setup in place, the
 * install prompt where the browser offers one, a word on notifications (the permission is asked later, never
 * here) and the home server's own guide when its managers wrote one.
 *
 * Nothing in it blocks anyone: Escape, the close button and "Skip for now" all close it, and closing stores
 * `first_run_done` on the account so it never returns on another device. `/help` brings it back.
 */
export function Welcome() {
  const t = useT();
  const prefs = usePrefsStore((s) => s.prefs);
  const save = usePrefsStore((s) => s.save);
  const openSettings = useOpenUserSettings();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const pwa = usePwaState();
  const forced = useFirstRunUi((s) => s.forced);
  const hide = useFirstRunUi((s) => s.hide);
  const [me, setMe] = useState<Me | null>(null);
  const [needsMfa, setNeedsMfa] = useState(false);
  const [closed, setClosed] = useState(false);
  const [step, setStep] = useState(0);
  const [wizard, setWizard] = useState(false);
  const [mfaOn, setMfaOn] = useState(false);

  // The end-to-end dev server (e2e/playwright.config.ts) sets VITE_SR_E2E: every e2e account is brand new, and
  // this modal would cover the Lounge in every flow those tests drive. The first-run spec opts in per page.
  const e2eMuted = import.meta.env.VITE_SR_E2E === '1' && !safeSession('sr-e2e-first-run');

  useEffect(() => {
    let live = true;
    apiFetch<Me>('/users/@me', { retries: 0 }).then(
      (m) => {
        if (!live) return;
        setMe(m);
        setNeedsMfa(m.mfa_enabled === false);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, []);

  const wanted = (me?.first_run_done === false && !closed && !e2eMuted) || forced;
  const home = useHomeServer(wanted);
  const ios = isIosSafari();
  const steps = useMemo(() => {
    const out: StepId[] = ['intro', 'look'];
    if (needsMfa) out.push('security');
    if (!pwa.installed && (pwa.canInstall || ios)) out.push('install');
    out.push('notify');
    if (home) out.push('server');
    return out;
  }, [needsMfa, pwa.installed, pwa.canInstall, ios, home]);

  const index = Math.min(step, steps.length - 1);
  const current = steps[index] ?? 'intro';
  const last = index >= steps.length - 1;

  const finish = () => {
    setClosed(true);
    hide();
    setStep(0);
    // A failed save only means the welcome may show once more on the next page load.
    void saveFirstRunDone(true).catch(() => undefined);
  };
  const go = (section: 'profile' | 'notifications') => {
    finish();
    openSettings(section);
  };

  return (
    <>
      <Modal
        open={wanted && !wizard}
        onClose={finish}
        title={t('lounge.welcome.title')}
        size="sm"
        footer={
          <div className="lng-firstrun-footer">
            <span className="lng-muted" aria-live="polite">
              {t('lounge.firstRun.progress', { n: index + 1, total: steps.length })}
            </span>
            <span className="lng-firstrun-actions">
              <Button variant="ghost" onClick={finish}>
                {last ? t('lounge.firstRun.close') : t('lounge.firstRun.skip')}
              </Button>
              {index > 0 && (
                <Button variant="secondary" onClick={() => setStep(index - 1)}>
                  {t('lounge.firstRun.back')}
                </Button>
              )}
              {last ? (
                <Button onClick={finish}>{t('lounge.firstRun.finish')}</Button>
              ) : (
                <Button onClick={() => setStep(index + 1)}>{t('lounge.firstRun.next')}</Button>
              )}
            </span>
          </div>
        }
      >
        <div className="lng-welcome-flow">
          {current === 'intro' && (
            <section aria-labelledby="lng-firstrun-intro">
              <h3 id="lng-firstrun-intro">{t('lounge.firstRun.intro.title')}</h3>
              <p>{t('lounge.firstRun.intro.body')}</p>
              <ul className="lng-firstrun-list">
                <li>{t('lounge.firstRun.intro.lounge')}</li>
                <li>{t('lounge.firstRun.intro.library')}</li>
                <li>{t('lounge.firstRun.intro.cinema')}</li>
              </ul>
            </section>
          )}
          {current === 'look' && (
            <>
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
            </>
          )}
          {current === 'security' && (
            <section aria-labelledby="lng-welcome-2fa">
              <h3 id="lng-welcome-2fa">{t('lounge.firstRun.security.title')}</h3>
              {mfaOn ? (
                <p role="status">{t('lounge.firstRun.security.done')}</p>
              ) : (
                <>
                  <p className="lng-muted">{t('lounge.firstRun.security.body')}</p>
                  <Button size="sm" variant="secondary" onClick={() => setWizard(true)}>
                    {t('lounge.firstRun.security.start')}
                  </Button>
                </>
              )}
            </section>
          )}
          {current === 'install' && (
            <section aria-labelledby="lng-firstrun-install">
              <h3 id="lng-firstrun-install">{t('lounge.firstRun.install.title')}</h3>
              <p className="lng-muted">{t('lounge.firstRun.install.body')}</p>
              {pwa.canInstall ? (
                <Button size="sm" variant="secondary" onClick={() => void promptInstall()}>
                  {t('lounge.firstRun.install.action')}
                </Button>
              ) : (
                <ol>
                  <li>{t('pwa.install.ios.step1')}</li>
                  <li>{t('pwa.install.ios.step2')}</li>
                  <li>{t('pwa.install.ios.step3')}</li>
                </ol>
              )}
            </section>
          )}
          {current === 'notify' && (
            <section aria-labelledby="lng-firstrun-notify">
              <h3 id="lng-firstrun-notify">{t('lounge.firstRun.notify.title')}</h3>
              <p className="lng-muted">{t('lounge.firstRun.notify.body')}</p>
              <Button size="sm" variant="secondary" onClick={() => go('notifications')}>
                {t('lounge.firstRun.notify.open')}
              </Button>
              <p className="lng-muted">{t('lounge.firstRun.reopenHint')}</p>
            </section>
          )}
          {current === 'server' && home && (
            <section aria-labelledby="lng-firstrun-server">
              <h3 id="lng-firstrun-server">
                {t('lounge.firstRun.server.title', { name: home.name })}
              </h3>
              <p className="lng-muted">{t('lounge.firstRun.server.body')}</p>
              <div className="lng-welcome-row">
                {home.guide && (
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => {
                      finish();
                      openDialog({ kind: 'serverGuide', guildId: home.id });
                    }}
                  >
                    {t('lounge.firstRun.server.guide')}
                  </Button>
                )}
                {home.questions && (
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => {
                      finish();
                      openDialog({ kind: 'onboarding', guildId: home.id });
                    }}
                  >
                    {t('lounge.firstRun.server.questions')}
                  </Button>
                )}
              </div>
            </section>
          )}
        </div>
      </Modal>
      {/* Mounted closed and opened by the button: the wizard asks the server for a new secret every time it
          opens, and mounting it already open runs that request twice under StrictMode, so the key on screen
          could be the one the server no longer holds. */}
      {wanted && needsMfa && (
        <Suspense fallback={null}>
          <TotpWizard
            open={wizard}
            onClose={() => setWizard(false)}
            onEnabled={() => setMfaOn(true)}
          />
        </Suspense>
      )}
    </>
  );
}
