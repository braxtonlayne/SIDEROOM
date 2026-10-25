import { useEffect, useState, type MouseEvent } from 'react';
import { useT } from '../i18n/react';

const TARGET = 'main, [role="main"]';
const ID = 'main-content';

/**
 * "Skip to main content" (XC-A11Y-001): the first tab stop on every page. Hidden until focused.
 * It targets the first `<main>` (or `[role=main]`), giving it an id on mount, so pages need no plumbing.
 */
export function SkipLink() {
  const t = useT();
  // The link points at the region's own id when it has one (cin-main, lib-content, lounge-main): a link to an id
  // that does not exist is an axe `skip-link` finding and a dead link without scripting.
  const [target, setTarget] = useState(ID);
  useEffect(() => {
    const el = document.querySelector<HTMLElement>(TARGET);
    if (!el) return;
    if (!el.id) el.id = ID;
    setTarget(el.id);
  }, []);
  const go = (e: MouseEvent) => {
    const el = document.querySelector<HTMLElement>(TARGET);
    if (!el) return;
    e.preventDefault();
    if (!el.hasAttribute('tabindex')) el.setAttribute('tabindex', '-1');
    el.focus();
    el.scrollIntoView?.({ block: 'start' });
  };
  return (
    <a href={`#${target}`} className="sr-skip-link" onClick={go}>
      {t('a11y.skipToContent')}
    </a>
  );
}
