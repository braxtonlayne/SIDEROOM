// Pieces shared by the reviewer workspace (Admin) and the submitter view (Library): state chips,
// the timeline, the Library preview and the "what changed since the last submit" diff.
import { Badge, type BadgeVariant } from '@sideroom/ui';
import { useEffect } from 'react';
import { formatDateTime, t, type MessageKey, useT } from '../i18n';
import { onDispatch } from '../store/events';
import type { ItemState, LibraryPreview, SubmissionEvent, SubmissionState } from './types';
import './review.css';

// Getters, so the text is resolved when it is read (the locale is not known at module load).
export const STATE_LABEL: Record<SubmissionState, string> = {
  get draft() {
    return t('review.shared.stateDraft');
  },
  get submitted() {
    return t('review.shared.stateSubmitted');
  },
  get in_review() {
    return t('review.shared.stateInReview');
  },
  get changes_requested() {
    return t('review.shared.stateChangesRequested');
  },
  get approved() {
    return t('review.shared.stateApproved');
  },
  get rejected() {
    return t('review.shared.stateRejected');
  },
  get published() {
    return t('review.shared.statePublished');
  },
  get withdrawn() {
    return t('review.shared.stateWithdrawn');
  },
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
  useT();
  return <Badge variant={STATE_VARIANT[state]}>{STATE_LABEL[state]}</Badge>;
}

const ITEM_VARIANT: Record<ItemState, BadgeVariant> = {
  pending: 'neutral',
  approved: 'success',
  rejected: 'danger',
  published: 'success',
};
const ITEM_LABEL: Record<ItemState, MessageKey> = {
  pending: 'review.shared.itemPending',
  approved: 'review.shared.stateApproved',
  rejected: 'review.shared.stateRejected',
  published: 'review.shared.statePublished',
};
export function ItemChip({ state }: { state: ItemState }) {
  const t = useT();
  return <Badge variant={ITEM_VARIANT[state]}>{t(ITEM_LABEL[state])}</Badge>;
}

export const fmtWhen = (ms: number | null | undefined) =>
  ms ? formatDateTime(ms, { dateStyle: 'medium', timeStyle: 'short' }) : '';

export function ago(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return t('review.shared.justNow');
  const m = Math.round(s / 60);
  if (m < 60) return t('review.shared.minutesAgo', { n: m });
  const h = Math.round(m / 60);
  if (h < 48) return t('review.shared.hoursAgo', { n: h });
  return t('review.shared.daysAgo', { n: Math.round(h / 24) });
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

const EVENT_LABEL: Record<string, MessageKey> = {
  created: 'review.shared.eventCreated',
  edited: 'review.shared.eventEdited',
  submitted: 'review.shared.eventSubmitted',
  resubmitted: 'review.shared.eventResubmitted',
  claimed: 'review.shared.eventClaimed',
  released: 'review.shared.eventReleased',
  comment: 'review.shared.eventComment',
  changes_requested: 'review.shared.stateChangesRequested',
  approved: 'review.shared.stateApproved',
  rejected: 'review.shared.stateRejected',
  published: 'review.shared.statePublished',
  publish_failed: 'review.shared.eventPublishFailed',
  withdrawn: 'review.shared.stateWithdrawn',
  item_decision: 'review.shared.eventItemDecision',
};

export function eventLabel(e: SubmissionEvent): string {
  if (e.kind === 'item_decision') {
    const st = typeof e.data.state === 'string' ? e.data.state : '';
    return st
      ? t('review.shared.eventItemState', { state: st })
      : t('review.shared.eventItemDecision');
  }
  const key = EVENT_LABEL[e.kind];
  return key ? t(key) : e.kind.replace(/_/g, ' ');
}

export function Timeline({
  events,
  actorLabel,
}: {
  events: SubmissionEvent[];
  /** Who an actor id is, for the viewer: "You", "Reviewer", an id. */
  actorLabel: (id: string | null) => string;
}) {
  const t = useT();
  if (events.length === 0) return <p className="rv-muted">{t('review.shared.noEvents')}</p>;
  return (
    <ol className="rv-timeline" aria-label={t('review.shared.timeline')}>
      {events.map((e) => (
        <li key={e.id} className={`rv-event rv-event-${e.kind}`}>
          <div className="rv-event-head">
            <strong>{eventLabel(e)}</strong>
            {e.internal && <Badge variant="warning">{t('review.shared.internal')}</Badge>}
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
  c.title
    ? t('review.shared.chapterTitled', { number: num(c.number), title: c.title })
    : t('review.shared.chapter', { number: num(c.number) });

const list = (v: string[]) => (v.length ? v.join(', ') : t('review.shared.none'));

/** Field label and display value of the series facts, in the order a reviewer reads them. */
export function seriesFacts(p: LibraryPreview['series']): [string, string][] {
  return [
    [t('review.shared.factTitle'), p.title],
    [t('review.shared.factAltTitles'), list(p.alt_titles)],
    [t('review.shared.factType'), p.kind],
    [t('review.shared.factAuthors'), list(p.authors)],
    [t('review.shared.factArtists'), list(p.artists)],
    [t('review.shared.factStatus'), p.status],
    [t('review.shared.factYear'), p.year ? String(p.year) : t('review.shared.unknown')],
    [t('review.shared.factGenres'), list(p.genres)],
    [t('review.shared.factTags'), list(p.tags)],
    [t('review.shared.factReadingDirection'), p.reading_direction],
    [
      t('review.shared.factAgeRestricted'),
      p.age_restricted ? t('review.shared.yes') : t('review.shared.no'),
    ],
    [t('review.shared.factSynopsis'), p.synopsis || t('review.shared.none')],
  ];
}

export function PreviewFacts({ preview }: { preview: LibraryPreview }) {
  const t = useT();
  return (
    <dl className="rv-kv" aria-label={t('review.shared.seriesDetails')}>
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
      out.push({
        field: chapterName(c),
        before: t('review.shared.notThere'),
        after: t('review.shared.pages', { count: c.page_count }),
      });
    } else if (
      old.page_count !== c.page_count ||
      old.title !== c.title ||
      old.number !== c.number
    ) {
      out.push({
        field: chapterName(c),
        before: t('review.shared.pages', { count: old.page_count }),
        after: t('review.shared.pages', { count: c.page_count }),
      });
    }
  }
  for (const [id, c] of pc) {
    if (!nc.has(id))
      out.push({
        field: chapterName(c),
        before: t('review.shared.pages', { count: c.page_count }),
        after: t('review.shared.removed'),
      });
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
  const t = useT();
  if (changes.length === 0) return <p className="rv-muted">{t('review.shared.noChanges')}</p>;
  return (
    <ul className="rv-changes" aria-label={t('review.shared.changesList')}>
      {changes.map((c) => (
        <li key={`${c.field}:${c.after}`}>
          <strong>{c.field}</strong>
          <span className="rv-before">{c.before || t('review.shared.empty')}</span>
          <span aria-hidden="true">→</span>
          <span className="sr-only-label">{t('review.shared.changedTo')}</span>
          <span className="rv-after">{c.after || t('review.shared.empty')}</span>
        </li>
      ))}
    </ul>
  );
}
