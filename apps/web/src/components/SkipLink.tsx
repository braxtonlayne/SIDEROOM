import { useEffect, type MouseEvent } from 'react';
import { useT } from '../i18n/react';

const TARGET = 'main, [role="main"]';
const ID = 'main-content';

/**
 * "Skip to main content" (XC-A11Y-001): the first tab stop on every page. Hidden until focused.
 * It targets the first `<main>` (or `[role=main]`), giving it an id on mount, so pages need no plumbing.
 */
export function SkipLink() {
  const t = useT();
  useEffect(() => {
    const el = document.querySelector<HTMLElement>(TARGET);
    if (el && !el.id) el.id = ID;
  });
  const go = (e: MouseEvent) => {
    const el = document.querySelector<HTMLElement>(TARGET);
    if (!el) return;
    e.preventDefault();
    if (!el.hasAttribute('tabindex')) el.setAttribute('tabindex', '-1');
    el.focus();
    el.scrollIntoView?.({ block: 'start' });
  };
  return (
    <a href={`#${ID}`} className="sr-skip-link" onClick={go}>
      {t('a11y.skipToContent')}
    </a>
  );
}
