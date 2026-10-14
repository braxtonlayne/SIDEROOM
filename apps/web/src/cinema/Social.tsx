// Reviews and comments on a title (CIN-TTL-003, social API section 8.6). Authors come back as ids only, so other
// people show as "A member" until the API carries display names.
import { Button, Checkbox, EmptyState, Notice, Select, Skeleton, TextArea } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { formatDate, useT } from '../i18n';
import {
  deleteComment,
  deleteReview,
  getComments,
  getReviews,
  keys,
  postComment,
  putReview,
  setCommentLike,
  setReviewHelpful,
} from './api';
import type { Comment, Review } from './types';

const when = (ms: number) => formatDate(ms, { year: 'numeric', month: 'short', day: 'numeric' });

/** Spoiler text stays hidden until the reader asks. */
function Body({ text, spoiler }: { text: string; spoiler: boolean }) {
  const t = useT();
  const [shown, setShown] = useState(false);
  if (spoiler && !shown)
    return (
      <p className="cin-spoiler">
        <span className="cin-muted">{t('cinema.social.spoilerNote')}</span>
        <button type="button" className="cin-link-btn" onClick={() => setShown(true)}>
          {t('cinema.social.showSpoiler')}
        </button>
      </p>
    );
  return <p className="cin-post-body">{text}</p>;
}

function ReviewItem({ r, titleId }: { r: Review; titleId: string }) {
  const t = useT();
  const qc = useQueryClient();
  const refresh = () => void qc.invalidateQueries({ queryKey: ['cinema', 'reviews', titleId] });
  const helpful = useMutation({
    mutationFn: (on: boolean) => setReviewHelpful(r.id, on),
    onSuccess: refresh,
  });
  const remove = useMutation({ mutationFn: () => deleteReview(titleId), onSuccess: refresh });
  return (
    <li className="cin-post">
      <p className="cin-post-head">
        <strong>{r.mine ? t('cinema.social.you') : t('cinema.social.member')}</strong>
        {r.rating ? <span className="cin-rating-badge">{r.rating}/10</span> : null}
        <span className="cin-muted">{when(r.created_at_ms)}</span>
      </p>
      <Body text={r.body} spoiler={r.spoiler} />
      <p className="cin-post-actions">
        <button
          type="button"
          className="cin-link-btn"
          aria-pressed={r.voted}
          onClick={() => helpful.mutate(!r.voted)}
        >
          {r.voted ? t('cinema.social.markedHelpful') : t('cinema.social.helpful')} (
          {r.helpful_count})
        </button>
        {r.mine && (
          <button type="button" className="cin-link-btn" onClick={() => remove.mutate()}>
            {t('cinema.social.deleteReview')}
          </button>
        )}
      </p>
    </li>
  );
}

export function Reviews({ titleId }: { titleId: string }) {
  const t = useT();
  const qc = useQueryClient();
  const [sort, setSort] = useState<'helpful' | 'recent'>('helpful');
  const [text, setText] = useState('');
  const [spoiler, setSpoiler] = useState(false);
  const [score, setScore] = useState('');
  const q = useQuery({
    queryKey: keys.reviews(titleId, sort),
    queryFn: () => getReviews(titleId, sort),
  });
  const save = useMutation({
    mutationFn: () =>
      putReview(titleId, {
        body: text.trim(),
        spoiler,
        ...(score ? { rating: Number(score) } : {}),
      }),
    onSuccess: () => {
      setText('');
      setSpoiler(false);
      setScore('');
      void qc.invalidateQueries({ queryKey: ['cinema'] });
    },
  });
  const mine = q.data?.find((r) => r.mine);
  return (
    <div className="cin-social">
      <form
        className="cin-post-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (text.trim()) save.mutate();
        }}
      >
        <TextArea
          label={mine ? t('cinema.social.replaceYourReview') : t('cinema.social.writeReview')}
          value={text}
          maxLength={5000}
          rows={4}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="cin-post-form-row">
          <Select
            aria-label={t('cinema.social.ratingLabel')}
            placeholder={t('cinema.social.ratingPlaceholder')}
            value={score || '0'}
            onValueChange={(v) => setScore(v === '0' ? '' : v)}
            options={[
              { value: '0', label: t('cinema.social.noRating') },
              ...Array.from({ length: 10 }, (_, i) => ({
                value: String(i + 1),
                label: t('cinema.social.outOfTen', { n: i + 1 }),
              })),
            ]}
          />
          <Checkbox
            label={t('cinema.social.containsSpoilers')}
            checked={spoiler}
            onChange={(e) => setSpoiler(e.target.checked)}
          />
          <Button type="submit" disabled={!text.trim() || save.isPending}>
            {mine ? t('cinema.social.replaceReview') : t('cinema.social.postReview')}
          </Button>
        </div>
        {save.isError && <Notice variant="danger">{t('cinema.social.saveError')}</Notice>}
      </form>
      <div className="cin-ep-tools">
        <Select
          aria-label={t('cinema.social.sortReviews')}
          placeholder={t('cinema.social.sort')}
          value={sort}
          onValueChange={(v) => setSort(v as 'helpful' | 'recent')}
          options={[
            { value: 'helpful', label: t('cinema.social.mostHelpful') },
            { value: 'recent', label: t('cinema.social.newest') },
          ]}
        />
      </div>
      {q.isPending && <Skeleton height={80} />}
      {q.isError && <Notice variant="danger">{t('cinema.social.reviewsUnavailable')}</Notice>}
      {q.isSuccess && q.data.length === 0 && (
        <EmptyState
          title={t('cinema.social.noReviewsTitle')}
          description={t('cinema.social.noReviewsBody')}
        />
      )}
      <ul className="cin-posts">
        {q.data?.map((r) => (
          <ReviewItem key={r.id} r={r} titleId={titleId} />
        ))}
      </ul>
    </div>
  );
}

function CommentItem({ c, titleId }: { c: Comment; titleId: string }) {
  const t = useT();
  const qc = useQueryClient();
  const refresh = () => void qc.invalidateQueries({ queryKey: keys.comments(titleId) });
  const like = useMutation({
    mutationFn: (on: boolean) => setCommentLike(c.id, on),
    onSuccess: refresh,
  });
  const remove = useMutation({ mutationFn: () => deleteComment(c.id), onSuccess: refresh });
  return (
    <li className="cin-post">
      <p className="cin-post-head">
        <strong>{c.mine ? t('cinema.social.you') : t('cinema.social.member')}</strong>
        <span className="cin-muted">{when(c.created_at_ms)}</span>
      </p>
      <Body text={c.body} spoiler={c.spoiler} />
      <p className="cin-post-actions">
        <button
          type="button"
          className="cin-link-btn"
          aria-pressed={c.liked}
          onClick={() => like.mutate(!c.liked)}
        >
          {c.liked ? t('cinema.social.liked') : t('cinema.social.like')} ({c.like_count})
        </button>
        {c.mine && (
          <button type="button" className="cin-link-btn" onClick={() => remove.mutate()}>
            {t('cinema.social.deleteComment')}
          </button>
        )}
      </p>
    </li>
  );
}

export function Comments({ titleId }: { titleId: string }) {
  const t = useT();
  const qc = useQueryClient();
  const [text, setText] = useState('');
  const [spoiler, setSpoiler] = useState(false);
  const q = useQuery({ queryKey: keys.comments(titleId), queryFn: () => getComments(titleId) });
  const post = useMutation({
    mutationFn: () => postComment(titleId, { body: text.trim(), spoiler }),
    onSuccess: () => {
      setText('');
      setSpoiler(false);
      void qc.invalidateQueries({ queryKey: keys.comments(titleId) });
    },
  });
  return (
    <div className="cin-social">
      <form
        className="cin-post-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (text.trim()) post.mutate();
        }}
      >
        <TextArea
          label={t('cinema.social.addComment')}
          value={text}
          maxLength={2000}
          rows={3}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="cin-post-form-row">
          <Checkbox
            label={t('cinema.social.containsSpoilers')}
            checked={spoiler}
            onChange={(e) => setSpoiler(e.target.checked)}
          />
          <Button type="submit" disabled={!text.trim() || post.isPending}>
            {t('cinema.social.postComment')}
          </Button>
        </div>
        {post.isError && <Notice variant="danger">{t('cinema.social.postError')}</Notice>}
      </form>
      {q.isPending && <Skeleton height={80} />}
      {q.isError && <Notice variant="danger">{t('cinema.social.commentsUnavailable')}</Notice>}
      {q.isSuccess && q.data.length === 0 && (
        <EmptyState title={t('cinema.social.noCommentsTitle')} />
      )}
      <ul className="cin-posts">
        {q.data?.map((c) => (
          <CommentItem key={c.id} c={c} titleId={titleId} />
        ))}
      </ul>
    </div>
  );
}
