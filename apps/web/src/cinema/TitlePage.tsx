// Title detail (CIN-TTL-001..004): header with actions, variant picker, tabs (Episodes, Details, Reviews,
// Comments, More like this, Contributors), season selector with episode list.
import {
  Button,
  DisplayHeading,
  DropdownMenu,
  EmptyState,
  GoldButton,
  Icon,
  Notice,
  ProgressBar,
  Select,
  Skeleton,
  Tabs,
  useToast,
  type MenuEntry,
} from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { lazy, Suspense, useState } from 'react';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { ReportButton } from '../moderation/reports';
import {
  getEpisodes,
  getRelated,
  getTitle,
  getRatings,
  keys,
  setInList,
  setRating,
  setWatched,
} from './api';
import { DownloadModal } from './player/DownloadModal';
import { Comments, Reviews } from './Social';
import { playLabel } from './Discover';
import { TrailerModal } from './TrailerModal';
import type { Episode, Title, TitleDetail } from './types';
import {
  AgeBadge,
  formatRuntime,
  label,
  mergeMy,
  TitleCard,
  useMy,
  useOpen,
  watchHref,
} from './ui';

const STANDARD_CUT = '__standard';

// Loaded on first use: the destination picker pulls in the Lounge data hooks.
const StartPartyDialog = lazy(() =>
  import('../watchparty/StartPartyDialog').then((m) => ({ default: m.StartPartyDialog })),
);

function Meta({ d }: { d: TitleDetail }) {
  const tr = useT();
  const t = d.title;
  const runtime =
    t.kind === 'movie'
      ? formatRuntime(t.runtime_min)
      : t.season_count
        ? tr('cinema.title.seasonCount', { n: t.season_count })
        : '';
  return (
    <p className="cin-hero-meta">
      {t.year && <span>{t.year}</span>}
      {runtime && <span>{runtime}</span>}
      {t.content_rating && <span className="cin-rating-badge">{t.content_rating}</span>}
      {t.genres.length > 0 && <span>{t.genres.map(label).join(', ')}</span>}
      {t.age_restricted && <AgeBadge />}
    </p>
  );
}

function Header({
  d,
  variant,
  onVariant,
}: {
  d: TitleDetail;
  variant?: string;
  onVariant: (v: string) => void;
}) {
  const tr = useT();
  const my = useMy();
  const t = mergeMy(d.title, my);
  const open = useOpen();
  const toast = useToast();
  const qc = useQueryClient();
  const [trailer, setTrailer] = useState(false);
  const [download, setDownload] = useState(false);
  const [together, setTogether] = useState(false);
  const refresh = () => void qc.invalidateQueries({ queryKey: ['cinema'] });
  const inList = t.my?.in_list ?? false;
  const list = useMutation({
    mutationFn: (on: boolean) => setInList(t.id, on),
    onSuccess: refresh,
  });
  const rate = useMutation({
    mutationFn: (v: number | null) => setRating(t.id, v),
    onSuccess: refresh,
  });
  const ratings = useQuery({
    queryKey: keys.ratings(t.id),
    queryFn: () => getRatings(t.id),
    staleTime: 30_000,
  });
  const mine = ratings.data?.mine ?? null;
  const query = [
    variant ? `variant=${encodeURIComponent(variant)}` : '',
    t.my?.episode_id ? `episode=${encodeURIComponent(t.my.episode_id)}` : '',
  ].filter(Boolean);
  const watch = `${watchHref(t.id)}${query.length ? `?${query.join('&')}` : ''}`;

  const ratingItems: MenuEntry[] = [
    { type: 'label', label: tr('cinema.title.yourRating') },
    ...Array.from({ length: 10 }, (_, i) => 10 - i).map((n): MenuEntry => ({
      type: 'checkbox',
      label: tr('cinema.title.outOfTen', { n }),
      checked: mine === n,
      onCheckedChange: () => rate.mutate(n),
    })),
    ...(mine
      ? ([
          { type: 'separator' },
          { label: tr('cinema.title.clearRating'), onSelect: () => rate.mutate(null) },
        ] as MenuEntry[])
      : []),
  ];

  const share = async () => {
    const url = `${window.location.origin}/cinema/t/${t.id}`;
    try {
      await navigator.clipboard.writeText(url);
      toast.show({
        title: tr('cinema.title.linkCopied'),
        description: tr('cinema.title.linkCopiedBody'),
      });
    } catch {
      toast.show({ title: tr('cinema.title.copyFailed'), variant: 'warning' });
    }
  };

  return (
    <header className="cin-title-head">
      {t.backdrop_url && <img className="cin-hero-img" src={t.backdrop_url} alt="" />}
      <div className="cin-hero-shade" aria-hidden="true" />
      <div className="cin-title-body">
        {t.logo_url ? (
          <>
            <img className="cin-title-logo" src={t.logo_url} alt="" />
            <h1 className="cin-sr-only">{t.title}</h1>
          </>
        ) : (
          <DisplayHeading as="h1" size="xl">
            {t.title}
          </DisplayHeading>
        )}
        {t.original_title && t.original_title !== t.title && (
          <p className="cin-muted">{t.original_title}</p>
        )}
        <Meta d={d} />
        <p className="cin-hero-synopsis">{t.synopsis}</p>
        {d.variants.length > 0 && (
          <div className="cin-variant">
            <Select
              aria-label={tr('cinema.title.version')}
              placeholder={tr('cinema.title.version')}
              value={variant ?? STANDARD_CUT}
              onValueChange={(v) => onVariant(v === STANDARD_CUT ? '' : v)}
              options={[
                { value: STANDARD_CUT, label: tr('cinema.title.standardCut') },
                ...d.variants.map((v) => ({ value: v.id, label: v.label })),
              ]}
            />
          </div>
        )}
        <div className="cin-hero-actions" id="rate">
          <GoldButton onClick={() => void open(watch, t.age_restricted)}>
            <Icon name="play" size={16} /> {playLabel(t)}
          </GoldButton>
          <Button variant="secondary" aria-pressed={inList} onClick={() => list.mutate(!inList)}>
            <Icon name={inList ? 'check' : 'plus'} size={16} /> {tr('cinema.title.myList')}
          </Button>
          <DropdownMenu
            label={tr('cinema.title.rate')}
            items={ratingItems}
            trigger={
              <Button variant="secondary">
                <Icon name="star" size={16} />{' '}
                {mine ? tr('cinema.title.rated', { n: mine }) : tr('cinema.title.rate')}
              </Button>
            }
          />
          <Button variant="secondary" onClick={() => setTogether(true)}>
            <Icon name="users" size={16} /> {tr('cinema.title.watchTogether')}
          </Button>
          <Button variant="secondary" onClick={() => void share()}>
            <Icon name="link" size={16} /> {tr('cinema.title.shareToLounge')}
          </Button>
          <Button
            variant="secondary"
            disabled={d.seasons.length > 0}
            title={d.seasons.length > 0 ? tr('cinema.title.downloadEpisodeHint') : undefined}
            onClick={() => setDownload(true)}
          >
            <Icon name="download" size={16} /> {tr('cinema.title.download')}
          </Button>
          <ReportButton target={{ kind: 'cinema', id: t.id }} />
          {(t.trailer_url || t.has_trailer) && (
            <Button
              variant="secondary"
              onClick={() =>
                t.trailer_url
                  ? setTrailer(true)
                  : void open(`${watchHref(t.id)}?trailer=1`, t.age_restricted)
              }
            >
              {tr('cinema.title.trailer')}
            </Button>
          )}
        </div>
        {t.age_restricted && <p className="cin-muted">{tr('cinema.title.adultNote')}</p>}
      </div>
      <DownloadModal
        open={download}
        onClose={() => setDownload(false)}
        titleId={t.id}
        title={t.title}
      />
      {together && (
        <Suspense fallback={null}>
          <StartPartyDialog
            open
            onClose={() => setTogether(false)}
            title={{
              titleId: t.id,
              episodeId: t.my?.episode_id ?? null,
              variantId: variant ?? null,
              label: t.title,
            }}
          />
        </Suspense>
      )}
      {t.trailer_url && (
        <TrailerModal
          open={trailer}
          onClose={() => setTrailer(false)}
          title={t.title}
          src={t.trailer_url}
        />
      )}
    </header>
  );
}

function EpisodeRow({
  e,
  titleId,
  restricted,
  season,
  seriesTitle,
}: {
  e: Episode;
  titleId: string;
  seriesTitle: string;
  restricted: boolean;
  season: number;
}) {
  const tr = useT();
  const open = useOpen();
  const qc = useQueryClient();
  const seen = useMutation({
    mutationFn: (w: boolean) => setWatched(titleId, w, { episode_id: e.id }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: keys.episodes(titleId, season) }),
  });
  const [download, setDownload] = useState(false);
  const started = (e.progress_percent ?? 0) > 0 && !e.watched;
  return (
    <li className="cin-ep">
      <button
        type="button"
        className="cin-ep-still"
        aria-label={tr('cinema.title.playEpisode', { n: e.number, title: e.title })}
        onClick={() => void open(`${watchHref(titleId)}?episode=${e.id}`, restricted)}
      >
        {e.still_url ? (
          <img src={e.still_url} alt="" loading="lazy" />
        ) : (
          <span aria-hidden="true">{e.number}</span>
        )}
        {started && (
          <ProgressBar
            className="cin-ep-progress"
            value={e.progress_percent ?? 0}
            label={tr('cinema.title.episodeProgress', { n: e.number })}
            size={3}
          />
        )}
      </button>
      <div className="cin-ep-text">
        <p className="cin-ep-title">
          <span className="cin-ep-num">{e.number}.</span> {e.title}
          {e.runtime_min ? (
            <span className="cin-muted"> · {formatRuntime(e.runtime_min)}</span>
          ) : null}
        </p>
        {e.synopsis && <p className="cin-muted cin-ep-syn">{e.synopsis}</p>}
      </div>
      <button
        type="button"
        className="cin-qa cin-ep-seen"
        aria-label={tr('cinema.title.downloadEpisode', { n: e.number })}
        onClick={() => setDownload(true)}
      >
        <Icon name="download" size={16} />
      </button>
      <DownloadModal
        open={download}
        onClose={() => setDownload(false)}
        titleId={titleId}
        episodeId={e.id}
        title={tr('cinema.title.episodeHeading', {
          series: seriesTitle,
          season,
          n: e.number,
        })}
      />
      <button
        type="button"
        className="cin-qa cin-ep-seen"
        aria-pressed={e.watched}
        aria-label={
          e.watched
            ? tr('cinema.title.markEpUnwatched', { n: e.number })
            : tr('cinema.title.markEpWatched', { n: e.number })
        }
        onClick={() => seen.mutate(!e.watched)}
      >
        <Icon name="check" size={16} />
      </button>
    </li>
  );
}

function Episodes({ d }: { d: TitleDetail }) {
  const tr = useT();
  const t = d.title;
  const qc = useQueryClient();
  const seasons = d.seasons;
  const [season, setSeason] = useState<number | undefined>(undefined);
  const current = season ?? seasons[0]?.number ?? 1;
  const q = useQuery({
    queryKey: keys.episodes(t.id, current),
    queryFn: () => getEpisodes(t.id, current),
  });
  const all = useMutation({
    mutationFn: (w: boolean) => setWatched(t.id, w, { season: current }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: keys.episodes(t.id, current) }),
  });
  const items = q.data?.items ?? [];
  const allSeen = items.length > 0 && items.every((e) => e.watched);
  return (
    <div>
      <div className="cin-ep-tools">
        {seasons.length > 0 && (
          <Select
            aria-label={tr('cinema.title.season')}
            placeholder={tr('cinema.title.season')}
            value={String(current)}
            onValueChange={(v) => setSeason(Number(v))}
            options={seasons.map((s) => ({
              value: String(s.number),
              label: s.title ?? tr('cinema.title.seasonN', { n: s.number }),
            }))}
          />
        )}
        <Button
          variant="secondary"
          disabled={items.length === 0}
          onClick={() => all.mutate(!allSeen)}
        >
          {allSeen ? tr('cinema.title.markSeasonUnwatched') : tr('cinema.title.markSeasonWatched')}
        </Button>
      </div>
      {q.isPending && <Skeleton height={80} />}
      {q.isError && (
        <Notice variant="danger" title={tr('cinema.title.episodesErrorTitle')}>
          {tr('cinema.title.tryAgainMoment')}
        </Notice>
      )}
      {q.isSuccess && items.length === 0 && <EmptyState title={tr('cinema.title.noEpisodes')} />}
      <ol className="cin-eps">
        {items.map((e) => (
          <EpisodeRow
            key={e.id}
            e={e}
            titleId={t.id}
            restricted={t.age_restricted}
            season={current}
            seriesTitle={t.title}
          />
        ))}
      </ol>
    </div>
  );
}

function Details({ d }: { d: TitleDetail }) {
  const tr = useT();
  const rows: [string, string][] = [
    [tr('cinema.title.originalTitle'), d.title.original_title ?? ''],
    [tr('cinema.title.cast'), d.cast.map((p) => p.name).join(', ')],
    [
      tr('cinema.title.crew'),
      d.crew.map((p) => (p.role ? `${p.name} (${p.role})` : p.name)).join(', '),
    ],
    [tr('cinema.title.studios'), d.studios.join(', ')],
    [tr('cinema.title.audio'), d.audio_languages.join(', ')],
    [tr('cinema.title.subtitles'), d.subtitle_languages.join(', ')],
    [tr('cinema.title.versions'), d.variants.map((v) => v.label).join(', ')],
  ];
  const shown = rows.filter(([, v]) => v);
  if (shown.length === 0) return <EmptyState title={tr('cinema.title.noDetails')} />;
  return (
    <dl className="cin-details">
      {shown.map(([k, v]) => (
        <div key={k} className="cin-details-row">
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function Related({ title }: { title: Title }) {
  const tr = useT();
  const q = useQuery({ queryKey: keys.related(title.id), queryFn: () => getRelated(title) });
  if (q.isPending) return <Skeleton height={200} />;
  const items = q.data?.items ?? [];
  if (q.isError || items.length === 0)
    return <EmptyState title={tr('cinema.title.nothingSimilar')} />;
  return (
    <>
      <h2 className="cin-sr-only">{tr('cinema.title.moreLikeThis')}</h2>
      <ul className="cin-grid">
        {items.map((it) => (
          <li key={it.id}>
            <TitleCard title={it} />
          </li>
        ))}
      </ul>
    </>
  );
}

export function TitlePage({ id }: { id: string }) {
  const tr = useT();
  const [variant, setVariant] = useState<string | undefined>();
  const q = useQuery({ queryKey: keys.title(id), queryFn: () => getTitle(id) });

  if (q.isPending)
    return (
      <div className="cin-page">
        <Skeleton height={320} />
      </div>
    );
  if (q.isError) {
    const missing = q.error instanceof ApiError && q.error.status === 404;
    return (
      <div className="cin-page">
        <EmptyState
          title={missing ? tr('cinema.title.notFoundTitle') : tr('cinema.title.unavailableTitle')}
          description={
            missing ? tr('cinema.title.notFoundBody') : tr('cinema.title.tryAgainMoment')
          }
        />
      </div>
    );
  }
  const d = q.data;
  const t = d.title;
  const episodic = d.seasons.length > 0;
  return (
    <>
      <Header d={d} variant={variant} onVariant={setVariant} />
      <div className="cin-page">
        <Tabs
          label={tr('cinema.title.sections')}
          items={[
            ...(episodic
              ? [
                  {
                    id: 'episodes',
                    label: tr('cinema.title.tabEpisodes'),
                    content: <Episodes d={d} />,
                  },
                ]
              : []),
            { id: 'details', label: tr('cinema.title.tabDetails'), content: <Details d={d} /> },
            {
              id: 'reviews',
              label: tr('cinema.title.tabReviews'),
              content: <Reviews titleId={t.id} />,
            },
            {
              id: 'comments',
              label: tr('cinema.title.tabComments'),
              content: <Comments titleId={t.id} />,
            },
            {
              id: 'related',
              label: tr('cinema.title.moreLikeThis'),
              content: <Related title={t} />,
            },
            {
              id: 'contributors',
              label: tr('cinema.title.tabContributors'),
              disabled: d.contributors.length === 0,
              content: (
                <ul className="cin-contrib">
                  {d.contributors.map((c) => (
                    <li key={`${c.user_id}-${c.role}`}>
                      {c.name} <span className="cin-muted">· {c.role}</span>
                    </li>
                  ))}
                </ul>
              ),
            },
          ]}
        />
      </div>
    </>
  );
}
