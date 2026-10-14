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
import { useT, type MessageKey } from '../i18n';
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

const FILTERS: { value: string; labelKey: MessageKey }[] = [
  { value: '', labelKey: 'library.submissions.filter.all' },
  { value: 'draft', labelKey: 'library.submissions.filter.draft' },
  { value: 'submitted,in_review', labelKey: 'library.submissions.filter.waiting' },
  { value: 'changes_requested', labelKey: 'library.submissions.filter.changes' },
  { value: 'approved,published', labelKey: 'library.submissions.filter.published' },
  { value: 'rejected,withdrawn', labelKey: 'library.submissions.filter.closed' },
];

const FORMATS: { value: string; labelKey: MessageKey }[] = [
  { value: 'images', labelKey: 'library.submissions.format.images' },
  { value: 'epub', labelKey: 'library.submissions.format.epub' },
  { value: 'pdf', labelKey: 'library.submissions.format.pdf' },
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
  const t = useT();
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
        <h1 className="lib-h1">{t('library.submissions.title')}</h1>
        <div className="rv-row">
          <Select
            aria-label={t('library.submissions.filterLabel')}
            options={FILTERS.map((f) => ({ value: f.value, label: t(f.labelKey) }))}
            value={filter}
            onValueChange={setFilter}
          />
          <Button className="lib-btn lib-btn-gold" onClick={() => setCreating((c) => !c)}>
            {creating ? t('library.submissions.closeForm') : t('library.submissions.new')}
          </Button>
        </div>
      </div>
      {creating && <NewSubmission {...(series ? { series } : {})} />}
      {list.isPending && <Spinner label={t('library.submissions.loadingList')} />}
      {list.isError && (
        <Notice variant="danger" role="alert">
          {t('library.submissions.listError')}
        </Notice>
      )}
      {list.data && list.data.items.length === 0 && (
        <EmptyState
          title={t('library.submissions.emptyTitle')}
          description={t('library.submissions.emptyDescription')}
        />
      )}
      {list.data && list.data.items.length > 0 && (
        <ul
          className="rv-stack"
          style={{ listStyle: 'none', padding: 0 }}
          aria-label={t('library.submissions.listLabel')}
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
                {t('library.submissions.kindUpdated', {
                  kind:
                    s.kind === 'library.items'
                      ? t('library.submissions.kindChapters')
                      : t('library.submissions.kindSeries'),
                  when: fmtWhen(s.updated_at_ms),
                })}
              </p>
              {s.state === 'changes_requested' && s.decision_reason && (
                <p style={{ margin: '8px 0 0' }}>
                  <strong>{t('library.submissions.reviewerNote')}</strong> {s.decision_reason}
                </p>
              )}
              {s.state === 'rejected' && s.decision_reason && (
                <p style={{ margin: '8px 0 0' }}>
                  <strong>{t('library.submissions.why')}</strong> {s.decision_reason}
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

function NewSubmission({ series }: { series?: string }) {
  const t = useT();
  const seriesKinds = (Object.keys(KIND_LABEL) as SeriesKind[]).map((k) => ({
    value: k,
    label: KIND_LABEL[k],
  }));
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
      aria-label={t('library.submissions.new')}
      onSubmit={(e) => {
        e.preventDefault();
        if (ready) create.mutate();
      }}
    >
      <Select
        aria-label={t('library.submissions.form.what')}
        options={[
          { value: 'library.series', label: t('library.submissions.form.newSeries') },
          { value: 'library.items', label: t('library.submissions.form.newChapters') },
        ]}
        value={kind}
        onValueChange={setKind}
      />
      {kind === 'library.series' ? (
        <>
          <TextInput
            label={t('library.submissions.form.title')}
            value={title}
            maxLength={200}
            required
            onChange={(e) => setTitle(e.target.value)}
          />
          <Select
            aria-label={t('library.submissions.form.type')}
            options={seriesKinds}
            value={seriesKind}
            onValueChange={(v) => setSeriesKind(v as SeriesKind)}
          />
          <TextInput
            label={t('library.submissions.form.authors')}
            hint={t('library.submissions.form.commas')}
            value={authors}
            onChange={(e) => setAuthors(e.target.value)}
          />
          <TextInput
            label={t('library.submissions.form.genres')}
            hint={t('library.submissions.form.commas')}
            value={genres}
            onChange={(e) => setGenres(e.target.value)}
          />
          <TextArea
            label={t('library.submissions.form.synopsis')}
            value={synopsis}
            rows={4}
            onChange={(e) => setSynopsis(e.target.value)}
          />
          <Checkbox
            label={t('library.submissions.form.ageRestricted')}
            checked={restricted}
            onChange={(e) => setRestricted(e.target.checked)}
          />
        </>
      ) : (
        <TextInput
          label={t('library.submissions.form.seriesId')}
          hint={t('library.submissions.form.seriesIdHint')}
          inputMode="numeric"
          value={seriesId}
          onChange={(e) => setSeriesId(e.target.value)}
        />
      )}
      {create.isError && <Problems messages={errorMessages(create.error)} />}
      <div>
        <Button type="submit" disabled={!ready} loading={create.isPending}>
          {t('library.submissions.form.start')}
        </Button>
      </div>
    </form>
  );
}

// ---- detail ----

const FINAL: SubmissionState[] = ['rejected', 'published', 'withdrawn'];
const EDITABLE: SubmissionState[] = ['draft', 'changes_requested'];

export function SubmissionPage({ id }: { id: string }) {
  const t = useT();
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

  if (detail.isPending) return <Spinner label={t('library.submissions.loadingOne')} />;
  if (detail.isError || !detail.data)
    return (
      <EmptyState
        title={t('library.submissions.notFoundTitle')}
        description={t('library.submissions.notFoundDescription')}
        action={
          <Link to="/library/submissions" className="lib-btn">
            {t('library.submissions.back')}
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
          {t('library.submissions.all')}
        </Link>
      </p>
      <div className="rv-head">
        <h1 className="lib-h1" style={{ fontSize: '1.75rem' }}>
          {s.title}
        </h1>
        <StateChip state={s.state} />
      </div>
      <p className="rv-muted">
        {t('library.submissions.created', { kind: d.kind_label, when: fmtWhen(s.created_at_ms) })}
        {s.submitted_at_ms
          ? t('library.submissions.submittedSuffix', { when: fmtWhen(s.submitted_at_ms) })
          : ''}
      </p>

      {s.state === 'changes_requested' && (
        <div className="rv-banner" role="status">
          <strong>{t('library.submissions.changesTitle')}</strong>
          <p>{lastChange?.body ?? s.decision_reason}</p>
          <p className="rv-muted">{t('library.submissions.changesHint')}</p>
        </div>
      )}
      {s.state === 'rejected' && (
        <Notice variant="danger" title={t('library.submissions.rejected')}>
          {s.decision_reason}
        </Notice>
      )}
      {s.state === 'published' && (
        <Notice variant="success" title={t('library.submissions.published')}>
          {t('library.submissions.publishedBody')}
        </Notice>
      )}
      {(s.state === 'submitted' || s.state === 'in_review' || s.state === 'approved') && (
        <Notice variant="info" title={STATE_LABEL[s.state]}>
          {s.state === 'approved'
            ? t('library.submissions.approvedBody')
            : t('library.submissions.pendingBody')}
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
            {s.state === 'changes_requested'
              ? t('library.submissions.resend')
              : t('library.submissions.submit')}
          </Button>
        )}
        {!FINAL.includes(s.state) && s.state !== 'approved' && (
          <Button variant="secondary" onClick={() => setConfirm('withdraw')}>
            {t('library.submissions.withdraw')}
          </Button>
        )}
      </div>
      {submit.isError && <Problems messages={errorMessages(submit.error)} />}
      {withdraw.isError && <Problems messages={errorMessages(withdraw.error)} />}

      <section className="rv-panel" aria-labelledby="sub-time-h">
        <h2 id="sub-time-h">{t('library.submissions.timeline')}</h2>
        <Timeline
          events={d.events}
          actorLabel={(a) =>
            a === s.submitter_id
              ? t('library.submissions.actorYou')
              : a
                ? t('library.submissions.actorReviewer')
                : t('library.submissions.actorSomeone')
          }
        />
        {!FINAL.includes(s.state) && (
          <CommentForm busy={comment.isPending} onSend={(b) => comment.mutateAsync(b)} />
        )}
        {comment.isError && <Problems messages={errorMessages(comment.error)} />}
      </section>

      <ConfirmDialog
        open={confirm === 'withdraw'}
        title={t('library.submissions.withdrawTitle')}
        description={t('library.submissions.withdrawDescription')}
        confirmLabel={t('library.submissions.withdraw')}
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
  const t = useT();
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
      aria-label={t('library.submissions.details.label')}
      onSubmit={(e) => {
        e.preventDefault();
        if (dirty && title.trim()) save.mutate();
      }}
    >
      <TextInput
        label={t('library.submissions.details.title')}
        value={title}
        maxLength={200}
        onChange={(e) => setTitle(e.target.value)}
      />
      <TextArea
        label={t('library.submissions.details.note')}
        hint={t('library.submissions.details.noteHint')}
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
          {t('library.submissions.details.save')}
        </Button>
      </div>
    </form>
  );
}

function SeriesPreviewPanel({ id }: { id: string }) {
  const t = useT();
  const p = useQuery({ queryKey: key.preview(id), queryFn: () => rv.getMinePreview(id) });
  if (!p.data) return null;
  return (
    <section className="rv-panel" aria-labelledby="sub-meta-h">
      <h2 id="sub-meta-h">{t('library.submissions.preview')}</h2>
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
  const t = useT();
  const [body, setBody] = useState('');
  return (
    <form
      className="rv-form"
      onSubmit={(e) => {
        e.preventDefault();
        const text = body.trim();
        if (text) void onSend(text).then(() => setBody(''));
      }}
    >
      <TextArea
        label={t('library.submissions.comment.label')}
        value={body}
        rows={3}
        maxLength={4000}
        onChange={(e) => setBody(e.target.value)}
      />
      <div>
        <Button type="submit" variant="secondary" disabled={!body.trim()} loading={busy}>
          {t('library.submissions.comment.send')}
        </Button>
      </div>
    </form>
  );
}

// ---- chapters and uploads ----

function ChaptersPanel({ detail, editable }: { detail: SubmissionDetail; editable: boolean }) {
  const t = useT();
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
      <h2 id="sub-ch-h">{t('library.submissions.chapters.title')}</h2>
      {chapters.isPending && <Spinner label={t('library.submissions.chapters.loading')} />}
      {chapters.data && rows.length === 0 && (
        <p className="rv-muted">{t('library.submissions.chapters.empty')}</p>
      )}
      {rows.length > 0 && (
        <div className="rv-table-wrap">
          <table className="rv-table" aria-label={t('library.submissions.chapters.tableLabel')}>
            <thead>
              <tr>
                <th scope="col">{t('library.submissions.chapters.colChapter')}</th>
                <th scope="col">{t('library.submissions.chapters.colFile')}</th>
                <th scope="col">{t('library.submissions.chapters.colReview')}</th>
                <th scope="col">
                  <span className="sr-only-label">
                    {t('library.submissions.chapters.colActions')}
                  </span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => {
                const item = itemOf.get(c.id);
                const name = c.title
                  ? t('library.submissions.chapters.namedTitled', { n: c.number, title: c.title })
                  : t('library.submissions.chapters.named', { n: c.number });
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
                        <span className="rv-muted">
                          {t('library.submissions.chapters.notSubmitted')}
                        </span>
                      )}
                      {item?.note && <div>{item.note}</div>}
                    </td>
                    <td>
                      {editable && (
                        <Button
                          size="sm"
                          variant="ghost"
                          aria-label={t('library.submissions.chapters.removeLabel', { name })}
                          disabled={del.isPending}
                          onClick={() => del.mutate(c)}
                        >
                          {t('library.submissions.chapters.remove')}
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
  const t = useT();
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
      aria-label={t('library.submissions.add.label')}
      onSubmit={(e) => {
        e.preventDefault();
        if (ok) add.mutate();
      }}
    >
      <TextInput
        label={t('library.submissions.add.number')}
        inputMode="decimal"
        value={number}
        onChange={(e) => setNumber(e.target.value)}
      />
      <TextInput
        label={t('library.submissions.add.title')}
        value={title}
        maxLength={200}
        onChange={(e) => setTitle(e.target.value)}
      />
      <Select
        aria-label={t('library.submissions.add.fileType')}
        options={FORMATS.map((f) => ({ value: f.value, label: t(f.labelKey) }))}
        value={format}
        onValueChange={(v) => setFormat(v as ChapterFormat)}
      />
      {add.isError && <Problems messages={errorMessages(add.error)} />}
      <div>
        <Button type="submit" variant="secondary" disabled={!ok} loading={add.isPending}>
          {t('library.submissions.add.submit')}
        </Button>
      </div>
    </form>
  );
}

const UPLOAD_TEXT: Record<string, MessageKey> = {
  awaiting_upload: 'library.submissions.upload.awaiting',
  queued: 'library.submissions.upload.queued',
  running: 'library.submissions.upload.running',
  done: 'library.submissions.upload.done',
  failed: 'library.submissions.upload.failed',
  expired: 'library.submissions.upload.expired',
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
  const t = useT();
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
      setError(errorMessages(e, t('library.submissions.upload.error')));
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
          ? t('library.submissions.upload.pages', { n: chapter.page_count })
          : chapter.format === 'images'
            ? t('library.submissions.upload.noPages')
            : t('library.submissions.upload.noFile')}
        {latest && (
          <span className="rv-muted" role="status">
            {' '}
            · {UPLOAD_TEXT[latest.status] ? t(UPLOAD_TEXT[latest.status]!) : latest.status}
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
          <span className="sr-only-label">
            {t('library.submissions.upload.fileFor', { n: chapter.number })}
          </span>
          <input
            ref={input}
            type="file"
            accept={accept}
            disabled={busy}
            aria-label={t('library.submissions.upload.chooseFor', { n: chapter.number })}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void send(f);
            }}
          />
        </label>
      )}
      {busy && <Spinner label={t('library.submissions.upload.uploading')} />}
    </div>
  );
}
