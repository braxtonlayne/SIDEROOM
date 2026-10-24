// Wire types of the submissions and review API (crates/sr-review, crates/sr-api routes/review).
// Ids are snowflake strings, timestamps are Unix milliseconds.

export type SubmissionState =
  | 'draft'
  | 'submitted'
  | 'in_review'
  | 'changes_requested'
  | 'approved'
  | 'rejected'
  | 'published'
  | 'withdrawn';

export type ItemState = 'pending' | 'approved' | 'rejected' | 'published';

export interface Submission {
  id: string;
  kind: string;
  submitter_id: string | null;
  state: SubmissionState;
  title: string;
  summary: string;
  ref_id: string | null;
  payload: Record<string, unknown>;
  flags: string[];
  claimed_by: string | null;
  claim_expires_at_ms: number | null;
  decided_by: string | null;
  decision_reason: string | null;
  submitted_at_ms: number | null;
  decided_at_ms: number | null;
  published_at_ms: number | null;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface SubmissionItem {
  id: string;
  submission_id: string;
  ref_id: string;
  label: string;
  position: number;
  state: ItemState;
  note: string | null;
}

export interface SubmissionEvent {
  id: string;
  submission_id: string;
  kind: string;
  actor_id: string | null;
  body: string | null;
  internal: boolean;
  data: Record<string, unknown>;
  created_at_ms: number;
}

export interface SubmissionDetail {
  submission: Submission;
  items: SubmissionItem[];
  events: SubmissionEvent[];
  kind_label: string;
}

export interface SubmissionPage {
  items: Submission[];
  total: number;
  limit: number;
  offset: number;
}

export interface KindInfo {
  id: string;
  label: string;
}

/** The reviewer preview of a Cinema submission (`sr-cinema` `submissions.rs` `preview`). */
export interface CinemaPreview {
  title_id: string;
  new_title: boolean;
  title: {
    kind: string;
    title: string;
    original_title?: string | null;
    year?: number | null;
    runtime_min?: number | null;
    content_rating?: string | null;
    synopsis?: string | null;
    genres?: string[];
    languages?: string[];
  };
  seasons: { number: number; name: string | null }[];
  existing_episodes: number;
  episodes: {
    id: string;
    season: number;
    number: number;
    title: string | null;
    has_asset: boolean;
    published: boolean;
  }[];
}

/** True when a preview is a Library one (it has the series facts the Library panels read). */
export function isLibraryPreview(p: unknown): p is LibraryPreview {
  return typeof p === 'object' && p !== null && 'series' in p && 'chapters' in p;
}

/** What the Library kinds show a reviewer (crates/sr-library/src/submissions.rs `preview`). */
export interface LibraryPreview {
  series_id: string;
  new_series: boolean;
  series: {
    kind: string;
    title: string;
    alt_titles: string[];
    authors: string[];
    artists: string[];
    status: string;
    year: number | null;
    synopsis: string;
    genres: string[];
    tags: string[];
    reading_direction: string;
    age_restricted: boolean;
    has_cover: boolean;
  };
  editions: { id: string; language: string; label: string }[];
  existing_chapters: number;
  chapters: {
    id: string;
    edition_id: string;
    number: number;
    title: string | null;
    format: string;
    page_count: number;
    release_group: string | null;
    published: boolean;
  }[];
}

export interface SubmissionListParams {
  kind?: string;
  state?: string;
  claimed?: 'mine' | 'unclaimed';
  submitter?: string;
  q?: string;
  sort?: 'submitted' | 'updated' | 'title';
  order?: 'asc' | 'desc';
  min_age_hours?: number;
  limit?: number;
  offset?: number;
}

export interface CreateSubmission {
  kind: string;
  payload: Record<string, unknown>;
  title?: string;
  summary?: string;
}

// ---- Library draft content (the shapes the Library routes already use) ----

export type ChapterFormat = 'images' | 'epub' | 'pdf';

export interface DraftChapter {
  id: string;
  series_id: string;
  edition_id: string;
  /** The volume the chapter belongs to; null when it has none. */
  volume_id: string | null;
  number: number;
  title: string | null;
  page_count: number;
  format: ChapterFormat;
  release_group: string | null;
  published: boolean;
}

export interface DraftVolume {
  id: string;
  edition_id: string;
  number: number;
  title: string | null;
}

export interface DraftChapterList {
  chapters: DraftChapter[];
  /** The volumes of the series' editions (a new series has none). */
  volumes?: DraftVolume[];
}

export type UploadStatus = 'awaiting_upload' | 'queued' | 'running' | 'done' | 'failed' | 'expired';

export interface Upload {
  id: string;
  chapter_id: string;
  series_id: string;
  filename: string;
  kind: string;
  size_bytes: number;
  status: UploadStatus;
  error: string | null;
  warnings: string[];
  page_count: number | null;
  metadata: Record<string, unknown>;
  created_at_ms: number;
  updated_at_ms: number;
  finished_at_ms: number | null;
}

export interface UploadSlot {
  upload: Upload;
  upload_url: string;
  upload_method: string;
  expires_in_secs: number;
  max_bytes: number;
}

export interface ReviewPage {
  index: number;
  width: number;
  height: number;
  url: string;
}

export interface ReviewPageManifest {
  pages: ReviewPage[];
}

export interface ContentRequest {
  id: string;
  section: string;
  title: string;
  creator: string | null;
  year: number | null;
  link: string | null;
  note: string;
  status: 'open' | 'fulfilled' | 'declined';
  vote_count: number;
  voted: boolean;
  submission_id: string | null;
  created_at_ms: number;
  claimed_by: string | null;
  claim_note: string | null;
  claimed_by_me: boolean;
}

export interface RequestPage {
  items: ContentRequest[];
  total: number;
  limit: number;
  offset: number;
}

export interface NamingItem {
  name: string;
  cinema: {
    role: string;
    title: string;
    year: number | null;
    season: number | null;
    episodes: number[];
    episode_title: string | null;
    variant: string | null;
    language: string | null;
    warnings: string[];
  } | null;
  library: {
    role: string;
    series: string | null;
    volume: number | null;
    chapter: number | null;
    title: string | null;
    group: string | null;
    page: number | null;
    warnings: string[];
  } | null;
}
