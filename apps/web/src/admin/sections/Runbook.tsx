/**
 * Admin > Runbook (T-03, D-369): `deploy/RUNBOOK.md` as shipped with this build, rendered by the restricted
 * Markdown renderer `/help/whats-new` uses. Every heading carries the anchor `scripts/lib/playbooks.mjs` checks
 * (`runbookSlug` below is the same rule; `p23runbook.test.tsx` keeps the two equal), so the Admin > System tiles
 * and the alert rules can point at `/admin/runbook#<anchor>`. This module is a lazy chunk of the Admin bundle.
 */
import { useRouterState } from '@tanstack/react-router';
import { useEffect, useRef } from 'react';
import { useT } from '../../i18n';
import { EntranceMarkdown } from '../../routes/entrance/entranceCopy';
import runbook from '../../../../../deploy/RUNBOOK.md?raw';
import { SectionHeader } from '../ui';

/** The heading anchor rule of `scripts/lib/playbooks.mjs` `slug` (GitHub's heading anchors). */
export function runbookSlug(heading: string): string {
  return heading
    .toLowerCase()
    .replace(/`/g, '')
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .trim()
    .replace(/\s/g, '-');
}

/** The file without its `# Runbook` title line (the section header is the page title). */
export function runbookBody(source: string): string {
  return source.replace(/\r\n?/g, '\n').replace(/^# [^\n]*\n/, '');
}

function focusHash(root: HTMLElement | null, raw: string) {
  let hash = raw.replace(/^#/, '');
  try {
    hash = decodeURIComponent(hash);
  } catch {
    // A malformed escape: look the raw text up.
  }
  if (!root || !hash) return;
  const target = root.ownerDocument.getElementById(hash);
  if (!target || !root.contains(target)) return;
  target.setAttribute('tabindex', '-1');
  target.scrollIntoView?.({ block: 'start' });
  target.focus({ preventScroll: true });
}

export function RunbookSection() {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  // The router's hash (a tile link) and the browser's (a link inside the runbook) both move the focus.
  const hash = useRouterState({ select: (s) => s.location.hash });
  useEffect(() => focusHash(ref.current, hash), [hash]);
  useEffect(() => {
    const onHash = () => focusHash(ref.current, window.location.hash);
    window.addEventListener('hashchange', onHash);
    return () => window.removeEventListener('hashchange', onHash);
  }, []);
  return (
    <>
      <SectionHeader
        title={t('admin.runbook.title')}
        description={t('admin.runbook.description')}
      />
      <div ref={ref} className="adm-runbook" data-testid="runbook">
        <EntranceMarkdown source={runbookBody(runbook)} headingId={runbookSlug} />
      </div>
    </>
  );
}
