// The report dialog (MOD-RPT-010; spec section 3.2). Mounted once at the app root. The client sends only ids and
// the note; the server snapshots the evidence, routes the report and keeps the reporter hidden from the reported
// person (spec 3.3). The form itself is a lazy chunk (ReportForm.tsx): the first paint carries only this stub.
import { lazy, Suspense } from 'react';
import type { MessageKey } from '../i18n';
import type { ReportCategory } from './api';
import { useReports } from './reports';

const ReportForm = lazy(() => import('./ReportForm').then((m) => ({ default: m.ReportForm })));

/** Labels of the categories, resolved at render time. */
export const CATEGORY_KEYS: Record<ReportCategory, MessageKey> = {
  spam: 'moderation.category.spam',
  harassment: 'moderation.category.harassment',
  illegal_content: 'moderation.category.illegalContent',
  child_safety: 'moderation.category.childSafety',
  ban_evasion: 'moderation.category.banEvasion',
  fraud: 'moderation.category.fraud',
  platform_abuse: 'moderation.category.platformAbuse',
  nsfw_policy: 'moderation.category.nsfwPolicy',
  other: 'moderation.category.other',
};

/** Mounted once at the app root. */
export function ReportDialog() {
  const target = useReports((s) => s.target);
  if (!target) return null;
  return (
    <Suspense fallback={null}>
      <ReportForm key={`${target.kind}:${target.id}`} target={target} />
    </Suspense>
  );
}
