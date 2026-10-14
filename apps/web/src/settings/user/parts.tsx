import { Button, Notice, Spinner } from '@sideroom/ui';
import type { ReactNode } from 'react';
import { formatDateTime, useT } from '../../i18n';
import { errorText, type Resource } from './api';
import './user-settings.css';

export function SectionTitle({ children }: { children: ReactNode }) {
  return <h1 className="us-title">{children}</h1>;
}

export function Card({
  title,
  description,
  actions,
  children,
  id,
}: {
  title: string;
  description?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
  id?: string;
}) {
  return (
    <section className="us-card" aria-labelledby={id ? `${id}-h` : undefined} id={id}>
      <div className="us-card-head">
        <div>
          <h2 className="us-card-title" id={id ? `${id}-h` : undefined}>
            {title}
          </h2>
          {description && <p className="us-muted">{description}</p>}
        </div>
        {actions && <div className="us-actions">{actions}</div>}
      </div>
      {children}
    </section>
  );
}

/** A label/value line inside a card. */
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="us-field">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

/** Shown where the server has no endpoint yet: states the gap instead of faking a working control. */
export function Unavailable({ children }: { children: ReactNode }) {
  const t = useT();
  return (
    <Notice variant="info" title={t('settings.parts.unavailableTitle')} className="us-gap">
      {children}
    </Notice>
  );
}

/** Loading / error wrapper for a Resource. */
export function Loaded<T>({
  res,
  what,
  children,
}: {
  res: Resource<T>;
  what: string;
  children: (data: T) => ReactNode;
}) {
  const t = useT();
  if (res.data !== undefined) return <>{children(res.data)}</>;
  if (res.loading)
    return (
      <p className="us-muted" role="status">
        <Spinner size="sm" label={t('settings.parts.loadingAria', { what })} />{' '}
        {t('settings.parts.loading', { what })}
      </p>
    );
  return (
    <Notice variant="danger" title={t('settings.parts.loadError', { what })}>
      {errorText(res.error)}{' '}
      <Button variant="link" size="sm" onClick={res.reload}>
        {t('settings.parts.tryAgain')}
      </Button>
    </Notice>
  );
}

export function fmtDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : formatDateTime(d);
}

export function displayOf(u: { username?: string; global_name?: string | null; id: string }) {
  return u.global_name || u.username || u.id;
}
