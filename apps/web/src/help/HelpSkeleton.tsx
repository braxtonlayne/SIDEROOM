import { EntranceShell } from '../components/EntranceShell';

/**
 * What `/help` and `/help/whats-new` show while the handbook's words (the full message catalog) or the changelog
 * chunk are still arriving: the entrance's own chrome with placeholder lines, announced politely. It uses no
 * `t()` key, so it paints from the first-paint fragments alone.
 */
export function HelpSkeleton() {
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose ent-notes" role="status" aria-busy="true" aria-live="polite">
        <div className="ent-skeleton ent-skeleton-title" />
        <div className="ent-skeleton" />
        <div className="ent-skeleton" />
        <div className="ent-skeleton ent-skeleton-short" />
      </main>
    </EntranceShell>
  );
}
