import type {
  LibraryPreview,
  Submission,
  SubmissionDetail,
  SubmissionEvent,
  SubmissionItem,
} from '../types';

export const submission = (id: string, o: Partial<Submission> = {}): Submission => ({
  id,
  kind: 'library.series',
  submitter_id: '77',
  state: 'submitted',
  title: `Submission ${id}`,
  summary: '',
  ref_id: '900',
  payload: {},
  flags: [],
  claimed_by: null,
  claim_expires_at_ms: null,
  decided_by: null,
  decision_reason: null,
  submitted_at_ms: Date.now() - 3 * 3600_000,
  decided_at_ms: null,
  published_at_ms: null,
  created_at_ms: Date.now() - 86_400_000,
  updated_at_ms: Date.now() - 3600_000,
  ...o,
});

export const item = (n: number, o: Partial<SubmissionItem> = {}): SubmissionItem => ({
  id: `i${n}`,
  submission_id: '1',
  ref_id: `c${n}`,
  label: `Chapter ${n}`,
  position: n,
  state: 'pending',
  note: null,
  ...o,
});

export const event = (
  id: number,
  kind: string,
  o: Partial<SubmissionEvent> = {},
): SubmissionEvent => ({
  id: String(id),
  submission_id: '1',
  kind,
  actor_id: '77',
  body: null,
  internal: false,
  data: {},
  created_at_ms: 1_700_000_000_000 + id * 60_000,
  ...o,
});

export const preview = (o: Partial<LibraryPreview['series']> = {}, pages = 12): LibraryPreview => ({
  series_id: '900',
  new_series: true,
  series: {
    kind: 'manga',
    title: 'Dungeon Tales',
    alt_titles: [],
    authors: ['Kui'],
    artists: [],
    status: 'ongoing',
    year: 2014,
    synopsis: 'Delicious.',
    genres: ['fantasy'],
    tags: [],
    reading_direction: 'rtl',
    age_restricted: false,
    has_cover: false,
    ...o,
  },
  editions: [{ id: 'e1', language: 'en', label: 'Default' }],
  existing_chapters: 0,
  chapters: [1, 2].map((n) => ({
    id: `c${n}`,
    edition_id: 'e1',
    number: n,
    title: null,
    format: 'images',
    page_count: pages,
    release_group: null,
    published: false,
  })),
});

export const detail = (
  s: Partial<Submission> = {},
  items: SubmissionItem[] = [item(1), item(2)],
  events: SubmissionEvent[] = [
    event(1, 'created'),
    event(2, 'submitted', { data: { snapshot: preview() } }),
  ],
): SubmissionDetail => ({
  submission: submission('1', s),
  items,
  events,
  kind_label: 'Library series',
});
