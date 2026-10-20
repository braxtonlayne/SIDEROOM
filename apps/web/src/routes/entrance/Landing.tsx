import { DisplayHeading, Eyebrow, GoldButton } from '@sideroom/ui';
import { EntranceMarkdown, useEntranceCopy, useEntranceHero } from './entranceCopy';
import { Link, useRouter } from '@tanstack/react-router';
import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { checkInvite } from '../../api/entrance';
import { Door } from '../../components/Door';
import { EntranceShell } from '../../components/EntranceShell';
import { BookIcon, ChatIcon, DoorIcon, FilmIcon, KeyIcon, LockIcon } from '../../components/icons';
import { useT } from '../../i18n/react';
import type { MessageKey } from '../../i18n/messages/en';
import { useCompliancePublic } from '../../lib/compliancePublic';
import { formatInviteCode, holdInviteCode, isCompleteInviteCode } from '../../lib/inviteCode';
import { useRedirectIfLoggedIn } from '../../lib/useRedirectIfLoggedIn';

type CheckState = 'idle' | 'checking' | 'valid' | 'invalid' | 'closed' | 'limited' | 'error';

const MESSAGES: Partial<Record<CheckState, { tone: 'good' | 'bad'; key: MessageKey }>> = {
  valid: { tone: 'good', key: 'entrance.landing.valid' },
  invalid: { tone: 'bad', key: 'entrance.landing.invalid' },
  closed: { tone: 'bad', key: 'entrance.landing.closed' },
  limited: { tone: 'bad', key: 'entrance.landing.limited' },
  error: { tone: 'bad', key: 'entrance.landing.error' },
};

const WORLDS: { n: string; Icon: typeof ChatIcon; title: MessageKey; copy: MessageKey }[] = [
  {
    n: '01',
    Icon: ChatIcon,
    title: 'entrance.landing.world.lounge.title',
    copy: 'entrance.landing.world.lounge.copy',
  },
  {
    n: '02',
    Icon: FilmIcon,
    title: 'entrance.landing.world.cinema.title',
    copy: 'entrance.landing.world.cinema.copy',
  },
  {
    n: '03',
    Icon: BookIcon,
    title: 'entrance.landing.world.library.title',
    copy: 'entrance.landing.world.library.copy',
  },
];

export function Landing({ initialCode = '' }: { initialCode?: string }) {
  const copy = useEntranceCopy();
  const note = copy['landing-note'];
  const hero = useEntranceHero();
  useRedirectIfLoggedIn();
  const t = useT();
  const compliance = useCompliancePublic();
  const router = useRouter();
  const [code, setCode] = useState(() => formatInviteCode(initialCode));
  const [state, setState] = useState<CheckState>('idle');
  const [shakes, setShakes] = useState(0);
  const strip = useRef<HTMLElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const complete = isCompleteInviteCode(code);

  const submit = useCallback(
    async (value: string) => {
      if (!isCompleteInviteCode(value)) return;
      setState('checking');
      try {
        const res = await checkInvite(value);
        if (res.valid) {
          holdInviteCode(value);
          setState('valid');
          timer.current = setTimeout(() => void router.navigate({ to: '/register' }), 600);
        } else if (res.reason === 'closed') {
          setState('closed');
        } else {
          setState('invalid');
          setShakes((n) => n + 1);
        }
      } catch (e) {
        setState(e instanceof ApiError && e.status === 429 ? 'limited' : 'error');
      }
    },
    [router],
  );

  // /join/:code auto-checks once on arrival.
  const auto = useRef(false);
  useEffect(() => {
    if (initialCode && !auto.current) {
      auto.current = true;
      void submit(formatInviteCode(initialCode));
    }
  }, [initialCode, submit]);
  useEffect(() => () => clearTimeout(timer.current), []);

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
                  value={code}
                  onChange={(e) => {
                    setCode(formatInviteCode(e.target.value));
                    if (state !== 'checking') setState('idle');
                  }}
                  placeholder={t('entrance.landing.invitePlaceholder')}
                  autoComplete="off"
                  autoCapitalize="characters"
                  spellCheck={false}
                  aria-invalid={state === 'invalid' || state === 'closed' ? true : undefined}
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
              <Link to="/no-invitation">{t('entrance.landing.noInvitation')}</Link>
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
            <button
              type="button"
              className="ent-glimpse"
              onClick={() => strip.current?.scrollIntoView({ behavior: 'smooth' })}
            >
              {t('entrance.landing.glimpse')} <span aria-hidden="true">↓</span>
            </button>
          </div>
        </section>
        <section className="ent-strip" ref={strip} aria-labelledby="strip-title">
          <div className="ent-strip-head">
            <h2 id="strip-title">
              {t('entrance.landing.stripTitle')} <em>{t('entrance.landing.stripAccent')}</em>
            </h2>
            <p>{t('entrance.landing.stripSub')}</p>
          </div>
          <ul className="ent-worlds">
            {WORLDS.map(({ n, Icon, title, copy }) => (
              <li key={n}>
                <div className="ent-world-num">
                  <span>{n} /</span>
                  <Icon />
                </div>
                <h3>{t(title)}</h3>
                <p>{t(copy)}</p>
              </li>
            ))}
          </ul>
        </section>
      </main>
    </EntranceShell>
  );
}
