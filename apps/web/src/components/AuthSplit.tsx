import { DisplayHeading, Eyebrow } from '@sideroom/ui';
import type { ReactNode } from 'react';
import { useT } from '../i18n/react';
import { Door } from './Door';
import { EntranceShell } from './EntranceShell';

export interface AuthSplitProps {
  eyebrow: string;
  heading: ReactNode;
  accent: ReactNode;
  copy: ReactNode;
  footLabel: string;
  /** Right-panel content. */
  children: ReactNode;
  nav?: 'member-access' | 'back-to-entrance';
}

/** Two-panel layout shared by login, register, 2FA and password reset. */
export function AuthSplit({
  eyebrow,
  heading,
  accent,
  copy,
  footLabel,
  children,
  nav = 'back-to-entrance',
}: AuthSplitProps) {
  const t = useT();
  return (
    <EntranceShell nav={nav}>
      <main className="ent-split">
        <section className="ent-split-left" aria-label={t('entrance.auth.welcome')}>
          <Door className="ent-door-bg" />
          <div className="ent-split-copy">
            <Eyebrow>{eyebrow}</Eyebrow>
            <DisplayHeading as="h2" size="xl" accent={accent} className="ent-big">
              {heading}
            </DisplayHeading>
            <p className="ent-lede">{copy}</p>
          </div>
          <div className="ent-split-foot">
            <p>
              <span className="ent-bullet" aria-hidden="true" /> {t('entrance.auth.foot')}
            </p>
            <p className="ent-meta">{footLabel}</p>
          </div>
        </section>
        <section className="ent-split-right">
          <div className="ent-formwrap">{children}</div>
        </section>
      </main>
    </EntranceShell>
  );
}

export function FormHead({
  eyebrow,
  title,
  sub,
}: {
  eyebrow: string;
  title: string;
  sub?: string;
}) {
  return (
    <header className="ent-formhead">
      <p className="ent-form-eyebrow">{eyebrow}</p>
      <h1>{title}</h1>
      {sub && <p className="ent-sub">{sub}</p>}
    </header>
  );
}
