import { Notice } from '@sideroom/ui';
import type { ReactNode } from 'react';
import { DiscordIcon } from './icons';

/** "or make yourself at home" rule. */
export function Divider({ children }: { children: ReactNode }) {
  return (
    <div className="ent-divider" role="separator">
      <span>{children}</span>
    </div>
  );
}

/** Local primitive: the wide Discord button (packages/ui has no brand/outline-wide variant). */
export function DiscordButton({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a className="ent-discord" href={href}>
      <DiscordIcon width={22} height={22} />
      <span>{children}</span>
      <span aria-hidden="true" className="ent-discord-arrow">
        ↗
      </span>
    </a>
  );
}

export function FormError({ children }: { children: ReactNode }) {
  if (!children) return null;
  return (
    <Notice variant="danger" role="alert" className="ent-notice">
      {children}
    </Notice>
  );
}
