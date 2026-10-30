// The ring on an avatar. `interactive={false}` is the decoration only, for avatars that already sit inside a link or
// button (a list row): it adds no focusable element and no nested control. The default wraps the avatar in a button
// that opens the viewer.
import { useState, type ReactNode } from 'react';
import { useT } from '../../i18n';
import { RingViewer } from './RingViewer';
import { useRingsOf } from './store';
import './rings.css';

export function RingAvatar({
  userId,
  name,
  interactive = true,
  children,
}: {
  userId: string | undefined;
  name: string;
  interactive?: boolean;
  children: ReactNode;
}) {
  const t = useT();
  const entry = useRingsOf(userId);
  const [open, setOpen] = useState(false);
  if (!entry) return <>{children}</>;
  const unseen = !entry.all_seen;
  const label = unseen
    ? t('lounge.rings.avatar.hasUnseen', { name })
    : t('lounge.rings.avatar.has', { name });
  if (!interactive) {
    return (
      <span className="rng-wrap" data-unseen={unseen || undefined}>
        {children}
        <span className="rng-sr">{label}</span>
      </span>
    );
  }
  return (
    <>
      <span className="rng-wrap" data-unseen={unseen || undefined}>
        <button type="button" className="rng-btn" aria-label={label} onClick={() => setOpen(true)}>
          {children}
        </button>
      </span>
      {open && <RingViewer entry={entry} name={name} onClose={() => setOpen(false)} />}
    </>
  );
}
