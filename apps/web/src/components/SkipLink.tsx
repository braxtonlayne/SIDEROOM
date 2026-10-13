import type { MouseEvent } from 'react';
import { useT } from '../i18n/react';

/**
 * "Skip to main content" (XC-A11Y-001): the first tab stop on every page. Hidden until focused.
 * It targets the first `<main>` (or `[role=main]`) so pages need no id plumbing.
 */
export function SkipLink({ target = 'main, [role="main"]' }: { target?: string }) {
  const t = useT();
  const go = (e: MouseEvent) => {
    const el = document.querySelector<HTMLElement>(target);
    if (!el) return;
    e.preventDefault();
    if (!el.hasAttribute('tabindex')) el.setAttribute('tabindex', '-1');
    el.focus();
    el.scrollIntoView?.({ block: 'start' });
  };
  return (
    <a href="#main" className="sr-skip-link" onClick={go}>
      {t('a11y.skipToContent')}
    </a>
  );
}
