// One Cinema submission (RDY-MED-002): the draft title and episodes, the video upload dock, the state of
// every video through the probe and the transcode, reviewer notes, and submit / resubmit / withdraw.
// The pattern is the Library submission page (library/Submissions.tsx) with videos instead of chapters.
import {
  Badge,
  Button,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  Notice,
  Spinner,
  TextArea,
  TextInput,
  type BadgeVariant,
} from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useRef, useState } from 'react';
import { useT, type MessageKey } from '../../i18n';
import * as rv from '../../review/api';
import { errorMessages } from '../../review/api';
import { ArtworkField } from '../../review/ArtworkField';
import { UploadDock } from '../../review/UploadDock';
import {
  STATE_LABEL,
  StateChip,
  Timeline,
  fmtWhen,
  useSubmissionUpdates,
} from '../../review/shared';
import type { SubmissionState } from '../../review/types';
import * as api from './api';
import { encodeTarget, getCinemaUploadManager } from './uploads';
import { VideoPicker } from './VideoPicker';

const key = {
  detail: (id: string) => ['cinema-submit', 'detail', id] as const,
  draft: (id: string) => ['cinema-submit', 'draft', id] as const,
};

const FINAL: SubmissionState[] = ['rejected', 'published', 'withdrawn'];
const EDITABLE: SubmissionState[] = ['draft', 'changes_requested'];

/** Asset states that still move on their own: the screen keeps asking while one is left. */
const MOVING: api.AssetState[] = [
  'awaiting_upload',
  'uploaded',
  'probing',
  'proxy_ready',
  'in_review',
  'approved',
  'transcoding',
];

const STATE_TEXT: Record<api.AssetState, MessageKey> = {
  awaiting_upload: 'cinema.submit.video.uploading',
  uploaded: 'cinema.submit.video.checking',
  probing: 'cinema.submit.video.checking',
  proxy_ready: 'cinema.submit.video.preparing',
  in_review: 'cinema.submit.video.preparing',
  approved: 'cinema.submit.video.preparing',
  transcoding: 'cinema.submit.video.preparing',
  published: 'cinema.submit.video.ready',
  failed: 'cinema.submit.video.failed',
  rejected: 'cinema.submit.video.failed',
  removed: 'cinema.submit.video.removed',
  expired: 'cinema.submit.video.removed',
};

const STATE_VARIANT: Record<api.AssetState, BadgeVariant> = {
  awaiting_upload: 'accent',
  uploaded: 'accent',
  probing: 'accent',
  proxy_ready: 'accent',
  in_review: 'accent',
  approved: 'accent',
  transcoding: 'accent',
  published: 'success',
  failed: 'danger',
  rejected: 'danger',
  removed: 'neutral',
  expired: 'neutral',
};

export function VideoChip({ video }: { video: api.DraftVideo | null }) {
  const t = useT();
  if (!video) return <Badge variant="warning">{t('cinema.submit.video.none')}</Badge>;
  return (
    <span>
      <Badge variant={STATE_VARIANT[video.state]}>{t(STATE_TEXT[video.state])}</Badge>
      {video.error && <span className="rv-muted"> {video.error}</span>}
    </span>
  );
}

function Problems({ messages }: { messages: string[] }) {
  if (messages.length === 0) return null;
  return (
    <Notice variant="danger" role="alert">
      {messages.length === 1 ? (
        messages[0]
      ) : (
        <ul style={{ margin: 0, paddingInlineStart: 18 }}>
          {messages.map((m) => (
            <li key={m}>{m}</li>
          ))}
        </ul>
      )}
    </Notice>
  );
}

const csv = (s: string) =>
  s
    .split(',')
    .map((x) => x.trim())
    .filter(Boolean);

const whole = (v: string): number | null => (/^\d{1,4}$/.test(v.trim()) ? Number(v.trim()) : null);

export function CinemaDraftPage({ id }: { id: string }) {
  const t = useT();
  const qc = useQueryClient();
  useSubmissionUpdates(() => void qc.invalidateQueries({ queryKey: ['cinema-submit'] }));
  const detail = useQuery({ queryKey: key.detail(id), queryFn: () => rv.getMine(id) });
  const draft = useQuery({
    queryKey: key.draft(id),
    queryFn: () => api.getDraft(id),
    refetchInterval: (q) => {
      const d = q.state.data;
      const busy =
        d !== undefined &&
        (d.pending.length > 0 ||
          d.items.some((i) => i.video !== null && MOVING.includes(i.video.state)));
      return busy ? 4000 : false;
    },
  });
  const [confirm, setConfirm] = useState(false);
  const put = (d: Awaited<ReturnType<typeof rv.submit>>) => {
    qc.setQueryData(key.detail(id), d);
    void qc.invalidateQueries({ queryKey: ['submissions', 'mine'] });
  };
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: key.draft(id) });
    void qc.invalidateQueries({ queryKey: key.detail(id) });
  };
  const submit = useMutation({ mutationFn: () => rv.submit(id), onSuccess: put });
  const withdraw = useMutation({
    mutationFn: () => rv.withdraw(id),
    onSuccess: (d) => {
      put(d);
      setConfirm(false);
    },
  });
  const comment = useMutation({
    mutationFn: (body: string) => rv.commentMine(id, body),
    onSuccess: put,
  });

  if (detail.isPending || draft.isPending) return <Spinner label={t('cinema.submit.loadingOne')} />;
  if (detail.isError || !detail.data || draft.isError || !draft.data)
    return (
      <EmptyState
        title={t('cinema.submit.notFoundTitle')}
        description={t('cinema.submit.notFoundDescription')}
        action={
          <Link to="/cinema/submit" className="cin-btn">
            {t('cinema.submit.back')}
          </Link>
        }
      />
    );
  const d = detail.data;
  const s = d.submission;
  const view = draft.data;
  const editable = EDITABLE.includes(s.state);
  const film = view.draft.new_title && view.draft.title.kind === 'movie';
  const lastChange = d.events.filter((e) => e.kind === 'changes_requested').at(-1);
  const taken = new Set(
    view.items
      .filter((i) => i.video !== null && i.season_number !== null)
      .map((i) => `${i.season_number}:${i.number}`),
  );

  return (
    <div className="cin-submit">
      <p style={{ margin: '0 0 12px' }}>
        <Link to="/cinema/submit" className="cin-link">
          {t('cinema.submit.all')}
        </Link>
      </p>
      <div className="rv-head">
        <h1 className="cin-h1">{s.title}</h1>
        <StateChip state={s.state} />
      </div>
      <p className="rv-muted">
        {t('cinema.submit.created', { kind: d.kind_label, when: fmtWhen(s.created_at_ms) })}
        {s.submitted_at_ms
          ? t('cinema.submit.submittedSuffix', { when: fmtWhen(s.submitted_at_ms) })
          : ''}
      </p>

      {s.state === 'changes_requested' && (
        <div className="rv-banner" role="status">
          <strong>{t('cinema.submit.changesTitle')}</strong>
          <p>{lastChange?.body ?? s.decision_reason}</p>
          <p className="rv-muted">{t('cinema.submit.changesHint')}</p>
        </div>
      )}
      {s.state === 'rejected' && (
        <Notice variant="danger" title={t('cinema.submit.rejected')}>
          {s.decision_reason}
        </Notice>
      )}
      {s.state === 'published' && (
        <Notice variant="success" title={t('cinema.submit.published')}>
          {t('cinema.submit.publishedBody')}
        </Notice>
      )}
      {(s.state === 'submitted' || s.state === 'in_review' || s.state === 'approved') && (
        <Notice variant="info" title={STATE_LABEL[s.state]}>
          {s.state === 'approved'
            ? t('cinema.submit.approvedBody')
            : t('cinema.submit.pendingBody')}
        </Notice>
      )}

      {editable && view.draft.new_title && (
        <>
          <TitleForm
            key={view.draft.title.id + view.draft.title.title}
            id={id}
            title={view.draft.title}
            onSaved={refresh}
          />
          <section className="rv-panel" aria-labelledby="cs-art-h">
            <h2 id="cs-art-h">{t('cinema.submit.artwork')}</h2>
            <div className="rv-stack">
              <ArtworkField submissionId={id} slot="poster" onSaved={refresh} />
              <ArtworkField submissionId={id} slot="backdrop" onSaved={refresh} />
            </div>
          </section>
        </>
      )}
      {!view.draft.new_title && (
        <p className="rv-muted">
          {t('cinema.submit.existingSeries', {
            title: view.draft.title.title,
            count: view.draft.published_episodes,
          })}
        </p>
      )}

      <section className="rv-panel" aria-labelledby="cs-videos-h">
        <h2 id="cs-videos-h">
          {film ? t('cinema.submit.video.heading') : t('cinema.submit.episodes')}
        </h2>
        {editable && (
          <VideoPicker
            submissionId={id}
            film={film}
            taken={taken}
            manager={getCinemaUploadManager()}
            onQueued={refresh}
          />
        )}
        <UploadDock manager={getCinemaUploadManager()} />
        {view.items.length === 0 ? (
          <p className="rv-muted">
            {film ? t('cinema.submit.video.emptyFilm') : t('cinema.submit.episodesEmpty')}
          </p>
        ) : (
          <ul className="rv-changes" aria-label={t('cinema.submit.videoList')}>
            {view.items.map((i) => (
              <ItemRow
                key={i.target_id}
                id={id}
                item={i}
                episode={view.draft.episodes.find((e) => e.id === i.target_id) ?? null}
                editable={editable}
                film={film}
                onChanged={refresh}
              />
            ))}
          </ul>
        )}
        {editable && !film && <AddEpisode id={id} onAdded={refresh} />}
      </section>

      <div className="rv-actions" style={{ margin: '16px 0' }}>
        {editable && (
          <Button loading={submit.isPending} onClick={() => submit.mutate()}>
            {s.state === 'changes_requested'
              ? t('cinema.submit.resend')
              : t('cinema.submit.submit')}
          </Button>
        )}
        {!FINAL.includes(s.state) && s.state !== 'approved' && (
          <Button variant="secondary" onClick={() => setConfirm(true)}>
            {t('cinema.submit.withdraw')}
          </Button>
        )}
      </div>
      {submit.isError && <Problems messages={errorMessages(submit.error)} />}
      {withdraw.isError && <Problems messages={errorMessages(withdraw.error)} />}

      <section className="rv-panel" aria-labelledby="cs-time-h">
        <h2 id="cs-time-h">{t('cinema.submit.timeline')}</h2>
        <Timeline
          events={d.events}
          actorLabel={(a) =>
            a === s.submitter_id
              ? t('cinema.submit.actorYou')
              : a
                ? t('cinema.submit.actorReviewer')
                : t('cinema.submit.actorSomeone')
          }
        />
        {!FINAL.includes(s.state) && (
          <CommentForm busy={comment.isPending} onSend={(b) => comment.mutateAsync(b)} />
        )}
        {comment.isError && <Problems messages={errorMessages(comment.error)} />}
      </section>

      <ConfirmDialog
        open={confirm}
        title={t('cinema.submit.withdrawTitle')}
        description={t('cinema.submit.withdrawDescription')}
        confirmLabel={t('cinema.submit.withdraw')}
        danger
        loading={withdraw.isPending}
        onCancel={() => setConfirm(false)}
        onConfirm={() => withdraw.mutate()}
      />
    </div>
  );
}

// ---- title ----

function TitleForm({
  id,
  title,
  onSaved,
}: {
  id: string;
  title: api.DraftTitle;
  onSaved: () => void;
}) {
  const t = useT();
  const [name, setName] = useState(title.title);
  const [original, setOriginal] = useState(title.original_title ?? '');
  const [year, setYear] = useState(title.year === null ? '' : String(title.year));
  const [runtime, setRuntime] = useState(
    title.runtime_min === null ? '' : String(title.runtime_min),
  );
  const [rating, setRating] = useState(title.content_rating ?? '');
  const [synopsis, setSynopsis] = useState(title.synopsis);
  const [genres, setGenres] = useState(title.genres.join(', '));
  const [restricted, setRestricted] = useState(title.age_restricted);
  const save = useMutation({
    mutationFn: () => {
      // The whole editable title goes back; the server keeps the artwork it holds.
      return api.updateTitle(id, {
        kind: title.kind,
        alt_titles: title.alt_titles,
        tagline: title.tagline,
        tags: title.tags,
        languages: title.languages,
        subtitle_languages: title.subtitle_languages,
        cast: title.cast,
        directors: title.directors,
        writers: title.writers,
        studios: title.studios,
        crew: title.crew,
        external_ids: title.external_ids,
        title: name.trim(),
        original_title: original.trim() || null,
        year: whole(year),
        runtime_min: whole(runtime),
        content_rating: rating.trim() || null,
        synopsis: synopsis.trim(),
        genres: csv(genres),
        age_restricted: restricted,
      });
    },
    onSuccess: onSaved,
  });
  return (
    <form
      className="rv-panel rv-form"
      aria-label={t('cinema.submit.details.label')}
      onSubmit={(e) => {
        e.preventDefault();
        if (name.trim()) save.mutate();
      }}
    >
      <TextInput
        label={t('cinema.submit.details.title')}
        value={name}
        maxLength={200}
        onChange={(e) => setName(e.target.value)}
      />
      <TextInput
        label={t('cinema.submit.details.original')}
        value={original}
        maxLength={200}
        onChange={(e) => setOriginal(e.target.value)}
      />
      <div className="rv-row">
        <TextInput
          label={t('cinema.submit.details.year')}
          inputMode="numeric"
          value={year}
          onChange={(e) => setYear(e.target.value)}
        />
        <TextInput
          label={t('cinema.submit.details.runtime')}
          inputMode="numeric"
          value={runtime}
          onChange={(e) => setRuntime(e.target.value)}
        />
        <TextInput
          label={t('cinema.submit.details.rating')}
          value={rating}
          maxLength={20}
          onChange={(e) => setRating(e.target.value)}
        />
      </div>
      <TextInput
        label={t('cinema.submit.details.genres')}
        hint={t('cinema.submit.details.commas')}
        value={genres}
        onChange={(e) => setGenres(e.target.value)}
      />
      <TextArea
        label={t('cinema.submit.details.synopsis')}
        value={synopsis}
        rows={4}
        onChange={(e) => setSynopsis(e.target.value)}
      />
      <Checkbox
        label={t('cinema.submit.details.ageRestricted')}
        checked={restricted}
        onChange={(e) => setRestricted(e.target.checked)}
      />
      {save.isError && <Problems messages={errorMessages(save.error)} />}
      {save.isSuccess && <p role="status">{t('cinema.submit.details.saved')}</p>}
      <div>
        <Button type="submit" variant="secondary" disabled={!name.trim()} loading={save.isPending}>
          {t('cinema.submit.details.save')}
        </Button>
      </div>
    </form>
  );
}

// ---- episodes and videos ----

function ItemRow({
  id,
  item,
  episode,
  editable,
  film,
  onChanged,
}: {
  id: string;
  item: api.DraftItem;
  episode: api.DraftEpisode | null;
  editable: boolean;
  film: boolean;
  onChanged: () => void;
}) {
  const t = useT();
  const [editing, setEditing] = useState(false);
  const replace = useRef<HTMLInputElement>(null);
  const del = useMutation({
    mutationFn: () => api.deleteEpisode(id, item.target_id),
    onSuccess: onChanged,
  });
  const manager = getCinemaUploadManager();
  return (
    <li>
      <strong>{item.label}</strong>
      <VideoChip video={item.video} />
      {item.video && (
        <span className="rv-muted">
          {item.video.filename}
          {item.video.duration_ms !== null
            ? ` · ${t('cinema.submit.video.minutes', { n: Math.round(item.video.duration_ms / 60000) })}`
            : ''}
        </span>
      )}
      {editable && (
        <span className="rv-row">
          <Button
            size="sm"
            variant="secondary"
            onClick={() => replace.current?.click()}
            aria-label={t(item.video ? 'cinema.submit.video.replace' : 'cinema.submit.video.add', {
              label: item.label,
            })}
          >
            {item.video ? t('cinema.submit.video.replaceShort') : t('cinema.submit.video.addShort')}
          </Button>
          <input
            ref={replace}
            type="file"
            hidden
            accept="video/*,.mkv,.m2ts,.ts"
            aria-label={t('cinema.submit.video.fileFor', { label: item.label })}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f)
                void manager
                  .enqueue(
                    id,
                    encodeTarget(
                      item.target_kind === 'episode'
                        ? { kind: 'episode', id: item.target_id }
                        : { kind: 'auto' },
                    ),
                    f,
                  )
                  .then(onChanged, onChanged);
              if (replace.current) replace.current.value = '';
            }}
          />
          {!film && episode && (
            <>
              <Button
                size="sm"
                variant="ghost"
                aria-label={t('cinema.submit.episode.edit', { label: item.label })}
                onClick={() => setEditing((e) => !e)}
              >
                {t('cinema.submit.episode.editShort')}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                loading={del.isPending}
                aria-label={t('cinema.submit.episode.remove', { label: item.label })}
                onClick={() => del.mutate()}
              >
                ✕
              </Button>
            </>
          )}
        </span>
      )}
      {del.isError && <Problems messages={errorMessages(del.error)} />}
      {editing && episode && (
        <EpisodeForm
          id={id}
          episode={episode}
          onDone={() => {
            setEditing(false);
            onChanged();
          }}
        />
      )}
    </li>
  );
}

function EpisodeForm({
  id,
  episode,
  onDone,
}: {
  id: string;
  episode: api.DraftEpisode;
  onDone: () => void;
}) {
  const t = useT();
  const [name, setName] = useState(episode.title);
  const [synopsis, setSynopsis] = useState(episode.synopsis);
  const [aired, setAired] = useState(episode.air_date ?? '');
  const [runtime, setRuntime] = useState(
    episode.runtime_min === null ? '' : String(episode.runtime_min),
  );
  const save = useMutation({
    mutationFn: () =>
      api.updateEpisode(id, episode.id, {
        number: episode.number,
        title: name.trim(),
        synopsis: synopsis.trim(),
        air_date: aired.trim() || null,
        runtime_min: whole(runtime),
      }),
    onSuccess: onDone,
  });
  return (
    <form
      className="rv-form"
      aria-label={t('cinema.submit.episode.formLabel', { n: episode.number })}
      onSubmit={(e) => {
        e.preventDefault();
        save.mutate();
      }}
    >
      <TextInput
        label={t('cinema.submit.episode.title')}
        value={name}
        maxLength={200}
        onChange={(e) => setName(e.target.value)}
      />
      <TextArea
        label={t('cinema.submit.episode.synopsis')}
        value={synopsis}
        rows={3}
        onChange={(e) => setSynopsis(e.target.value)}
      />
      <div className="rv-row">
        <TextInput
          label={t('cinema.submit.episode.aired')}
          hint={t('cinema.submit.episode.airedHint')}
          value={aired}
          onChange={(e) => setAired(e.target.value)}
        />
        <TextInput
          label={t('cinema.submit.episode.runtime')}
          inputMode="numeric"
          value={runtime}
          onChange={(e) => setRuntime(e.target.value)}
        />
      </div>
      {save.isError && <Problems messages={errorMessages(save.error)} />}
      <div>
        <Button type="submit" variant="secondary" loading={save.isPending}>
          {t('cinema.submit.episode.save')}
        </Button>
      </div>
    </form>
  );
}

function AddEpisode({ id, onAdded }: { id: string; onAdded: () => void }) {
  const t = useT();
  const [season, setSeason] = useState('1');
  const [number, setNumber] = useState('');
  const [name, setName] = useState('');
  const add = useMutation({
    mutationFn: () =>
      api.addEpisode(id, {
        season: whole(season) ?? 1,
        number: whole(number) ?? 0,
        title: name.trim(),
        synopsis: '',
      }),
    onSuccess: () => {
      setNumber('');
      setName('');
      onAdded();
    },
  });
  const ok = whole(number) !== null && whole(season) !== null;
  return (
    <form
      className="rv-form"
      aria-label={t('cinema.submit.episode.addLabel')}
      onSubmit={(e) => {
        e.preventDefault();
        if (ok) add.mutate();
      }}
    >
      <div className="rv-row">
        <TextInput
          label={t('cinema.submit.episode.season')}
          inputMode="numeric"
          value={season}
          onChange={(e) => setSeason(e.target.value)}
        />
        <TextInput
          label={t('cinema.submit.episode.number')}
          inputMode="numeric"
          value={number}
          onChange={(e) => setNumber(e.target.value)}
        />
        <TextInput
          label={t('cinema.submit.episode.title')}
          value={name}
          maxLength={200}
          onChange={(e) => setName(e.target.value)}
        />
      </div>
      {add.isError && <Problems messages={errorMessages(add.error)} />}
      <div>
        <Button type="submit" variant="secondary" disabled={!ok} loading={add.isPending}>
          {t('cinema.submit.episode.add')}
        </Button>
      </div>
    </form>
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
        label={t('cinema.submit.comment.label')}
        value={body}
        rows={3}
        maxLength={4000}
        onChange={(e) => setBody(e.target.value)}
      />
      <div>
        <Button type="submit" variant="secondary" disabled={!body.trim()} loading={busy}>
          {t('cinema.submit.comment.send')}
        </Button>
      </div>
    </form>
  );
}
