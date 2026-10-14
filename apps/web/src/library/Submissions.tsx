// "Submissions" in the Library sidebar (REV-MY): the member's own submissions with their review
// states, a form to start one (a new series, or new chapters for a series that exists), and the
// submission page: chapters and uploads, reviewer notes, submit / resubmit / withdraw.
import {
  Button,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  Notice,
  Select,
  Spinner,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import * as rv from '../review/api';
import { errorMessages } from '../review/api';
import {
  ItemChip,
  PreviewFacts,
  STATE_LABEL,
  StateChip,
  Timeline,
  fmtWhen,
  useSubmissionUpdates,
} from '../review/shared';
import type {
  ChapterFormat,
  DraftChapter,
  SubmissionDetail,
  SubmissionState,
  Upload,
} from '../review/types';
import { KIND_LABEL } from './ui';
import type { SeriesKind } from './types';

const key = {
  list: (state: string) => ['submissions', 'mine', state] as const,
  detail: (id: string) => ['submissions', 'detail', id] as const,
  preview: (id: string) => ['submissions', 'preview', id] as const,
  chapters: (id: string) => ['submissions', 'chapters', id] as const,
  uploads: (id: string, chapter: string) => ['submissions', 'uploads', id, chapter] as const,
};

const csv = (s: string) =>
  s
    .split(',')
    .map((x) => x.trim())
    .filter(Boolean);

const FILTERS = [
  { value: '', label: 'Every state' },
  { value: 'draft', label: 'Drafts' },
  { value: 'submitted,in_review', label: 'Waiting for review' },
  { value: 'changes_requested', label: 'Changes requested' },
  { value: 'approved,published', label: 'Published' },
  { value: 'rejected,withdrawn', label: 'Closed' },
];

const FORMATS = [
  { value: 'images', label: 'Images (CBZ, CB7, CBR archive)' },
  { value: 'epub', label: 'EPUB' },
  { value: 'pdf', label: 'PDF' },
];

function useLiveRefresh() {
  const qc = useQueryClient();
  useSubmissionUpdates(() => void qc.invalidateQueries({ queryKey: ['submissions'] }));
}

function Problems({ messages }: { messages: string[] }) {
  if (messages.length === 0) return null;
  return (
    <Notice variant="danger" role="alert">
      {messages.length === 1 ? (
        messages[0]
      ) : (
        <ul style={{ margin: 0, paddingLeft: 18 }}>
          {messages.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}
    </Notice>
  );
}

// ---- list ----

export function SubmissionsPage({ series }: { series?: string | undefined }) {
  useLiveRefresh();
  const [filter, setFilter] = useState('');
  const [creating, setCreating] = useState(!!series);
  const list = useQuery({
    queryKey: key.list(filter),
    queryFn: () =>
      rv.listMine({
        sort: 'updated',
        order: 'desc',
        limit: 50,
        ...(filter ? { state: filter } : {}),
      }),
  });
  return (
    <div>
      <div className="lib-page-head">
        <h1 className="lib-h1">Submissions</h1>
        <div className="rv-row">
          <Select
            aria-label="Filter submissions"
            options={FILTERS}
            value={filter}
            onValueChange={setFilter}
          />
          <Button className="lib-btn lib-btn-gold" onClick={() => setCreating((c) => !c)}>
            {creating ? 'Close the form' : 'New submission'}
          </Button>
        </div>
      </div>
      {creating && <NewSubmission {...(series ? { series } : {})} />}
      {list.isPending && <Spinner label="Loading your submissions" />}
      {list.isError && (
        <Notice variant="danger" role="alert">
          Could not load your submissions.
        </Notice>
      )}
      {list.data && list.data.items.length === 0 && (
        <EmptyState
          title="No submissions yet"
          description="Add a series or new chapters. A curator reviews every submission before it appears in the library."
        />
      )}
      {list.data && list.data.items.length > 0 && (
        <ul
          className="rv-stack"
          style={{ listStyle: 'none', padding: 0 }}
          aria-label="My submissions"
        >
          {list.data.items.map((s) => (
            <li key={s.id} className="rv-panel" style={{ marginBottom: 0 }}>
              <div className="rv-head" style={{ marginBottom: 4 }}>
                <h2 style={{ fontSize: '1.125rem' }}>
                  <Link to="/library/submissions/$submissionId" params={{ submissionId: s.id }}>
                    {s.title}
                  </Link>
                </h2>
                <StateChip state={s.state} />
              </div>
              <p className="rv-muted" style={{ margin: 0 }}>
                {s.kind === 'library.items' ? 'New chapters' : 'New series'} · updated{' '}
                {fmtWhen(s.updated_at_ms)}
              </p>
              {s.state === 'changes_requested' && s.decision_reason && (
                <p style={{ margin: '8px 0 0' }}>
                  <strong>Reviewer note:</strong> {s.decision_reason}
                </p>
              )}
              {s.state === 'rejected' && s.decision_reason && (
                <p style={{ margin: '8px 0 0' }}>
                  <strong>Why:</strong> {s.decision_reason}
                </p>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ---- create ----

const SERIES_KINDS = (Object.keys(KIND_LABEL) as SeriesKind[]).map((k) => ({
  value: k,
  label: KIND_LABEL[k],
}));

function NewSubmission({ series }: { series?: string }) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [kind, setKind] = useState(series ? 'library.items' : 'library.series');
  const [title, setTitle] = useState('');
  const [seriesKind, setSeriesKind] = useState<SeriesKind>('manga');
  const [authors, setAuthors] = useState('');
  const [genres, setGenres] = useState('');
  const [synopsis, setSynopsis] = useState('');
  const [restricted, setRestricted] = useState(false);
  const [seriesId, setSeriesId] = useState(series ?? '');
  const create = useMutation({
    mutationFn: () =>
      rv.createSubmission({
        kind,
        payload:
          kind === 'library.series'
            ? {
                kind: seriesKind,
                title: title.trim(),
                authors: csv(authors),
                genres: csv(genres),
                synopsis: synopsis.trim(),
                age_restricted: restricted,
              }
            : { series_id: seriesId.trim() },
      }),
    onSuccess: (d) => {
      void qc.invalidateQueries({ queryKey: ['submissions'] });
      void navigate({
        to: '/library/submissions/$submissionId',
        params: { submissionId: d.submission.id },
      });
    },
  });
  const ready = kind === 'library.series' ? title.trim().length > 0 : /^\d+$/.test(seriesId.trim());
  return (
    <form
      className="rv-panel rv-form"
      aria-label="New submission"
      onSubmit={(e) => {
        e.preventDefault();
        if (ready) create.mutate();
      }}
    >
      <Select
        aria-label="What are you submitting"
        options={[
          { value: 'library.series', label: 'A new series' },
          { value: 'library.items', label: 'New chapters for a series that is already here' },
        ]}
        value={kind}
        onValueChange={setKind}
      />
      {kind === 'library.series' ? (
        <>
          <TextInput
            label="Title"
            value={title}
            maxLength={200}
            required
            onChange={(e) => setTitle(e.target.value)}
          />
          <Select
            aria-label="Type"
            options={SERIES_KINDS}
            value={seriesKind}
            onValueChange={(v) => setSeriesKind(v as SeriesKind)}
          />
          <TextInput
            label="Authors"
            hint="Separate with commas"
            value={authors}
            onChange={(e) => setAuthors(e.target.value)}
          />
          <TextInput
            label="Genres"
            hint="Separate with commas"
            value={genres}
            onChange={(e) => setGenres(e.target.value)}
          />
          <TextArea
            label="Synopsis"
            value={synopsis}
            rows={4}
            onChange={(e) => setSynopsis(e.target.value)}
          />
          <Checkbox
            label="Age-restricted (18+)"
            checked={restricted}
            onChange={(e) => setRestricted(e.target.checked)}
          />
        </>
      ) : (
        <TextInput
          label="Series id"
          hint="The number at the end of the series page address"
          inputMode="numeric"
          value={seriesId}
          onChange={(e) => setSeriesId(e.target.value)}
        />
      )}
      {create.isError && <Problems messages={errorMessages(create.error)} />}
      <div>
        <Button type="submit" disabled={!ready} loading={create.isPending}>
          Start the draft
        </Button>
      </div>
    </form>
  );
}

// ---- detail ----

const FINAL: SubmissionState[] = ['rejected', 'published', 'withdrawn'];
const EDITABLE: SubmissionState[] = ['draft', 'changes_requested'];

export function SubmissionPage({ id }: { id: string }) {
  useLiveRefresh();
  const qc = useQueryClient();
  const detail = useQuery({ queryKey: key.detail(id), queryFn: () => rv.getMine(id) });
  const [confirm, setConfirm] = useState<'withdraw' | null>(null);
  const put = (d: SubmissionDetail) => {
    qc.setQueryData(key.detail(id), d);
    void qc.invalidateQueries({ queryKey: ['submissions', 'mine'] });
    void qc.invalidateQueries({ queryKey: key.preview(id) });
  };
  const submit = useMutation({ mutationFn: () => rv.submit(id), onSuccess: put });
  const withdraw = useMutation({
    mutationFn: () => rv.withdraw(id),
    onSuccess: (d) => {
      put(d);
      setConfirm(null);
    },
  });
  const comment = useMutation({
    mutationFn: (body: string) => rv.commentMine(id, body),
    onSuccess: put,
  });

  if (detail.isPending) return <Spinner label="Loading the submission" />;
  if (detail.isError || !detail.data)
    return (
      <EmptyState
        title="Submission not found"
        description="It may have been removed, or it is not yours."
        action={
          <Link to="/library/submissions" className="lib-btn">
            Back to submissions
          </Link>
        }
      />
    );
  const d = detail.data;
  const s = d.submission;
  const editable = EDITABLE.includes(s.state);
  const library = s.kind.startsWith('library.');
  const lastChange = d.events.filter((e) => e.kind === 'changes_requested').at(-1);

  return (
    <div>
      <p style={{ margin: '0 0 12px' }}>
        <Link to="/library/submissions" className="lib-link">
          All submissions
        </Link>
      </p>
      <div className="rv-head">
        <h1 className="lib-h1" style={{ fontSize: '1.75rem' }}>
          {s.title}
        </h1>
        <StateChip state={s.state} />
      </div>
      <p className="rv-muted">
        {d.kind_label} · created {fmtWhen(s.created_at_ms)}
        {s.submitted_at_ms ? ` · submitted ${fmtWhen(s.submitted_at_ms)}` : ''}
      </p>

      {s.state === 'changes_requested' && (
        <div className="rv-banner" role="status">
          <strong>A reviewer asked for changes</strong>
          <p>{lastChange?.body ?? s.decision_reason}</p>
          <p className="rv-muted">Fix it below, then send it for review again.</p>
        </div>
      )}
      {s.state === 'rejected' && (
        <Notice variant="danger" title="Rejected">
          {s.decision_reason}
        </Notice>
      )}
      {s.state === 'published' && (
        <Notice variant="success" title="Published">
          This is live in the library. Thank you!
        </Notice>
      )}
      {(s.state === 'submitted' || s.state === 'in_review' || s.state === 'approved') && (
        <Notice variant="info" title={STATE_LABEL[s.state]}>
          {s.state === 'approved'
            ? 'Approved. It is being published.'
            : 'A curator will look at it. You will hear back here.'}
        </Notice>
      )}

      {editable && <DetailsForm detail={d} onSaved={put} />}
      {library && <ChaptersPanel detail={d} editable={editable} />}
      {library && s.kind === 'library.series' && editable && <SeriesPreviewPanel id={id} />}

      <div className="rv-actions" style={{ margin: '16px 0' }}>
        {editable && (
          <Button
            className="lib-btn lib-btn-gold"
            loading={submit.isPending}
            onClick={() => submit.mutate()}
          >
            {s.state === 'changes_requested' ? 'Send for review again' : 'Submit for review'}
          </Button>
        )}
        {!FINAL.includes(s.state) && s.state !== 'approved' && (
          <Button variant="secondary" onClick={() => setConfirm('withdraw')}>
            Withdraw
          </Button>
        )}
      </div>
      {submit.isError && <Problems messages={errorMessages(submit.error)} />}
      {withdraw.isError && <Problems messages={errorMessages(withdraw.error)} />}

      <section className="rv-panel" aria-labelledby="sub-time-h">
        <h2 id="sub-time-h">Timeline</h2>
        <Timeline
          events={d.events}
          actorLabel={(a) => (a === s.submitter_id ? 'You' : a ? 'A reviewer' : 'Someone')}
        />
        {!FINAL.includes(s.state) && (
          <CommentForm busy={comment.isPending} onSend={(b) => comment.mutateAsync(b)} />
        )}
        {comment.isError && <Problems messages={errorMessages(comment.error)} />}
      </section>

      <ConfirmDialog
        open={confirm === 'withdraw'}
        title="Withdraw this submission"
        description="Reviewers stop looking at it. Your draft stays unpublished."
        confirmLabel="Withdraw"
        danger
        loading={withdraw.isPending}
        onCancel={() => setConfirm(null)}
        onConfirm={() => withdraw.mutate()}
      />
    </div>
  );
}

function DetailsForm({
  detail,
  onSaved,
}: {
  detail: SubmissionDetail;
  onSaved: (d: SubmissionDetail) => void;
}) {
  const s = detail.submission;
  const [title, setTitle] = useState(s.title);
  const [summary, setSummary] = useState(s.summary);
  const save = useMutation({
    mutationFn: () => rv.updateMine(s.id, { title: title.trim(), summary: summary.trim() }),
    onSuccess: onSaved,
  });
  const dirty = title.trim() !== s.title || summary.trim() !== s.summary;
  return (
    <form
      className="rv-panel rv-form"
      aria-label="Submission details"
      onSubmit={(e) => {
        e.preventDefault();
        if (dirty && title.trim()) save.mutate();
      }}
    >
      <TextInput
        label="Submission title"
        value={title}
        maxLength={200}
        onChange={(e) => setTitle(e.target.value)}
      />
      <TextArea
        label="Note for the reviewers"
        hint="Where the files come from, anything they should know"
        value={summary}
        rows={3}
        maxLength={4000}
        onChange={(e) => setSummary(e.target.value)}
      />
      {save.isError && <Problems messages={errorMessages(save.error)} />}
      <div>
        <Button
          type="submit"
          variant="secondary"
          disabled={!dirty || !title.trim()}
          loading={save.isPending}
        >
          Save details
        </Button>
      </div>
    </form>
  );
}

function SeriesPreviewPanel({ id }: { id: string }) {
  const p = useQuery({ queryKey: key.preview(id), queryFn: () => rv.getMinePreview(id) });
  if (!p.data) return null;
  return (
    <section className="rv-panel" aria-labelledby="sub-meta-h">
      <h2 id="sub-meta-h">What reviewers will see</h2>
      <PreviewFacts preview={p.data.preview} />
    </section>
  );
}

function CommentForm({
  busy,
  onSend,
}: {
  busy: boolean;
  onSend: (body: string) => Promise<unknown>;
}) {
  const [body, setBody] = useState('');
  return (
    <form
      className="rv-form"
      onSubmit={(e) => {
        e.preventDefault();
        const t = body.trim();
        if (t) void onSend(t).then(() => setBody(''));
      }}
    >
      <TextArea
        label="Write to the reviewers"
        value={body}
        rows={3}
        maxLength={4000}
        onChange={(e) => setBody(e.target.value)}
      />
      <div>
        <Button type="submit" variant="secondary" disabled={!body.trim()} loading={busy}>
          Send note
        </Button>
      </div>
    </form>
  );
}

// ---- chapters and uploads ----

function ChaptersPanel({ detail, editable }: { detail: SubmissionDetail; editable: boolean }) {
  const s = detail.submission;
  const qc = useQueryClient();
  const chapters = useQuery({
    queryKey: key.chapters(s.id),
    queryFn: () => rv.draftChapters(s.id),
  });
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: key.chapters(s.id) });
    void qc.invalidateQueries({ queryKey: key.detail(s.id) });
    void qc.invalidateQueries({ queryKey: key.preview(s.id) });
  };
  const itemOf = new Map(detail.items.map((i) => [i.ref_id, i]));
  const rows = (chapters.data?.chapters ?? [])
    .filter((c) =>
      s.kind === 'library.items' ? itemOf.has(c.id) : !c.published || itemOf.has(c.id),
    )
    .sort((a, b) => a.number - b.number);
  const del = useMutation({
    mutationFn: (c: DraftChapter) => rv.deleteDraftChapter(s.id, c.id),
    onSuccess: refresh,
  });
  return (
    <section className="rv-panel" aria-labelledby="sub-ch-h">
      <h2 id="sub-ch-h">Chapters</h2>
      {chapters.isPending && <Spinner label="Loading the chapters" />}
      {chapters.data && rows.length === 0 && (
        <p className="rv-muted">
          No chapters yet. Add one, then upload its file (CBZ, CB7, CBR, EPUB or PDF).
        </p>
      )}
      {rows.length > 0 && (
        <div className="rv-table-wrap">
          <table className="rv-table" aria-label="Chapters of this submission">
            <thead>
              <tr>
                <th scope="col">Chapter</th>
                <th scope="col">File</th>
                <th scope="col">Review</th>
                <th scope="col">
                  <span className="sr-only-label">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => {
                const item = itemOf.get(c.id);
                const name = c.title ? `Chapter ${c.number}: ${c.title}` : `Chapter ${c.number}`;
                return (
                  <tr key={c.id}>
                    <td>
                      <strong>{name}</strong>
                    </td>
                    <td>
                      <UploadCell
                        submissionId={s.id}
                        chapter={c}
                        editable={editable}
                        onDone={refresh}
                      />
                    </td>
                    <td>
                      {item ? (
                        <ItemChip state={item.state} />
                      ) : (
                        <span className="rv-muted">Not submitted</span>
                      )}
                      {item?.note && <div>{item.note}</div>}
                    </td>
                    <td>
                      {editable && (
                        <Button
                          size="sm"
                          variant="ghost"
                          aria-label={`Remove ${name}`}
                          disabled={del.isPending}
                          onClick={() => del.mutate(c)}
                        >
                          Remove
                        </Button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {del.isError && <Problems messages={errorMessages(del.error)} />}
      {editable && <AddChapter submissionId={s.id} onAdded={refresh} />}
    </section>
  );
}

function AddChapter({ submissionId, onAdded }: { submissionId: string; onAdded: () => void }) {
  const [number, setNumber] = useState('');
  const [title, setTitle] = useState('');
  const [format, setFormat] = useState<ChapterFormat>('images');
  const add = useMutation({
    mutationFn: () =>
      rv.createDraftChapter(submissionId, {
        number: Number(number),
        title: title.trim() || null,
        format,
      }),
    onSuccess: () => {
      setNumber('');
      setTitle('');
      onAdded();
    },
  });
  const ok = number.trim() !== '' && Number.isFinite(Number(number)) && Number(number) >= 0;
  return (
    <form
      className="rv-form"
      style={{ marginTop: 16 }}
      aria-label="Add a chapter"
      onSubmit={(e) => {
        e.preventDefault();
        if (ok) add.mutate();
      }}
    >
      <TextInput
        label="Chapter number"
        inputMode="decimal"
        value={number}
        onChange={(e) => setNumber(e.target.value)}
      />
      <TextInput
        label="Chapter title (optional)"
        value={title}
        maxLength={200}
        onChange={(e) => setTitle(e.target.value)}
      />
      <Select
        aria-label="File type"
        options={FORMATS}
        value={format}
        onValueChange={(v) => setFormat(v as ChapterFormat)}
      />
      {add.isError && <Problems messages={errorMessages(add.error)} />}
      <div>
        <Button type="submit" variant="secondary" disabled={!ok} loading={add.isPending}>
          Add chapter
        </Button>
      </div>
    </form>
  );
}

const UPLOAD_TEXT: Record<string, string> = {
  awaiting_upload: 'Waiting for the file',
  queued: 'Queued for conversion',
  running: 'Converting',
  done: 'Ready',
  failed: 'Failed',
  expired: 'Expired',
};

/** Upload state of one chapter: the file picker (while editable) and the latest upload's progress. */
function UploadCell({
  submissionId,
  chapter,
  editable,
  onDone,
}: {
  submissionId: string;
  chapter: DraftChapter;
  editable: boolean;
  onDone: () => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string[]>([]);
  const [sending, setSending] = useState(false);
  const accept =
    chapter.format === 'epub'
      ? '.epub'
      : chapter.format === 'pdf'
        ? '.pdf'
        : '.cbz,.zip,.cb7,.7z,.cbr,.rar';
  const uploads = useQuery({
    queryKey: key.uploads(submissionId, chapter.id),
    queryFn: () => rv.chapterUploads(submissionId, chapter.id),
    // Poll while the conversion runs.
    refetchInterval: (q) => {
      const u = q.state.data?.uploads[0];
      return u && ['awaiting_upload', 'queued', 'running'].includes(u.status) ? 1500 : false;
    },
  });
  const latest: Upload | undefined = uploads.data?.uploads[0];
  const finished = useRef<string | null>(null);
  const latestId = latest?.id;
  const latestStatus = latest?.status;
  useEffect(() => {
    if (latestStatus === 'done' && latestId && finished.current !== latestId) {
      finished.current = latestId;
      onDone();
    }
  }, [latestId, latestStatus, onDone]);

  const send = async (file: File) => {
    setError([]);
    setSending(true);
    try {
      const slot = await rv.beginUpload(submissionId, chapter.id, {
        filename: file.name,
        size_bytes: file.size,
      });
      await rv.putFile(slot, file);
      await rv.completeUpload(submissionId, slot.upload.id);
      await uploads.refetch();
    } catch (e) {
      setError(errorMessages(e, 'The upload failed.'));
    } finally {
      setSending(false);
      if (input.current) input.current.value = '';
    }
  };

  const busy =
    sending || (!!latest && ['awaiting_upload', 'queued', 'running'].includes(latest.status));
  return (
    <div className="rv-stack" style={{ gap: 4 }}>
      <span>
        {chapter.page_count > 0
          ? `${chapter.page_count} pages`
          : chapter.format === 'images'
            ? 'No pages yet'
            : 'No file yet'}
        {latest && (
          <span className="rv-muted" role="status">
            {' '}
            · {UPLOAD_TEXT[latest.status] ?? latest.status}
          </span>
        )}
      </span>
      {latest?.status === 'failed' && latest.error && <Problems messages={[latest.error]} />}
      {latest?.warnings.map((w) => (
        <span key={w} className="rv-muted">
          {w}
        </span>
      ))}
      <Problems messages={error} />
      {editable && (
        <label className="rv-file">
          <span className="sr-only-label">Upload a file for chapter {chapter.number}</span>
          <input
            ref={input}
            type="file"
            accept={accept}
            disabled={busy}
            aria-label={`Choose a file for chapter ${chapter.number}`}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void send(f);
            }}
          />
        </label>
      )}
      {busy && <Spinner label="Uploading" />}
    </div>
  );
}
