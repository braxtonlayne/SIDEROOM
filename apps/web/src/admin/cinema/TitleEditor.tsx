// Title editor (CIN-ADM-001): metadata form, artwork, seasons and episodes, variants and trailers, skip markers,
// and the video asset with its transcode status. One persisted `AdminTitleDetail` feeds every tab.
import { Badge, Button, Select, Switch, Tabs, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { Loading, SectionHeader, useAdminViewer } from '../ui';
import * as api from './api';
import { ArtworkPicker } from './ArtworkPicker';
import { AssetPanel } from './AssetPanel';
import {
  Banner,
  ListInput,
  LoadProblem,
  fromList,
  intOrNull,
  toList,
  useLoad,
  useRun,
} from './hooks';
import { ExtrasPanel } from './ExtrasPanel';
import { MarkersPanel } from './MarkersPanel';
import { SeasonsPanel } from './SeasonsPanel';

/** The editable input for a persisted title, optionally with some fields replaced. Artwork keys are recovered
 * from the signed URLs so a metadata save never clears the artwork. */
export function titleToInput(t: api.Title, over: Partial<api.TitleInput> = {}): api.TitleInput {
  return {
    kind: t.kind,
    title: t.title,
    original_title: t.original_title ?? null,
    alt_titles: t.alt_titles,
    year: t.year ?? null,
    runtime_min: t.runtime_min ?? null,
    content_rating: t.content_rating ?? null,
    synopsis: t.synopsis,
    tagline: t.tagline ?? null,
    genres: t.genres,
    tags: t.tags,
    languages: t.languages,
    subtitle_languages: t.subtitle_languages,
    cast: t.cast,
    directors: t.directors,
    writers: t.writers,
    studios: t.studios,
    crew: t.crew,
    age_restricted: t.age_restricted,
    poster_key: api.keyFromUrl(t.poster_url),
    backdrop_key: api.keyFromUrl(t.backdrop_url),
    logo_key: api.keyFromUrl(t.logo_url),
    external_ids: t.external_ids,
    ...over,
  };
}

interface Form {
  kind: api.TitleKind;
  title: string;
  original_title: string;
  alt_titles: string;
  year: string;
  runtime_min: string;
  content_rating: string;
  tagline: string;
  synopsis: string;
  genres: string;
  tags: string;
  languages: string;
  subtitle_languages: string;
  cast: string;
  directors: string;
  writers: string;
  studios: string;
  crew: string;
  age_restricted: boolean;
  imdb: string;
  tmdb: string;
}

const blank: Form = {
  kind: 'movie',
  title: '',
  original_title: '',
  alt_titles: '',
  year: '',
  runtime_min: '',
  content_rating: '',
  tagline: '',
  synopsis: '',
  genres: '',
  tags: '',
  languages: '',
  subtitle_languages: '',
  cast: '',
  directors: '',
  writers: '',
  studios: '',
  crew: '',
  age_restricted: false,
  imdb: '',
  tmdb: '',
};

const toForm = (t: api.Title): Form => ({
  kind: t.kind,
  title: t.title,
  original_title: t.original_title ?? '',
  alt_titles: fromList(t.alt_titles),
  year: t.year?.toString() ?? '',
  runtime_min: t.runtime_min?.toString() ?? '',
  content_rating: t.content_rating ?? '',
  tagline: t.tagline ?? '',
  synopsis: t.synopsis,
  genres: fromList(t.genres),
  tags: fromList(t.tags),
  languages: fromList(t.languages),
  subtitle_languages: fromList(t.subtitle_languages),
  cast: fromList(t.cast),
  directors: fromList(t.directors),
  writers: fromList(t.writers),
  studios: fromList(t.studios),
  crew: fromList(t.crew),
  age_restricted: t.age_restricted,
  imdb: t.external_ids.imdb ?? '',
  tmdb: t.external_ids.tmdb ?? '',
});

const orNull = (s: string) => s.trim() || null;

function formToInput(f: Form, base?: api.Title): api.TitleInput {
  const ext = { ...(base?.external_ids ?? {}) };
  for (const [k, v] of [
    ['imdb', f.imdb],
    ['tmdb', f.tmdb],
  ] as const) {
    if (v.trim()) ext[k] = v.trim();
    else delete ext[k];
  }
  const keys = base
    ? {
        poster_key: api.keyFromUrl(base.poster_url),
        backdrop_key: api.keyFromUrl(base.backdrop_url),
        logo_key: api.keyFromUrl(base.logo_url),
      }
    : {};
  return {
    kind: f.kind,
    title: f.title.trim(),
    original_title: orNull(f.original_title),
    alt_titles: toList(f.alt_titles),
    year: intOrNull(f.year),
    runtime_min: intOrNull(f.runtime_min),
    content_rating: orNull(f.content_rating),
    tagline: orNull(f.tagline),
    synopsis: f.synopsis,
    genres: toList(f.genres),
    tags: toList(f.tags),
    languages: toList(f.languages),
    subtitle_languages: toList(f.subtitle_languages),
    cast: toList(f.cast),
    directors: toList(f.directors),
    writers: toList(f.writers),
    studios: toList(f.studios),
    crew: toList(f.crew),
    age_restricted: f.age_restricted,
    external_ids: ext,
    ...keys,
  };
}

export function TitleEditor({ titleId, onClose }: { titleId: string | null; onClose: () => void }) {
  const t = useT();
  const [id, setId] = useState<string | null>(titleId);
  const detail = useLoad<api.AdminTitleDetail | null>(`cinema-title:${id ?? 'new'}`, async () =>
    id ? api.getTitle(id) : null,
  );
  const viewer = useAdminViewer();
  const canPublish = viewer.can('PUBLISH_MEDIA');
  const publish = useRun();

  const d = detail.data;
  const title = d?.title;

  const toggle = () => {
    if (!title) return;
    void publish.run(
      async () => {
        if (title.published) await api.unpublishTitle(title.id);
        else await api.publishTitle(title.id);
        await detail.reload();
        return true;
      },
      title.published ? t('admin.titleEditor.unpublished') : t('admin.titleEditor.published'),
    );
  };

  let body;
  if (detail.error && !d) {
    body = (
      <LoadProblem
        error={detail.error}
        status={detail.status}
        onRetry={() => void detail.reload()}
        what={t('admin.titleEditor.whatTitle')}
      />
    );
  } else if (detail.loading && !d && id) {
    body = <Loading label={t('admin.titleEditor.loading')} />;
  } else if (!id || !d || !title) {
    body = (
      <DetailsForm
        key="new"
        readOnly={!canPublish}
        onSave={async (input) => {
          const created = await api.createTitle(input);
          setId(created.id);
          return created;
        }}
        createLabel={t('admin.titleEditor.createDraft')}
      />
    );
  } else {
    body = (
      <Tabs
        label={t('admin.titleEditor.sections')}
        items={[
          {
            id: 'details',
            label: t('admin.titleEditor.tab.details'),
            content: (
              <DetailsForm
                key={title.updated_at_ms}
                title={title}
                readOnly={!canPublish}
                onSave={async (input) => {
                  const updated = await api.updateTitle(title.id, input);
                  await detail.reload();
                  return updated;
                }}
              />
            ),
          },
          {
            id: 'artwork',
            label: t('admin.titleEditor.tab.artwork'),
            content: (
              <ArtworkTab title={title} onSaved={() => detail.reload()} readOnly={!canPublish} />
            ),
          },
          ...(title.kind !== 'movie'
            ? [
                {
                  id: 'episodes',
                  label: t('admin.titleEditor.tab.episodes'),
                  content: <SeasonsPanel detail={d} reload={() => detail.reload()} />,
                },
              ]
            : []),
          {
            id: 'extras',
            label: t('admin.titleEditor.tab.extras'),
            content: <ExtrasPanel detail={d} reload={() => detail.reload()} />,
          },
          {
            id: 'markers',
            label: t('admin.titleEditor.tab.markers'),
            content: <MarkersPanel detail={d} reload={() => detail.reload()} />,
          },
          {
            id: 'asset',
            label: t('admin.titleEditor.tab.asset'),
            content: <AssetPanel detail={d} reload={() => detail.reload()} />,
          },
        ]}
      />
    );
  }

  return (
    <>
      <SectionHeader
        title={title ? title.title : t('admin.titleEditor.newTitle')}
        description={
          title ? (
            <>
              {title.kind} ·{' '}
              <Badge variant={title.published ? 'success' : 'neutral'}>
                {title.published
                  ? t('admin.titleEditor.statusPublished')
                  : t('admin.titleEditor.statusDraft')}
              </Badge>{' '}
              {title.age_restricted && (
                <Badge variant="danger">{t('admin.titleEditor.age18')}</Badge>
              )}
            </>
          ) : (
            t('admin.titleEditor.newDescription')
          )
        }
        actions={
          <>
            <Button variant="ghost" onClick={onClose}>
              {t('admin.titleEditor.back')}
            </Button>
            {title && (canPublish || title.published) && (
              <Button
                variant={title.published ? 'secondary' : 'primary'}
                loading={publish.busy}
                onClick={toggle}
              >
                {title.published
                  ? t('admin.titleEditor.unpublish')
                  : t('admin.titleEditor.publish')}
              </Button>
            )}
          </>
        }
      />
      <Banner error={publish.error} notice={publish.notice} />
      {body}
    </>
  );
}

function DetailsForm({
  title,
  onSave,
  readOnly,
  createLabel,
}: {
  title?: api.Title;
  onSave: (input: api.TitleInput) => Promise<api.Title>;
  readOnly?: boolean;
  createLabel?: string;
}) {
  const t = useT();
  const kindOptions = [
    { value: 'movie', label: t('admin.titleEditor.kind.movie') },
    { value: 'series', label: t('admin.titleEditor.kind.series') },
    { value: 'anime', label: t('admin.titleEditor.kind.anime') },
  ];
  const [f, setF] = useState<Form>(title ? toForm(title) : blank);
  const { busy, error, notice, run } = useRun();
  const set = <K extends keyof Form>(k: K, v: Form[K]) => setF((c) => ({ ...c, [k]: v }));
  const text = (k: keyof Form, label: string, extra: { hint?: string; type?: string } = {}) => (
    <TextInput
      label={label}
      value={f[k] as string}
      disabled={readOnly}
      onChange={(e) => set(k, e.target.value as never)}
      {...(extra.hint ? { hint: extra.hint } : {})}
      {...(extra.type ? { type: extra.type } : {})}
    />
  );
  const list = (k: keyof Form, label: string) => (
    <ListInput label={label} value={f[k] as string} onChange={(v) => set(k, v as never)} />
  );
  const valid = f.title.trim().length > 0;
  return (
    <form
      className="adm-form adm-cin-form"
      aria-label={t('admin.titleEditor.details.aria')}
      onSubmit={(e) => {
        e.preventDefault();
        if (!valid || busy || readOnly) return;
        void run(
          async () => {
            await onSave(formToInput(f, title));
            return true;
          },
          title ? t('admin.titleEditor.details.saved') : t('admin.titleEditor.details.created'),
        );
      }}
    >
      <div className="adm-cin-grid">
        <Select
          aria-label={t('admin.titleEditor.details.kind')}
          options={kindOptions}
          value={f.kind}
          disabled={readOnly}
          onValueChange={(v) => set('kind', v as api.TitleKind)}
        />
        {text('title', t('admin.titleEditor.details.title'))}
        {text('original_title', t('admin.titleEditor.details.originalTitle'))}
        {list('alt_titles', t('admin.titleEditor.details.altTitles'))}
        {text('year', t('admin.titleEditor.details.year'), { type: 'number' })}
        {text('runtime_min', t('admin.titleEditor.details.runtime'), { type: 'number' })}
        {text('content_rating', t('admin.titleEditor.details.contentRating'), {
          hint: t('admin.titleEditor.details.contentRatingHint'),
        })}
        {text('tagline', t('admin.titleEditor.details.tagline'))}
      </div>
      <TextArea
        label={t('admin.titleEditor.details.synopsis')}
        rows={5}
        value={f.synopsis}
        disabled={readOnly}
        onChange={(e) => set('synopsis', e.target.value)}
      />
      <div className="adm-cin-grid">
        {list('genres', t('admin.titleEditor.details.genres'))}
        {list('tags', t('admin.titleEditor.details.tags'))}
        {list('languages', t('admin.titleEditor.details.languages'))}
        {list('subtitle_languages', t('admin.titleEditor.details.subtitleLanguages'))}
        {list('cast', t('admin.titleEditor.details.cast'))}
        {list('directors', t('admin.titleEditor.details.directors'))}
        {list('writers', t('admin.titleEditor.details.writers'))}
        {list('studios', t('admin.titleEditor.details.studios'))}
        {list('crew', t('admin.titleEditor.details.crew'))}
        {text('imdb', t('admin.titleEditor.details.imdb'))}
        {text('tmdb', t('admin.titleEditor.details.tmdb'))}
      </div>
      <Switch
        label={t('admin.titleEditor.details.ageRestricted')}
        checked={f.age_restricted}
        disabled={readOnly}
        onChange={(e) => set('age_restricted', e.target.checked)}
      />
      <Banner error={error} notice={notice} />
      {readOnly ? (
        <p className="ss-na">{t('admin.titleEditor.details.readOnly')}</p>
      ) : (
        <div className="adm-actions">
          <Button type="submit" loading={busy} disabled={!valid}>
            {createLabel ?? t('admin.titleEditor.details.save')}
          </Button>
        </div>
      )}
    </form>
  );
}

function ArtworkTab({
  title,
  onSaved,
  readOnly,
}: {
  title: api.Title;
  onSaved: () => Promise<void>;
  readOnly?: boolean;
}) {
  const t = useT();
  const slots: {
    slot: 'poster' | 'backdrop' | 'logo';
    label: string;
    url?: string | null;
    wide?: boolean;
  }[] = [
    { slot: 'poster', label: t('admin.titleEditor.art.poster'), url: title.poster_url },
    {
      slot: 'backdrop',
      label: t('admin.titleEditor.art.backdrop'),
      url: title.backdrop_url,
      wide: true,
    },
    { slot: 'logo', label: t('admin.titleEditor.art.logo'), url: title.logo_url },
  ];
  const keyField = { poster: 'poster_key', backdrop: 'backdrop_key', logo: 'logo_key' } as const;
  return (
    <div className="adm-cin-art">
      <p className="ss-desc">{t('admin.titleEditor.art.intro')}</p>
      {slots.map((s) => (
        <ArtworkPicker
          key={s.slot}
          label={s.label}
          slot={s.slot}
          currentUrl={s.url}
          wide={!!s.wide}
          disabled={readOnly}
          onUpload={async (file, progress) => {
            const key = await api.uploadArtwork(title.id, s.slot, file, progress);
            await api.updateTitle(title.id, titleToInput(title, { [keyField[s.slot]]: key }));
            await onSaved();
          }}
          onClear={async () => {
            await api.updateTitle(title.id, titleToInput(title, { [keyField[s.slot]]: null }));
            await onSaved();
          }}
        />
      ))}
    </div>
  );
}
