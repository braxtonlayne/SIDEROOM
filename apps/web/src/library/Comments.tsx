// Comments (LIB-SOC-003, LIB-RDR-014): a threaded list with one reply level, spoiler tags, likes, edit and
// delete for your own, and reports. One component serves the series page's Comments tab and the reader's
// comments drawer.
import { Button, Checkbox, EmptyState, Icon, Notice, Skeleton, TextArea } from '@sideroom/ui';
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useT } from '../i18n';
import {
  commentKeys,
  deleteComment,
  editComment,
  getComments,
  getReplies,
  likeComment,
  postComment,
  reportComment,
  type CommentTarget,
  type LibComment,
} from './p13api';
import { timeAgo } from './ui';

const MAX = 2000;

export function CommentsSection({ target }: { target: CommentTarget }) {
  const t = useT();
  const key = commentKeys.list(target);
  const q = useInfiniteQuery({
    queryKey: key,
    queryFn: ({ pageParam }) => getComments(target, { before: pageParam, limit: 20 }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_before ?? undefined,
  });
  const items = q.data?.pages.flatMap((p) => p.items) ?? [];
  const total = q.data?.pages[0]?.total ?? 0;
  return (
    <section className="lib-comments" aria-label={t('library.comments.section')}>
      <h2 className="lib-h2">{t('library.comments.count', { n: total })}</h2>
      <CommentForm target={target} />
      {q.isPending ? (
        <Skeleton height={120} />
      ) : q.isError ? (
        <Notice variant="danger" title={t('library.comments.loadError')} />
      ) : items.length === 0 ? (
        <EmptyState
          title={t('library.comments.emptyTitle')}
          description={t('library.comments.emptyDescription')}
        />
      ) : (
        <>
          <ul className="lib-comment-list">
            {items.map((c) => (
              <CommentItem key={c.id} comment={c} target={target} />
            ))}
          </ul>
          {q.hasNextPage && (
            <div className="lib-more">
              <Button
                variant="secondary"
                disabled={q.isFetchingNextPage}
                onClick={() => void q.fetchNextPage()}
              >
                {t('library.comments.loadMore')}
              </Button>
            </div>
          )}
        </>
      )}
    </section>
  );
}

function CommentForm({
  target,
  parentId,
  onDone,
  initial,
  editId,
}: {
  target: CommentTarget;
  parentId?: string;
  onDone?: () => void;
  initial?: { body: string; spoiler: boolean };
  editId?: string;
}) {
  const t = useT();
  const qc = useQueryClient();
  const [body, setBody] = useState(initial?.body ?? '');
  const [spoiler, setSpoiler] = useState(initial?.spoiler ?? false);
  const save = useMutation({
    mutationFn: () =>
      editId
        ? editComment(editId, { body: body.trim(), spoiler })
        : postComment(target, { body: body.trim(), spoiler, parent_id: parentId }),
    onSuccess: () => {
      setBody('');
      setSpoiler(false);
      void qc.invalidateQueries({ queryKey: commentKeys.list(target) });
      if (parentId) void qc.invalidateQueries({ queryKey: commentKeys.replies(parentId) });
      onDone?.();
    },
  });
  return (
    <form
      className="lib-comment-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (body.trim()) save.mutate();
      }}
    >
      <TextArea
        label={parentId ? t('library.comments.replyLabel') : t('library.comments.bodyLabel')}
        value={body}
        rows={parentId || editId ? 2 : 3}
        maxLength={MAX}
        onChange={(e) => setBody(e.target.value)}
      />
      <Checkbox
        label={t('library.comments.spoiler')}
        checked={spoiler}
        onChange={(e) => setSpoiler(e.target.checked)}
      />
      {save.isError && <Notice variant="danger">{t('library.comments.saveError')}</Notice>}
      <div className="lib-actions">
        <Button type="submit" size="sm" disabled={!body.trim() || save.isPending}>
          {editId ? t('library.comments.saveEdit') : t('library.comments.post')}
        </Button>
        {onDone && (
          <Button variant="ghost" size="sm" type="button" onClick={onDone}>
            {t('library.comments.cancel')}
          </Button>
        )}
      </div>
    </form>
  );
}

function CommentItem({
  comment,
  target,
  reply,
}: {
  comment: LibComment;
  target: CommentTarget;
  reply?: boolean;
}) {
  const t = useT();
  const qc = useQueryClient();
  const [shown, setShown] = useState(!comment.spoiler);
  const [replying, setReplying] = useState(false);
  const [editing, setEditing] = useState(false);
  const [reporting, setReporting] = useState(false);
  const [reason, setReason] = useState('');
  const [open, setOpen] = useState(false);
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: commentKeys.list(target) });
    if (comment.parent_id)
      void qc.invalidateQueries({ queryKey: commentKeys.replies(comment.parent_id) });
  };
  const like = useMutation({
    mutationFn: () => likeComment(comment.id, !comment.liked),
    onSuccess: refresh,
  });
  const remove = useMutation({ mutationFn: () => deleteComment(comment.id), onSuccess: refresh });
  const report = useMutation({
    mutationFn: () => reportComment(comment.id, reason.trim()),
    onSuccess: () => {
      setReporting(false);
      setReason('');
    },
  });
  return (
    <li className="lib-comment" data-reply={reply || undefined}>
      <header className="lib-review-head">
        <strong>{comment.author_name}</strong>
        <span className="lib-muted">{timeAgo(comment.created_at_ms)}</span>
        {comment.edited_at_ms ? (
          <span className="lib-muted">{t('library.comments.edited')}</span>
        ) : null}
      </header>
      {editing ? (
        <CommentForm
          target={target}
          editId={comment.id}
          initial={{ body: comment.body, spoiler: comment.spoiler }}
          onDone={() => setEditing(false)}
        />
      ) : shown ? (
        <p className="lib-review-body">{comment.body}</p>
      ) : (
        <p className="lib-review-body">
          <span className="lib-muted">{t('library.comments.spoilerHidden')}</span>{' '}
          <button type="button" className="lib-linklike" onClick={() => setShown(true)}>
            {t('library.comments.showSpoiler')}
          </button>
        </p>
      )}
      <div className="lib-review-actions">
        <Button
          variant="ghost"
          size="sm"
          aria-pressed={comment.liked}
          disabled={like.isPending}
          onClick={() => like.mutate()}
        >
          <Icon name="heart" size={16} />
          {t('library.comments.likes', { n: comment.like_count })}
        </Button>
        {!reply && (
          <Button variant="ghost" size="sm" onClick={() => setReplying((v) => !v)}>
            {t('library.comments.reply')}
          </Button>
        )}
        {comment.mine ? (
          <>
            <Button variant="ghost" size="sm" onClick={() => setEditing((v) => !v)}>
              {t('library.comments.edit')}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={remove.isPending}
              onClick={() => remove.mutate()}
            >
              {t('library.comments.delete')}
            </Button>
          </>
        ) : (
          <Button variant="ghost" size="sm" onClick={() => setReporting((v) => !v)}>
            {t('library.comments.report')}
          </Button>
        )}
      </div>
      {reporting && (
        <form
          className="lib-comment-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (reason.trim()) report.mutate();
          }}
        >
          <TextArea
            label={t('library.comments.reportReason')}
            value={reason}
            rows={2}
            maxLength={500}
            onChange={(e) => setReason(e.target.value)}
          />
          {report.isError && (
            <Notice variant="danger">{t('library.comments.reportError')}</Notice>
          )}
          <div className="lib-actions">
            <Button type="submit" size="sm" disabled={!reason.trim() || report.isPending}>
              {t('library.comments.reportSend')}
            </Button>
            <Button variant="ghost" size="sm" type="button" onClick={() => setReporting(false)}>
              {t('library.comments.cancel')}
            </Button>
          </div>
        </form>
      )}
      {replying && (
        <CommentForm
          target={target}
          parentId={comment.id}
          onDone={() => {
            setReplying(false);
            setOpen(true);
          }}
        />
      )}
      {!reply && comment.reply_count > 0 && (
        <>
          <button type="button" className="lib-linklike" onClick={() => setOpen((v) => !v)}>
            {open
              ? t('library.comments.hideReplies')
              : t('library.comments.showReplies', { n: comment.reply_count })}
          </button>
          {open && <Replies commentId={comment.id} target={target} />}
        </>
      )}
    </li>
  );
}

function Replies({ commentId, target }: { commentId: string; target: CommentTarget }) {
  const t = useT();
  const q = useQuery({
    queryKey: commentKeys.replies(commentId),
    queryFn: () => getReplies(commentId, { limit: 50 }),
  });
  if (q.isPending) return <Skeleton height={48} />;
  if (q.isError) return <Notice variant="danger" title={t('library.comments.loadError')} />;
  return (
    <ul className="lib-comment-list lib-comment-replies">
      {q.data.items.map((c) => (
        <CommentItem key={c.id} comment={c} target={target} reply />
      ))}
    </ul>
  );
}
