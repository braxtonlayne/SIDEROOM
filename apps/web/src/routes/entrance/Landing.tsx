import { DisplayHeading, Eyebrow, GoldButton } from '@sideroom/ui';
import { Link, useRouter } from '@tanstack/react-router';
import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { checkInvite } from '../../api/entrance';
import { Door } from '../../components/Door';
import { EntranceShell } from '../../components/EntranceShell';
import { BookIcon, ChatIcon, DoorIcon, FilmIcon, KeyIcon, LockIcon } from '../../components/icons';
import { formatInviteCode, holdInviteCode, isCompleteInviteCode } from '../../lib/inviteCode';
import { useRedirectIfLoggedIn } from '../../lib/useRedirectIfLoggedIn';

type CheckState = 'idle' | 'checking' | 'valid' | 'invalid' | 'closed' | 'limited' | 'error';

const MESSAGES: Partial<Record<CheckState, { tone: 'good' | 'bad'; text: string }>> = {
  valid: { tone: 'good', text: "You're on the list." },
  invalid: { tone: 'bad', text: "That invitation doesn't open this door." },
  closed: { tone: 'bad', text: "The door's closed for now. We're at capacity." },
  limited: { tone: 'bad', text: 'Too many tries. Take a breath and try again later.' },
  error: { tone: 'bad', text: 'Something went wrong on our side. Try again in a moment.' },
};

const WORLDS = [
  { n: '01', Icon: ChatIcon, title: 'The lounge', copy: 'Good company. Open conversations.' },
  { n: '02', Icon: FilmIcon, title: 'The cinema', copy: 'Something worth staying up for.' },
  { n: '03', Icon: BookIcon, title: 'The library', copy: 'One more chapter. Then another.' },
];

export function Landing({ initialCode = '' }: { initialCode?: string }) {
  useRedirectIfLoggedIn();
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
            <Eyebrow>Good things stay between us.</Eyebrow>
            <DisplayHeading id="hero-title" size="xl" accent="closed." className="ent-big">
              The internet, with the door
            </DisplayHeading>
            <p className="ent-lede">
              A little less noise. A little more belonging.
              <br />A place for your people, and everything you’re into.
            </p>
            <form onSubmit={onSubmit} noValidate>
              <label className="ent-invite-label" htmlFor="invite-code">
                Your invitation
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
                  placeholder="Enter your invite code"
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
                  {state === 'checking' ? 'Checking…' : 'Check invitation'}
                </GoldButton>
              </div>
              <p
                id="invite-msg"
                className="ent-invite-msg"
                data-tone={msg?.tone}
                role="status"
                aria-live="polite"
              >
                {msg?.text}
              </p>
            </form>
            <div className="ent-helper">
              <span>
                <LockIcon width={14} height={14} /> Invitation required. Always.
              </span>
              <Link to="/no-invitation">No invitation?</Link>
            </div>
          </div>
          <div className="ent-hero-right">
            <Door className="ent-door-bg" />
            <div className="ent-corner" aria-hidden="true">
              <span>Est. after hours</span>
              <DoorIcon width={34} height={34} />
              <span>By invitation only</span>
            </div>
            <p className="ent-vertical" aria-hidden="true">
              Some doors are worth finding.
            </p>
          </div>
          <div className="ent-hero-foot">
            <p>
              <span className="ent-bullet" aria-hidden="true" />
              The outside is quiet. The inside is yours.
            </p>
            <button
              type="button"
              className="ent-glimpse"
              onClick={() => strip.current?.scrollIntoView({ behavior: 'smooth' })}
            >
              A glimpse inside <span aria-hidden="true">↓</span>
            </button>
          </div>
        </section>
        <section className="ent-strip" ref={strip} aria-labelledby="strip-title">
          <div className="ent-strip-head">
            <h2 id="strip-title">
              One door. <em>A few different worlds.</em>
            </h2>
            <p>For members, together.</p>
          </div>
          <ul className="ent-worlds">
            {WORLDS.map(({ n, Icon, title, copy }) => (
              <li key={n}>
                <div className="ent-world-num">
                  <span>{n} /</span>
                  <Icon />
                </div>
                <h3>{title}</h3>
                <p>{copy}</p>
              </li>
            ))}
          </ul>
        </section>
      </main>
    </EntranceShell>
  );
}
