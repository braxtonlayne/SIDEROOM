import { useRouter } from '@tanstack/react-router';
import { useEffect, useState, type ReactNode } from 'react';
import { apiFetch } from '../api/client';
import { useT, type MessageKey } from '../i18n';
import { useCompliancePublic } from '../lib/compliancePublic';
import { saveFirstRunDone } from '../lounge/shell/firstRun';
import { BridgedChannels } from './BridgedChannels';

/**
 * The member handbook (RDY-ONB-002), the body of `/help`. All words live in the i18n catalog
 * (`p23-first-run-1.ts`); this file only lists the sections, in order, with the screen each one is about.
 * The privacy section agrees with docs/09-retention-and-privacy.md.
 */
export interface HandbookSection {
  id: string;
  title: MessageKey;
  /** Paragraphs, in order. */
  body: MessageKey[];
  /** A bulleted list after the paragraphs. */
  list?: MessageKey[];
  /** Short code-like lines (file names) shown set in monospace. */
  examples?: MessageKey[];
  /** Paragraphs after the list and examples. */
  after?: MessageKey[];
  /** The screens the section describes: an in-app path (with an optional `#anchor`) and its label. */
  links: { href: string; label: MessageKey }[];
}

export const HANDBOOK_SECTIONS: readonly HandbookSection[] = [
  {
    id: 'joining',
    title: 'help.handbook.joining.title',
    body: ['help.handbook.joining.body1', 'help.handbook.joining.body2'],
    links: [{ href: '/settings/user/invites', label: 'help.handbook.joining.link' }],
  },
  {
    id: 'signin',
    title: 'help.handbook.signin.title',
    body: ['help.handbook.signin.body1', 'help.handbook.signin.body2'],
    links: [{ href: '/settings/user/account', label: 'help.handbook.signin.link' }],
  },
  {
    id: 'install',
    title: 'help.handbook.install.title',
    body: [
      'help.handbook.install.ios',
      'help.handbook.install.android',
      'help.handbook.install.notify',
    ],
    links: [{ href: '/settings/user/notifications', label: 'help.handbook.install.link' }],
  },
  {
    id: 'lounge',
    title: 'help.handbook.lounge.title',
    body: ['help.handbook.lounge.intro'],
    list: [
      'help.handbook.lounge.diff1',
      'help.handbook.lounge.diff2',
      'help.handbook.lounge.diff3',
      'help.handbook.lounge.diff4',
      'help.handbook.lounge.diff5',
    ],
    links: [{ href: '/channels', label: 'help.handbook.lounge.link' }],
  },
  {
    id: 'submit',
    title: 'help.handbook.submit.title',
    body: ['help.handbook.submit.body'],
    examples: [
      'help.handbook.submit.libraryName',
      'help.handbook.submit.movieName',
      'help.handbook.submit.showName',
    ],
    after: ['help.handbook.submit.tags'],
    links: [
      { href: '/library/submissions', label: 'help.handbook.submit.libraryLink' },
      { href: '/cinema/submit', label: 'help.handbook.submit.cinemaLink' },
    ],
  },
  {
    id: 'parties',
    title: 'help.handbook.parties.title',
    body: ['help.handbook.parties.body'],
    links: [{ href: '/cinema', label: 'help.handbook.parties.link' }],
  },
  {
    id: 'tv',
    title: 'help.handbook.tv.title',
    body: ['help.handbook.tv.body'],
    links: [{ href: '/cinema/tv', label: 'help.handbook.tv.link' }],
  },
  {
    id: 'storage',
    title: 'help.handbook.storage.title',
    body: ['help.handbook.storage.body1', 'help.handbook.storage.body2'],
    links: [{ href: '/settings/user/storage', label: 'help.handbook.storage.link' }],
  },
  {
    id: 'player',
    title: 'help.handbook.player.title',
    body: ['help.handbook.player.body1', 'help.handbook.player.body2'],
    links: [{ href: '/settings/user/appearance', label: 'help.handbook.player.link' }],
  },
  {
    id: 'drawer',
    title: 'help.handbook.drawer.title',
    body: ['help.handbook.drawer.body1', 'help.handbook.drawer.body2'],
    links: [{ href: '/channels', label: 'help.handbook.drawer.link' }],
  },
  {
    id: 'profiles',
    title: 'help.handbook.profiles.title',
    body: ['help.handbook.profiles.body1', 'help.handbook.profiles.body2'],
    links: [{ href: '/settings/user/profile', label: 'help.handbook.profiles.link' }],
  },
  {
    id: 'bridge',
    title: 'help.handbook.bridge.title',
    body: [
      'help.handbook.bridge.body1',
      'help.handbook.bridge.body2',
      'help.handbook.bridge.body3',
    ],
    links: [{ href: '/channels', label: 'help.handbook.bridge.link' }],
  },
  {
    id: 'activities',
    title: 'help.handbook.activities.title',
    body: ['help.handbook.activities.body1', 'help.handbook.activities.body2'],
    links: [{ href: '/channels', label: 'help.handbook.activities.link' }],
  },
  {
    id: 'emulator',
    title: 'help.handbook.emulator.title',
    body: ['help.handbook.emulator.body1', 'help.handbook.emulator.body2'],
    links: [{ href: '/games', label: 'help.handbook.emulator.link' }],
  },
  {
    id: 'games',
    title: 'help.handbook.games.title',
    body: ['help.handbook.games.body'],
    links: [{ href: '/channels', label: 'help.handbook.games.link' }],
  },
  {
    id: 'privacy',
    title: 'help.handbook.privacy.title',
    body: [
      'help.handbook.privacy.stored',
      'help.handbook.privacy.export',
      'help.handbook.privacy.delete',
      'help.handbook.privacy.report',
      'help.handbook.privacy.profiles',
      'help.handbook.privacy.bridge',
    ],
    links: [
      { href: '/settings/user/privacy', label: 'help.handbook.privacy.exportLink' },
      { href: '/settings/user/account', label: 'help.handbook.privacy.deleteLink' },
      { href: '/house-notes#privacy', label: 'help.handbook.privacy.notesLink' },
    ],
  },
  {
    id: 'operators',
    title: 'help.handbook.operators.title',
    body: ['help.handbook.operators.body'],
    links: [{ href: '/help/operators', label: 'help.handbook.operators.link' }],
  },
  {
    id: 'house',
    title: 'help.handbook.house.title',
    body: ['help.handbook.house.body'],
    links: [{ href: '/house-notes', label: 'help.handbook.house.link' }],
  },
];

/** A link to an in-app screen: a normal anchor (so it can be opened in a tab) that navigates without a reload. */
function AppLink({ href, children }: { href: string; children: ReactNode }) {
  const router = useRouter();
  return (
    <a
      href={href}
      onClick={(e) => {
        if (
          e.defaultPrevented ||
          e.button !== 0 ||
          e.metaKey ||
          e.ctrlKey ||
          e.shiftKey ||
          e.altKey
        )
          return;
        e.preventDefault();
        router.history.push(href);
      }}
    >
      {children}
    </a>
  );
}

/** Signed in? Asked once; a 401 (or any failure) means no, and the welcome section stays out of sight. */
function useSignedIn(): boolean {
  const [signedIn, setSignedIn] = useState(false);
  useEffect(() => {
    let live = true;
    apiFetch('/users/@me', { retries: 0 }).then(
      () => live && setSignedIn(true),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, []);
  return signedIn;
}

/** "Show the welcome again": stores the first run as not done and opens the Lounge, where it shows. */
function WelcomeAgain() {
  const t = useT();
  const router = useRouter();
  const [failed, setFailed] = useState(false);
  const click = () => {
    setFailed(false);
    saveFirstRunDone(false).then(
      () => router.history.push('/channels'),
      () => setFailed(true),
    );
  };
  return (
    <section id="welcome" aria-labelledby="hb-welcome">
      <h2 id="hb-welcome">{t('help.handbook.welcome.title')}</h2>
      <p>{t('help.handbook.welcome.body')}</p>
      <p>
        <button type="button" className="ent-button" onClick={click}>
          {t('help.handbook.welcome.button')}
        </button>
      </p>
      {failed && <p role="alert">{t('help.handbook.welcome.failed')}</p>}
    </section>
  );
}

export function Handbook() {
  const t = useT();
  const signedIn = useSignedIn();
  // The live number of credits a new account gets; the wording with "a handful" shows until it is known.
  const {
    operator_country: country,
    operator_contact: contact,
    invite_initial_credits: credits,
  } = useCompliancePublic();
  return (
    <>
      <p>{t('help.handbook.intro')}</p>
      <nav aria-labelledby="hb-contents">
        <h2 id="hb-contents">{t('help.handbook.contents')}</h2>
        <ol>
          {HANDBOOK_SECTIONS.map((s) => (
            <li key={s.id}>
              <a href={`#hb-${s.id}`}>{t(s.title)}</a>
            </li>
          ))}
        </ol>
        <p>
          <AppLink href="/help/whats-new">{t('help.handbook.whatsNew.link')}</AppLink>
        </p>
      </nav>
      {signedIn && <WelcomeAgain />}
      {HANDBOOK_SECTIONS.map((s) => (
        <section key={s.id} id={s.id} aria-labelledby={`hb-${s.id}`}>
          <h2 id={`hb-${s.id}`}>{t(s.title)}</h2>
          {s.body.map((k) => (
            <p key={k}>
              {k === 'help.handbook.joining.body1' && credits !== undefined
                ? t('help.handbook.joining.body1Count', { count: credits })
                : t(k)}
            </p>
          ))}
          {s.list && (
            <ul>
              {s.list.map((k) => (
                <li key={k}>{t(k)}</li>
              ))}
            </ul>
          )}
          {s.examples && (
            <ul>
              {s.examples.map((k) => (
                <li key={k}>
                  <code>{t(k)}</code>
                </li>
              ))}
            </ul>
          )}
          {s.after?.map((k) => (
            <p key={k}>{t(k)}</p>
          ))}
          {s.id === 'privacy' && signedIn && <BridgedChannels />}
          {s.id === 'privacy' && (country || contact) && (
            <>
              {country && <p>{t('entrance.houseNotes.privacy.operatorCountry', { country })}</p>}
              {contact && (
                <p>
                  {t('entrance.houseNotes.privacy.operatorContact')}{' '}
                  <a href={`mailto:${contact}`}>{contact}</a>
                </p>
              )}
            </>
          )}
          <ul className="ent-links">
            {s.links.map((l) => (
              <li key={l.href + l.label}>
                <AppLink href={l.href}>{t(l.label)}</AppLink>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </>
  );
}
