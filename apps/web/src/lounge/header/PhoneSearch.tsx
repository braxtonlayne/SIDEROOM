import { Icon } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useT } from '../../i18n';
import { SearchBox } from '../SearchBox';
import './header.css';

/**
 * Search on a phone (D-740). The desktop's inline search box is hidden at phone width, so the header shows this
 * button instead; it opens the same `SearchBox` (box, filters, results) as a sheet that fills the chat pane (not the
 * whole app: the pane switching by swipe works again once it closes). It closes with its button, Escape, the back
 * gesture (one history entry is pushed while it is open) and by choosing a hit; focus returns to the button.
 */
export function PhoneSearch({
  guildId,
  channelId,
  label,
}: {
  guildId: string | undefined;
  channelId: string;
  label: string;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  // The history entry pushed for the open sheet is still there.
  const pushed = useRef(false);
  const [host, setHost] = useState<Element | null>(null);

  const show = () => {
    setHost(button.current?.closest('.lng-main') ?? null);
    try {
      window.history.pushState(window.history.state, '');
      pushed.current = true;
    } catch {
      pushed.current = false;
    }
    setOpen(true);
  };

  /** Close from the button or Escape: take the pushed entry back off, so "back" leaves the channel as before. */
  const close = useCallback(() => {
    setOpen(false);
    if (pushed.current) {
      pushed.current = false;
      window.history.back();
    }
  }, []);

  /** A chosen hit navigates by replacing the pushed entry, so there is nothing to take off. */
  const jumped = useCallback(() => {
    pushed.current = false;
    setOpen(false);
  }, []);

  // The back gesture closes the sheet; so does moving to another channel.
  useEffect(() => {
    if (!open) return;
    const onPop = () => {
      pushed.current = false;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
    };
    window.addEventListener('popstate', onPop);
    document.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('popstate', onPop);
      document.removeEventListener('keydown', onKey);
    };
  }, [open, close]);
  useEffect(() => () => setOpen(false), [channelId]);

  // Focus goes back to the button once the sheet is gone.
  const wasOpen = useRef(false);
  useEffect(() => {
    if (wasOpen.current && !open) button.current?.focus();
    wasOpen.current = open;
  }, [open]);

  const sheet = open && (
    <div className="lng-search-sheet" role="dialog" aria-label={t('lounge.phoneSearch.title')}>
      <div className="lng-search-sheet-head">
        <button
          type="button"
          className="lng-icon-btn lng-search-sheet-close"
          aria-label={t('lounge.phoneSearch.close')}
          onClick={close}
        >
          <Icon name="arrow-left" size={20} />
        </button>
        <h2 className="lng-search-sheet-title">{t('lounge.phoneSearch.title')}</h2>
      </div>
      <SearchBox
        guildId={guildId}
        channelId={channelId}
        label={label}
        sheet={{ onClose: close, onJump: jumped }}
      />
    </div>
  );

  return (
    <>
      <button
        ref={button}
        type="button"
        className="lng-icon-btn lng-phone-search"
        aria-label={t('lounge.phoneSearch.open')}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={show}
      >
        <Icon name="search" size={20} />
      </button>
      {sheet && (host ? createPortal(sheet, host) : sheet)}
    </>
  );
}
