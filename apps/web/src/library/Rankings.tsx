// Rankings (LIB-BRW-002): the most read series of the week, the month, or all time. Global scope only for
// now; the Friends and People you know scopes wait for a reading-privacy setting.
import { EmptyState, Icon, Notice, SegmentedControl, Skeleton } from '@sideroom/ui';
import { useQuery } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';
import { formatNumber, useT } from '../i18n';
import { getRankings, socialKeys, type RankingPeriod } from './social';
import { scoreText } from './SeriesSocial';
import type { SeriesKind } from './types';
import { AgeBadge, KIND_LABEL, label, STATUS_LABEL } from './ui';

const KINDS: SeriesKind[] = ['manga', 'manhwa', 'manhua', 'webtoon', 'comic', 'novel', 'book'];

export function RankingsPage() {
  const t = useT();
  const [period, setPeriod] = useState<RankingPeriod>('week');
  const [kind, setKind] = useState('');
  const q = useQuery({
    queryKey: socialKeys.rankings(period, kind || undefined),
    queryFn: () => getRankings({ period, kind: kind || undefined, limit: 50 }),
  });
  return (
    <div className="lib-rankings">
      <div className="lib-page-head">
        <h1 className="lib-h1">{t('library.rankings.title')}</h1>
        <div className="lib-browse-tools">
          <SegmentedControl
            label={t('library.rankings.period')}
            size="sm"
            value={period}
            onValueChange={(v) => setPeriod(v as RankingPeriod)}
            options={[
              { value: 'week', label: t('library.rankings.week') },
              { value: 'month', label: t('library.rankings.month') },
              { value: 'all', label: t('library.rankings.all') },
            ]}
          />
          <label className="lib-select">
            <span className="lib-sr-only">{t('library.rankings.kind')}</span>
            <select value={kind} onChange={(e) => setKind(e.target.value)}>
              <option value="">{t('library.rankings.allKinds')}</option>
              {KINDS.map((k) => (
                <option key={k} value={k}>
                  {KIND_LABEL[k]}
                </option>
              ))}
            </select>
          </label>
        </div>
      </div>
      <p className="lib-muted">{t('library.rankings.note')}</p>
      {q.isPending ? (
        <Skeleton height={240} />
      ) : q.isError ? (
        <Notice variant="danger" title={t('library.rankings.loadError')} />
      ) : q.data.items.length === 0 ? (
        <EmptyState
          title={t('library.rankings.emptyTitle')}
          description={t('library.rankings.emptyDescription')}
        />
      ) : (
        <ol className="lib-rank-list" aria-label={t('library.rankings.listLabel')}>
          {q.data.items.map((r) => (
            <li key={r.series.id} className="lib-rank-row">
              <span className="lib-rank-n" aria-label={t('library.rankings.rank', { n: r.rank })}>
                {r.rank}
              </span>
              <Link
                to="/library/s/$seriesId"
                params={{ seriesId: r.series.id }}
                className="lib-update-cover"
                aria-hidden
                tabIndex={-1}
              >
                {r.series.cover_url ? (
                  <img src={r.series.cover_url} alt="" loading="lazy" />
                ) : (
                  <span>{r.series.title.charAt(0)}</span>
                )}
              </Link>
              <div className="lib-update-text">
                <Link
                  to="/library/s/$seriesId"
                  params={{ seriesId: r.series.id }}
                  className="lib-update-title"
                >
                  {r.series.title}
                </Link>
                {r.series.age_restricted && <AgeBadge />}
                <span className="lib-update-line">
                  {KIND_LABEL[r.series.kind]} · {STATUS_LABEL[r.series.status]}
                  {r.series.genres.length > 0
                    ? ` · ${r.series.genres.slice(0, 2).map(label).join(' · ')}`
                    : ''}
                </span>
              </div>
              <span className="lib-rank-stats">
                {r.rating_count > 0 && (
                  <span className="lib-review-rating">
                    <Icon name="star" size={16} />
                    {scoreText(r.rating_average)}
                  </span>
                )}
                <span className="lib-muted">
                  {period === 'all'
                    ? t('library.rankings.followers', { n: formatNumber(r.readers) })
                    : t('library.rankings.readers', { n: formatNumber(r.readers) })}
                </span>
              </span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
