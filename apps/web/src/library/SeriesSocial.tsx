// The social parts of the series page: ratings with a scope switch and distribution (LIB-SER-001/002,
// LIB-SOC-001), "Add to list" (LIB-SOC-006) and the Reviews tab (LIB-SOC-003, LIB-SOC-011).
import {
  Button,
  Checkbox,
  EmptyState,
  Icon,
  Modal,
  Notice,
  SegmentedControl,
  Skeleton,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { formatNumber, useT } from '../i18n';
import {
  addToList,
  createList,
  deleteReview,
  getMyLists,
  getRatings,
  getReviews,
  putReview,
  rateSeries,
  removeFromList,
  setHelpful,
  socialKeys,
  unrateSeries,
  type RatingAggregate,
  type RatingScope,
  type Review,
  type ReviewSort,
} from './social';
import { timeAgo } from './ui';

const SCORES = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

/** "8.4" for an average, or a dash without ratings. */
export const scoreText = (a: number | null | undefined) =>
  a == null ? '–' : formatNumber(a, { minimumFractionDigits: 1, maximumFractionDigits: 1 });

/* ---------------------------------------------------------------- ratings */

export function RatingBlock({ seriesId }: { seriesId: string }) {
  const t = useT();
  const qc = useQueryClient();
  const [scope, setScope] = useState<RatingScope>('global');
  const q = useQuery({
    queryKey: socialKeys.ratings(seriesId),
    queryFn: () => getRatings(seriesId),
  });
  const rate = useMutation({
    mutationFn: async (score: number | null) => {
      if (score === null) await unrateSeries(seriesId);
      else await rateSeries(seriesId, score);
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: socialKeys.ratings(seriesId) }),
  });
  if (q.isPending) return <Skeleton height={56} />;
  if (q.isError) return null;
  const r = q.data;
  const agg: RatingAggregate = r[scope];
  const max = Math.max(1, ...r.distribution);
  return (
    <section className="lib-rating" aria-label={t('library.social.rating.section')}>
      <div className="lib-rating-top">
        <p className="lib-rating-score">
          <Icon name="star" size={20} />
          <strong>{scoreText(agg.average)}</strong>
          <span className="lib-muted">
            {t('library.social.rating.outOfTen')}
            {' · '}
            {t('library.social.rating.count', { n: agg.count })}
          </span>
        </p>
        <SegmentedControl
          label={t('library.social.rating.scope')}
          size="sm"
          value={scope}
          onValueChange={(v) => setScope(v as RatingScope)}
          options={[
            { value: 'global', label: t('library.social.rating.scopeGlobal') },
            { value: 'known', label: t('library.social.rating.scopeKnown') },
            { value: 'friends', label: t('library.social.rating.scopeFriends') },
          ]}
        />
        <label className="lib-select lib-rate">
          <span>{t('library.social.rating.yours')}</span>
          <select
            value={r.mine ?? ''}
            disabled={rate.isPending}
            onChange={(e) => rate.mutate(e.target.value === '' ? null : Number(e.target.value))}
          >
            <option value="">{t('library.social.rating.none')}</option>
            {SCORES.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </label>
      </div>
      {r.global.count > 0 && (
        <details className="lib-rating-dist">
          <summary>{t('library.social.rating.distribution')}</summary>
          <ul>
            {[...SCORES].reverse().map((s) => {
              const n = r.distribution[s - 1] ?? 0;
              return (
                <li key={s}>
                  <span className="lib-rating-dist-label">{s}</span>
                  <span className="lib-rating-dist-bar" aria-hidden="true">
                    <span style={{ inlineSize: `${(n / max) * 100}%` }} />
                  </span>
                  <span className="lib-rating-dist-n">
                    <span className="lib-sr-only">
                      {t('library.social.rating.distributionRow', { score: s, n })}
                    </span>
                    <span aria-hidden="true">{n}</span>
                  </span>
                </li>
              );
            })}
          </ul>
        </details>
      )}
      {rate.isError && <Notice variant="danger">{t('library.social.rating.error')}</Notice>}
    </section>
  );
}

/* ------------------------------------------------------------ add to list */

export function AddToListButton({ seriesId }: { seriesId: string }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button variant="secondary" aria-haspopup="dialog" onClick={() => setOpen(true)}>
        <Icon name="list" size={16} />
        {t('library.social.addToList.button')}
      </Button>
      {open && <AddToListDialog seriesId={seriesId} onClose={() => setOpen(false)} />}
    </>
  );
}

function AddToListDialog({ seriesId, onClose }: { seriesId: string; onClose: () => void }) {
  const t = useT();
  const qc = useQueryClient();
  const [name, setName] = useState('');
  const lists = useQuery({
    queryKey: socialKeys.myLists(seriesId),
    queryFn: () => getMyLists(seriesId),
  });
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ['library', 'lists'] });
  };
  const toggle = useMutation({
    mutationFn: async ({ id, add }: { id: string; add: boolean }) => {
      if (add) await addToList(id, seriesId);
      else await removeFromList(id, seriesId);
    },
    onSuccess: refresh,
  });
  const create = useMutation({
    mutationFn: async () => {
      const list = await createList({ name: name.trim() });
      await addToList(list.id, seriesId);
    },
    onSuccess: () => {
      setName('');
      refresh();
    },
  });
  return (
    <Modal open onClose={onClose} title={t('library.social.addToList.title')} size="sm">
      {lists.isPending ? (
        <Skeleton height={80} />
      ) : lists.isError ? (
        <Notice variant="danger">{t('library.social.addToList.loadError')}</Notice>
      ) : (
        <>
          {lists.data.items.length === 0 ? (
            <p className="lib-muted">{t('library.social.addToList.empty')}</p>
          ) : (
            <ul className="lib-pick-list" aria-label={t('library.social.addToList.yourLists')}>
              {lists.data.items.map((l) => (
                <li key={l.id}>
                  <Checkbox
                    label={l.name}
                    checked={!!l.contains}
                    disabled={toggle.isPending}
                    onChange={(e) => toggle.mutate({ id: l.id, add: e.target.checked })}
                  />
                </li>
              ))}
            </ul>
          )}
          <form
            className="lib-inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              if (name.trim()) create.mutate();
            }}
          >
            <TextInput
              label={t('library.social.addToList.newName')}
              value={name}
              maxLength={100}
              onChange={(e) => setName(e.target.value)}
            />
            <Button type="submit" disabled={!name.trim() || create.isPending}>
              {t('library.social.addToList.create')}
            </Button>
          </form>
          {(toggle.isError || create.isError) && (
            <Notice variant="danger">{t('library.social.addToList.error')}</Notice>
          )}
        </>
      )}
    </Modal>
  );
}

/* ---------------------------------------------------------------- reviews */

export function ReviewsTab({ seriesId }: { seriesId: string }) {
  const t = useT();
  const [sort, setSort] = useState<ReviewSort>('helpful');
  const q = useInfiniteQuery({
    queryKey: socialKeys.reviews(seriesId, sort),
    queryFn: ({ pageParam }) => getReviews(seriesId, { sort, before: pageParam, limit: 10 }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_before ?? undefined,
  });
  const items = q.data?.pages.flatMap((p) => p.items) ?? [];
  const mine = items.find((r) => r.mine);
  return (
    <section className="lib-reviews" aria-label={t('library.social.reviews.section')}>
      <div className="lib-chapters-head">
        <h2 className="lib-h2">{t('library.social.reviews.title')}</h2>
        <SegmentedControl
          label={t('library.social.reviews.sort')}
          size="sm"
          value={sort}
          onValueChange={(v) => setSort(v as ReviewSort)}
          options={[
            { value: 'helpful', label: t('library.social.reviews.sortHelpful') },
            { value: 'recent', label: t('library.social.reviews.sortRecent') },
          ]}
        />
      </div>
      <ReviewForm seriesId={seriesId} existing={mine} />
      {q.isPending ? (
        <Skeleton height={120} />
      ) : q.isError ? (
        <Notice variant="danger" title={t('library.social.reviews.loadError')} />
      ) : items.length === 0 ? (
        <EmptyState
          title={t('library.social.reviews.emptyTitle')}
          description={t('library.social.reviews.emptyDescription')}
        />
      ) : (
        <>
          <ul className="lib-review-list">
            {items.map((r) => (
              <ReviewItem key={r.id} review={r} seriesId={seriesId} />
            ))}
          </ul>
          {q.hasNextPage && (
            <div className="lib-more">
              <Button
                variant="secondary"
                disabled={q.isFetchingNextPage}
                onClick={() => void q.fetchNextPage()}
              >
                {t('library.social.reviews.loadMore')}
              </Button>
            </div>
          )}
        </>
      )}
    </section>
  );
}

function ReviewForm({ seriesId, existing }: { seriesId: string; existing?: Review }) {
  const t = useT();
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const [body, setBody] = useState(existing?.body ?? '');
  const [spoiler, setSpoiler] = useState(existing?.spoiler ?? false);
  const [rating, setRating] = useState<number | null>(existing?.rating ?? null);
  const save = useMutation({
    mutationFn: () => putReview(seriesId, { body: body.trim(), rating, spoiler }),
    onSuccess: () => {
      setOpen(false);
      void qc.invalidateQueries({ queryKey: ['library', 'reviews', seriesId] });
      void qc.invalidateQueries({ queryKey: socialKeys.ratings(seriesId) });
    },
  });
  if (!open)
    return (
      <Button
        variant="secondary"
        onClick={() => {
          setBody(existing?.body ?? '');
          setSpoiler(existing?.spoiler ?? false);
          setRating(existing?.rating ?? null);
          setOpen(true);
        }}
      >
        <Icon name="edit" size={16} />
        {existing ? t('library.social.reviews.edit') : t('library.social.reviews.write')}
      </Button>
    );
  return (
    <form
      className="lib-review-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (body.trim()) save.mutate();
      }}
    >
      <TextArea
        label={t('library.social.reviews.body')}
        value={body}
        rows={5}
        maxLength={8000}
        onChange={(e) => setBody(e.target.value)}
      />
      <label className="lib-select lib-rate">
        <span>{t('library.social.reviews.rating')}</span>
        <select
          value={rating ?? ''}
          onChange={(e) => setRating(e.target.value === '' ? null : Number(e.target.value))}
        >
          <option value="">{t('library.social.rating.none')}</option>
          {SCORES.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
      </label>
      <Checkbox
        label={t('library.social.reviews.spoiler')}
        checked={spoiler}
        onChange={(e) => setSpoiler(e.target.checked)}
      />
      {save.isError && <Notice variant="danger">{t('library.social.reviews.saveError')}</Notice>}
      <div className="lib-actions">
        <Button type="submit" disabled={!body.trim() || save.isPending}>
          {t('library.social.reviews.save')}
        </Button>
        <Button variant="ghost" type="button" onClick={() => setOpen(false)}>
          {t('library.social.reviews.cancel')}
        </Button>
      </div>
    </form>
  );
}

function ReviewItem({ review, seriesId }: { review: Review; seriesId: string }) {
  const t = useT();
  const qc = useQueryClient();
  const [shown, setShown] = useState(!review.spoiler);
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ['library', 'reviews', seriesId] });
  };
  const vote = useMutation({
    mutationFn: () => setHelpful(review.id, !review.voted),
    onSuccess: refresh,
  });
  const remove = useMutation({
    mutationFn: () => deleteReview(seriesId),
    onSuccess: refresh,
  });
  return (
    <li className="lib-review">
      <header className="lib-review-head">
        <strong>{review.author_name}</strong>
        {review.rating != null && (
          <span className="lib-review-rating">
            <Icon name="star" size={16} />
            {t('library.social.reviews.ratingOf', { n: review.rating })}
          </span>
        )}
        <span className="lib-muted">{timeAgo(review.created_at_ms)}</span>
      </header>
      {shown ? (
        <p className="lib-review-body">{review.body}</p>
      ) : (
        <p className="lib-review-body">
          <span className="lib-muted">{t('library.social.reviews.spoilerHidden')}</span>{' '}
          <button type="button" className="lib-linklike" onClick={() => setShown(true)}>
            {t('library.social.reviews.showSpoiler')}
          </button>
        </p>
      )}
      <div className="lib-review-actions">
        {!review.mine && (
          <Button
            variant="ghost"
            size="sm"
            aria-pressed={review.voted}
            disabled={vote.isPending}
            onClick={() => vote.mutate()}
          >
            <Icon name="heart" size={16} />
            {t('library.social.reviews.helpful', { n: review.helpful_count })}
          </Button>
        )}
        {review.mine && (
          <>
            <span className="lib-muted">
              {t('library.social.reviews.helpfulCount', { n: review.helpful_count })}
            </span>
            <Button
              variant="ghost"
              size="sm"
              disabled={remove.isPending}
              onClick={() => remove.mutate()}
            >
              {t('library.social.reviews.delete')}
            </Button>
          </>
        )}
      </div>
    </li>
  );
}
