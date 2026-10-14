// Pieces shared by the reviewer workspace (Admin) and the submitter view (Library): state chips,
// the timeline, the Library preview and the "what changed since the last submit" diff.
import { Badge, type BadgeVariant } from '@sideroom/ui';
import { useEffect } from 'react';
import { onDispatch } from '../store/events';
import type { ItemState, LibraryPreview, SubmissionEvent, SubmissionState } from './types';
import './review.css';

export const STATE_LABEL: Record<SubmissionState, string> = {
  draft: 'Draft',
  submitted: 'Waiting for review',
  in_review: 'In review',
  changes_requested: 'Changes requested',
  approved: 'Approved',
  rejected: 'Rejected',
  published: 'Published',
  withdrawn: 'Withdrawn',
};

const STATE_VARIANT: Record<SubmissionState, BadgeVariant> = {
  draft: 'neutral',
  submitted: 'accent',
  in_review: 'accent',
  changes_requested: 'warning',
  approved: 'success',
  rejected: 'danger',
  published: 'success',
  withdrawn: 'neutral',
};

export function StateChip({ state }: { state: SubmissionState }) {
  return <Badge variant={STATE_VARIANT[state]}>{STATE_LABEL[state]}</Badge>;
}

const ITEM_VARIANT: Record<ItemState, BadgeVariant> = {
  pending: 'neutral',
  approved: 'success',
  rejected: 'danger',
  published: 'success',
};
const ITEM_LABEL: Record<ItemState, string> = {
  pending: 'Pending',
  approved: 'Approved',
  rejected: 'Rejected',
  published: 'Published',
};
export function ItemChip({ state }: { state: ItemState }) {
  return <Badge variant={ITEM_VARIANT[state]}>{ITEM_LABEL[state]}</Badge>;
}

export const fmtWhen = (ms: number | null | undefined) =>
  ms ? new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) : '';

export function ago(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return 'just now';
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h} h`;
  return `${Math.round(h / 24)} d`;
}

/** Run `fn` whenever the gateway says one of my submissions (or a request I voted on) changed. */
export function useSubmissionUpdates(fn: () => void): void {
  useEffect(
    () =>
      onDispatch((event) => {
        if (event === 'SR_SUBMISSION_UPDATE') fn();
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );
}

// ---- timeline ----

const EVENT_LABEL: Record<string, string> = {
  created: 'Draft created',
  edited: 'Details edited',
  submitted: 'Submitted for review',
  resubmitted: 'Resubmitted',
  claimed: 'A reviewer picked this up',
  released: 'Returned to the queue',
  comment: 'Note',
  changes_requested: 'Changes requested',
  approved: 'Approved',
  rejected: 'Rejected',
  published: 'Published',
  publish_failed: 'Publishing failed',
  withdrawn: 'Withdrawn',
  item_decision: 'Item decision',
};

export function eventLabel(e: SubmissionEvent): string {
  if (e.kind === 'item_decision') {
    const st = typeof e.data.state === 'string' ? e.data.state : '';
    return st ? `Item ${st}` : EVENT_LABEL.item_decision!;
  }
  return EVENT_LABEL[e.kind] ?? e.kind.replace(/_/g, ' ');
}

export function Timeline({
  events,
  actorLabel,
}: {
  events: SubmissionEvent[];
  /** Who an actor id is, for the viewer: "You", "Reviewer", an id. */
  actorLabel: (id: string | null) => string;
}) {
  if (events.length === 0) return <p className="rv-muted">Nothing has happened yet.</p>;
  return (
    <ol className="rv-timeline" aria-label="Timeline">
      {events.map((e) => (
        <li key={e.id} className={`rv-event rv-event-${e.kind}`}>
          <div className="rv-event-head">
            <strong>{eventLabel(e)}</strong>
            {e.internal && <Badge variant="warning">Internal</Badge>}
            <span className="rv-muted">
              {actorLabel(e.actor_id)} · {fmtWhen(e.created_at_ms)}
            </span>
          </div>
          {e.body && <p className="rv-event-body">{e.body}</p>}
        </li>
      ))}
    </ol>
  );
}

// ---- preview ----

const num = (n: number) => (Number.isInteger(n) ? String(n) : n.toFixed(2).replace(/0$/, ''));
export const chapterName = (c: { number: number; title: string | null }) =>
  c.title ? `Chapter ${num(c.number)}: ${c.title}` : `Chapter ${num(c.number)}`;

const list = (v: string[]) => (v.length ? v.join(', ') : 'None');

/** Field label and display value of the series facts, in the order a reviewer reads them. */
export function seriesFacts(p: LibraryPreview['series']): [string, string][] {
  return [
    ['Title', p.title],
    ['Alternative titles', list(p.alt_titles)],
    ['Type', p.kind],
    ['Authors', list(p.authors)],
    ['Artists', list(p.artists)],
    ['Status', p.status],
    ['Year', p.year ? String(p.year) : 'Unknown'],
    ['Genres', list(p.genres)],
    ['Tags', list(p.tags)],
    ['Reading direction', p.reading_direction],
    ['Age-restricted (18+)', p.age_restricted ? 'Yes' : 'No'],
    ['Synopsis', p.synopsis || 'None'],
  ];
}

export function PreviewFacts({ preview }: { preview: LibraryPreview }) {
  return (
    <dl className="rv-kv" aria-label="Series details">
      {seriesFacts(preview.series).map(([k, v]) => (
        <div key={k} className="rv-kv-row">
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      ))}
    </dl>
  );
}

// ---- diff between two submits ----

export interface Change {
  field: string;
  before: string;
  after: string;
}

/** Fields that differ between two previews: series facts and the chapter list. */
export function previewDiff(prev: LibraryPreview, next: LibraryPreview): Change[] {
  const out: Change[] = [];
  const a = new Map(seriesFacts(prev.series));
  for (const [k, v] of seriesFacts(next.series)) {
    const before = a.get(k) ?? '';
    if (before !== v) out.push({ field: k, before, after: v });
  }
  const pc = new Map(prev.chapters.map((c) => [c.id, c]));
  const nc = new Map(next.chapters.map((c) => [c.id, c]));
  for (const [id, c] of nc) {
    const old = pc.get(id);
    if (!old) {
      out.push({ field: chapterName(c), before: 'Not there', after: `${c.page_count} pages` });
    } else if (
      old.page_count !== c.page_count ||
      old.title !== c.title ||
      old.number !== c.number
    ) {
      out.push({
        field: chapterName(c),
        before: `${old.page_count} pages`,
        after: `${c.page_count} pages`,
      });
    }
  }
  for (const [id, c] of pc) {
    if (!nc.has(id))
      out.push({ field: chapterName(c), before: `${c.page_count} pages`, after: 'Removed' });
  }
  return out;
}

/** The preview stored with the last two submits of a submission, newest last. */
export function snapshots(events: SubmissionEvent[]): LibraryPreview[] {
  return events
    .filter((e) => (e.kind === 'submitted' || e.kind === 'resubmitted') && e.data.snapshot)
    .map((e) => e.data.snapshot as LibraryPreview);
}

export function ChangeList({ changes }: { changes: Change[] }) {
  if (changes.length === 0)
    return <p className="rv-muted">Nothing changed since the previous submit.</p>;
  return (
    <ul className="rv-changes" aria-label="Changes since the previous submit">
      {changes.map((c) => (
        <li key={`${c.field}:${c.after}`}>
          <strong>{c.field}</strong>
          <span className="rv-before">{c.before || 'Empty'}</span>
          <span aria-hidden="true">→</span>
          <span className="sr-only-label">changed to</span>
          <span className="rv-after">{c.after || 'Empty'}</span>
        </li>
      ))}
    </ul>
  );
}
