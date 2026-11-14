// The subtitles of one video asset (CIN-ADM-001, D-685): the tracks with their label, language and switches, hide or
// show for a track from the video file, delete for an uploaded one, and the form that adds a subtitle file. The
// server reads the format from the file's content and writes WebVTT again; it answers with a sentence when it
// refuses, which is shown as it comes.
import { Badge, Button, ConfirmDialog, Notice, ProgressBar, Switch, TextInput } from '@sideroom/ui';
import { useId, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { errorText } from '../ui';
import * as api from './api';
import { LoadProblem, useLoad } from './hooks';

export interface SubtitlesPanelProps {
  assetId: string;
  /** Languages the title already lists, offered as suggestions in the language fields. */
  languages: readonly string[];
}

export function SubtitlesPanel({ assetId, languages }: SubtitlesPanelProps) {
  const t = useT();
  const listId = useId();
  const res = useLoad<api.SubtitleTrack[]>(`subs:${assetId}`, () => api.listSubtitles(assetId));
  const [error, setError] = useState<string | null>(null);
  const [asking, setAsking] = useState<api.SubtitleTrack | null>(null);
  const [deleting, setDeleting] = useState(false);

  if (res.error && !res.data)
    return (
      <LoadProblem
        error={res.error}
        status={res.status}
        onRetry={() => void res.reload()}
        what={t('admin.subtitles.what')}
      />
    );
  if (!res.data) return <p className="ss-loading">{t('admin.subtitles.loading')}</p>;
  const tracks = res.data;

  const replace = (next: api.SubtitleTrack) =>
    res.set((cur) =>
      (cur ?? []).map((x) =>
        x.id === next.id
          ? next
          : // A new default clears the others: show it at once, the server did the same in one step.
            next.is_default
            ? { ...x, is_default: false }
            : x,
      ),
    );
  /** The track as the server saved it, or null when it refused. */
  const patch = async (track: api.SubtitleTrack, change: api.SubtitlePatch) => {
    setError(null);
    try {
      const saved = await api.patchSubtitle(assetId, track.id, change);
      replace(saved);
      return saved;
    } catch (e) {
      setError(errorText(e, t('admin.subtitles.saveFailed')));
      return null;
    }
  };

  return (
    <section className="adm-subs" aria-label={t('admin.subtitles.title')}>
      <h3 className="ss-sub">{t('admin.subtitles.title')}</h3>
      <datalist id={listId}>
        {languages.map((l) => (
          <option key={l} value={l} />
        ))}
      </datalist>
      {tracks.length === 0 ? (
        <p className="ss-na">{t('admin.subtitles.none')}</p>
      ) : (
        <ul className="ss-list" aria-label={t('admin.subtitles.list')}>
          {tracks.map((track, i) => (
            <TrackRow
              key={track.id}
              track={track}
              n={i + 1}
              listId={listId}
              onPatch={(change) => patch(track, change)}
              onDelete={() => setAsking(track)}
            />
          ))}
        </ul>
      )}
      {error && (
        <Notice variant="danger" role="alert" className="ss-status">
          {error}
        </Notice>
      )}
      <AddForm
        assetId={assetId}
        listId={listId}
        onAdded={() => {
          setError(null);
          void res.reload();
        }}
      />
      <p className="ss-meta">{t('admin.subtitles.kept')}</p>
      <ConfirmDialog
        open={asking !== null}
        danger
        title={t('admin.subtitles.delete.title', { label: asking?.label ?? '' })}
        description={t('admin.subtitles.delete.body')}
        confirmLabel={t('admin.subtitles.delete.confirm')}
        loading={deleting}
        onCancel={() => setAsking(null)}
        onConfirm={() => {
          const gone = asking;
          if (!gone) return;
          setDeleting(true);
          void api
            .deleteSubtitle(assetId, gone.id)
            .then(() => {
              setAsking(null);
              res.set((cur) => (cur ?? []).filter((x) => x.id !== gone.id));
            })
            .catch((e: unknown) => {
              setAsking(null);
              setError(errorText(e, t('admin.subtitles.deleteFailed')));
            })
            .finally(() => setDeleting(false));
        }}
      />
    </section>
  );
}

function TrackRow({
  track,
  n,
  listId,
  onPatch,
  onDelete,
}: {
  track: api.SubtitleTrack;
  n: number;
  listId: string;
  onPatch: (change: api.SubtitlePatch) => Promise<api.SubtitleTrack | null>;
  onDelete: () => void;
}) {
  const t = useT();
  const [label, setLabel] = useState(track.label);
  const [language, setLanguage] = useState(track.language ?? '');
  // Inline edits are saved when the field is left (or Enter is pressed); a refusal puts the old value back.
  const saveLabel = async () => {
    const next = label.trim();
    if (!next || next === track.label) return setLabel(track.label);
    if (!(await onPatch({ label: next }))) setLabel(track.label);
  };
  const saveLanguage = async () => {
    const next = language.trim();
    if (!next || next === (track.language ?? '')) return setLanguage(track.language ?? '');
    // The server stores a language in one form (D-719: `eng` comes back `en`): show what it saved.
    const saved = await onPatch({ language: next });
    setLanguage((saved ?? track).language ?? '');
  };
  const onEnter = (save: () => Promise<void>) => (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      void save();
    }
  };
  const name = track.label;
  return (
    <li className="ss-row adm-sub-row" data-hidden={track.hidden ? 'true' : 'false'}>
      <TextInput
        label={t('admin.subtitles.labelFor', { n })}
        value={label}
        maxLength={80}
        onChange={(e) => setLabel(e.target.value)}
        onBlur={() => void saveLabel()}
        onKeyDown={onEnter(saveLabel)}
      />
      <TextInput
        label={t('admin.subtitles.languageFor', { n })}
        list={listId}
        value={language}
        placeholder={t('admin.subtitles.languagePlaceholder')}
        onChange={(e) => setLanguage(e.target.value)}
        onBlur={() => void saveLanguage()}
        onKeyDown={onEnter(saveLanguage)}
      />
      <div className="adm-sub-flags">
        <Switch
          label={t('admin.subtitles.default')}
          aria-label={t('admin.subtitles.defaultFor', { name })}
          checked={track.is_default}
          onChange={(e) => void onPatch({ is_default: e.target.checked })}
        />
        <Switch
          label={t('admin.subtitles.forced')}
          aria-label={t('admin.subtitles.forcedFor', { name })}
          checked={track.forced}
          onChange={(e) => void onPatch({ forced: e.target.checked })}
        />
        <Switch
          label={t('admin.subtitles.hearingImpaired')}
          aria-label={t('admin.subtitles.hearingImpairedFor', { name })}
          checked={track.hearing_impaired}
          onChange={(e) => void onPatch({ hearing_impaired: e.target.checked })}
        />
      </div>
      <div className="adm-sub-meta">
        <Badge variant={track.source === 'uploaded' ? 'accent' : 'neutral'}>
          {track.source === 'uploaded'
            ? t('admin.subtitles.uploaded')
            : t('admin.subtitles.fromFile')}
        </Badge>
        {track.styled && <Badge variant="neutral">{t('admin.subtitles.styled')}</Badge>}
        {track.hidden && (
          <span className="ss-meta" role="status">
            {t('admin.subtitles.hiddenNote')}
          </span>
        )}
        {track.source === 'embedded' ? (
          <Button
            size="sm"
            variant="secondary"
            aria-label={
              track.hidden
                ? t('admin.subtitles.showFor', { name })
                : t('admin.subtitles.hideFor', { name })
            }
            onClick={() => void onPatch({ hidden: !track.hidden })}
          >
            {track.hidden ? t('admin.subtitles.show') : t('admin.subtitles.hide')}
          </Button>
        ) : (
          <Button
            size="sm"
            variant="danger"
            aria-label={t('admin.subtitles.deleteFor', { name })}
            onClick={onDelete}
          >
            {t('admin.subtitles.deleteButton')}
          </Button>
        )}
      </div>
    </li>
  );
}

function AddForm({
  assetId,
  listId,
  onAdded,
}: {
  assetId: string;
  listId: string;
  onAdded: () => void;
}) {
  const t = useT();
  const [file, setFile] = useState<File | null>(null);
  const [language, setLanguage] = useState('');
  const [label, setLabel] = useState('');
  const [progress, setProgress] = useState<number | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  const pick = (f: File | undefined) => {
    setProblem(null);
    setDone(null);
    if (f && f.size > api.SUBTITLE_MAX_BYTES) {
      setFile(null);
      if (input.current) input.current.value = '';
      return setProblem(
        t('admin.subtitles.tooLarge', { size: (f.size / (1024 * 1024)).toFixed(1) }),
      );
    }
    setFile(f ?? null);
  };
  const upload = async () => {
    if (!file) return;
    setProblem(null);
    setDone(null);
    setProgress(0);
    try {
      const added = await api.uploadSubtitle(
        assetId,
        file,
        {
          ...(language.trim() ? { language: language.trim() } : {}),
          ...(label.trim() ? { label: label.trim() } : {}),
        },
        setProgress,
      );
      setDone(t('admin.subtitles.added', { name: added.label }));
      setFile(null);
      setLabel('');
      if (input.current) input.current.value = '';
      onAdded();
    } catch (e) {
      setProblem(errorText(e, t('admin.subtitles.uploadFailed')));
    } finally {
      setProgress(null);
    }
  };

  return (
    <form
      className="adm-sub-add"
      aria-label={t('admin.subtitles.addForm')}
      onSubmit={(e) => {
        e.preventDefault();
        void upload();
      }}
    >
      <label className="ss-field">
        <span>{t('admin.subtitles.file')}</span>
        <input
          ref={input}
          className="adm-file"
          type="file"
          accept={api.SUBTITLE_EXTENSIONS.join(',')}
          disabled={progress !== null}
          onChange={(e) => pick(e.target.files?.[0])}
        />
      </label>
      <TextInput
        label={t('admin.subtitles.newLanguage')}
        list={listId}
        value={language}
        placeholder={t('admin.subtitles.languagePlaceholder')}
        onChange={(e) => setLanguage(e.target.value)}
      />
      <TextInput
        label={t('admin.subtitles.newLabel')}
        value={label}
        maxLength={80}
        onChange={(e) => setLabel(e.target.value)}
      />
      <Button type="submit" size="sm" loading={progress !== null} disabled={!file}>
        {t('admin.subtitles.add')}
      </Button>
      {progress !== null && (
        <ProgressBar value={Math.round(progress * 100)} label={t('admin.subtitles.uploading')} />
      )}
      {problem && (
        <p role="alert" className="adm-art-error">
          {problem}
        </p>
      )}
      {done && (
        <p role="status" className="ss-meta">
          {done}
        </p>
      )}
    </form>
  );
}
