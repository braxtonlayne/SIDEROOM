import { DisplayHeading, Eyebrow, GoldButton } from '@sideroom/ui';
import { EntranceMarkdown, useEntranceCopy, useEntranceHero } from './entranceCopy';
import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { checkInvite } from '../../api/entrance';
import { Door } from '../../components/Door';
import { EntranceShell } from '../../components/EntranceShell';
import { DoorIcon, KeyIcon, LockIcon } from '../../components/icons';
import { useT } from '../../i18n/react';
import type { MessageKey } from '../../i18n/messages/en';
import { useCompliancePublic } from '../../lib/compliancePublic';
import { formatInviteCode, holdInviteCode, isCompleteInviteCode } from '../../lib/inviteCode';
import { nav } from '../../lib/enterApp';
import { useRedirectIfLoggedIn } from '../../lib/useRedirectIfLoggedIn';

type CheckState =
  'idle' | 'checking' | 'valid' | 'invalid' | 'closed' | 'locked' | 'limited' | 'error';

const MESSAGES: Partial<Record<CheckState, { tone: 'good' | 'bad'; key: MessageKey }>> = {
  valid: { tone: 'good', key: 'entrance.landing.valid' },
  invalid: { tone: 'bad', key: 'entrance.landing.invalid' },
  closed: { tone: 'bad', key: 'entrance.landing.closed' },
  locked: { tone: 'bad', key: 'entrance.doors.closed' },
  limited: { tone: 'bad', key: 'entrance.landing.limited' },
  error: { tone: 'bad', key: 'entrance.landing.error' },
};

export function Landing({ initialCode = '' }: { initialCode?: string }) {
  const copy = useEntranceCopy();
  const note = copy['landing-note'];
  const hero = useEntranceHero();
  useRedirectIfLoggedIn();
  const t = useT();
  const compliance = useCompliancePublic();
  const [code, setCode] = useState(() => formatInviteCode(initialCode));
  const [state, setState] = useState<CheckState>('idle');
  const [shakes, setShakes] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const complete = isCompleteInviteCode(code);

  const submit = useCallback(async (value: string) => {
    if (!isCompleteInviteCode(value)) return;
    setState('checking');
    try {
      const res = await checkInvite(value);
      if (res.valid) {
        holdInviteCode(value);
        setState('valid');
        timer.current = setTimeout(() => nav.assign('/register'), 600);
      } else if (res.reason === 'closed') {
        setState('closed');
      } else if (res.reason === 'locked') {
        setState('locked');
      } else {
        setState('invalid');
        setShakes((n) => n + 1);
      }
    } catch (e) {
      setState(e instanceof ApiError && e.status === 429 ? 'limited' : 'error');
    }
  }, []);

  // /join/:code auto-checks once on arrival.
  const auto = useRef(false);
  useEffect(() => {
    if (initialCode && !auto.current) {
      auto.current = true;
      void submit(formatInviteCode(initialCode));
    }
  }, [initialCode, submit]);
  useEffect(() => () => clearTimeout(timer.current), []);
  // A refused code remounts the shaking input (and the button was disabled while checking), so focus would fall
  // to the page body: put it back in the field, where the message is read and the code is corrected.
  useEffect(() => {
    if (state === 'invalid' || state === 'closed' || state === 'locked') input.current?.focus();
  }, [state, shakes]);

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    void submit(code);
  };
  const msg = MESSAGES[state];
  const busy = state === 'checking' || state === 'valid';

  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main>
        <section className="ent-hero" aria-labelledby="hero-title">
          <div className="ent-hero-left">
            <Eyebrow>{t('entrance.landing.eyebrow')}</Eyebrow>
            <DisplayHeading
              id="hero-title"
              size="xl"
              accent={t('entrance.landing.headingAccent')}
              className="ent-big"
            >
              {t('entrance.landing.heading')}
            </DisplayHeading>
            <p className="ent-lede">
              {t('entrance.landing.lede1')}
              <br />
              {t('entrance.landing.lede2')}
            </p>
            {compliance.doors_closed && (
              <p className="ent-invite-msg" data-tone="bad" role="status" id="doors-closed">
                {t('entrance.doors.closed')}
              </p>
            )}
            <form onSubmit={onSubmit} noValidate>
              <label className="ent-invite-label" htmlFor="invite-code">
                {t('entrance.landing.inviteLabel')}
              </label>
              <div
                className={`ent-invite${shakes ? ' ent-shake' : ''}`}
                data-state={state}
                key={`shake-${shakes}`}
              >
                <KeyIcon aria-hidden="true" />
                <input
                  id="invite-code"
                  ref={input}
                  value={code}
                  onChange={(e) => {
                    setCode(formatInviteCode(e.target.value));
                    if (state !== 'checking') setState('idle');
                  }}
                  placeholder={t('entrance.landing.invitePlaceholder')}
                  autoComplete="off"
                  autoCapitalize="characters"
                  spellCheck={false}
                  aria-invalid={
                    state === 'invalid' || state === 'closed' || state === 'locked'
                      ? true
                      : undefined
                  }
                  aria-describedby="invite-msg"
                />
                <GoldButton
                  type="submit"
                  arrow
                  disabled={!complete || busy}
                  aria-busy={state === 'checking'}
                >
                  {state === 'checking'
                    ? t('entrance.landing.checking')
                    : t('entrance.landing.check')}
                </GoldButton>
              </div>
              <p
                id="invite-msg"
                className="ent-invite-msg"
                data-tone={msg?.tone}
                role="status"
                aria-live="polite"
              >
                {msg ? t(msg.key) : null}
              </p>
            </form>
            <div className="ent-helper">
              <span>
                <LockIcon width={14} height={14} /> {t('entrance.landing.required')}
              </span>
              <a href="/no-invitation">{t('entrance.landing.noInvitation')}</a>
            </div>
            <p className="ent-footnote">
              {t('entrance.landing.ageNotice', { age: compliance.minimum_age })}
            </p>
            {note && (
              <aside className="ent-landing-note">
                <EntranceMarkdown source={note} />
              </aside>
            )}
          </div>
          <div className="ent-hero-right">
            <Door
              className="ent-door-bg"
              {...(hero ? { image: { src: hero, alt: copy['hero-alt'] ?? '' } } : {})}
            />
            <div className="ent-corner" aria-hidden="true">
              <span>{t('entrance.landing.est')}</span>
              <DoorIcon width={34} height={34} />
              <span>{t('entrance.landing.invOnly')}</span>
            </div>
            <p className="ent-vertical" aria-hidden="true">
              {t('entrance.landing.vertical')}
            </p>
          </div>
          <div className="ent-hero-foot">
            <p>
              <span className="ent-bullet" aria-hidden="true" />
              {t('entrance.landing.footLine')}
            </p>
          </div>
        </section>
      </main>
    </EntranceShell>
  );
}
