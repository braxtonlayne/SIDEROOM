// The reviewer workspace (REV-RV): the queue of submissions the staff member may review, and a
// review page per submission with the metadata and a diff since the previous submit, per-item
// decisions, notes, and the decision bar (approve, request changes, reject). Docs: spec
// submissions-and-review section 5; every decision is also written to the staff access log.
import {
  Badge,
  Button,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  Modal,
  Notice,
  Select,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useLocation } from '@tanstack/react-router';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { formatTime, type MessageKey, useT } from '../../i18n';
import * as rv from '../../review/api';
import {
  DuplicatePanel,
  ManifestDiff,
  PackagePreview,
  VideoProxyPlayer,
} from '../../review/ReviewExtras';
import {
  ChangeList,
  ItemChip,
  PreviewFacts,
  StateChip,
  Timeline,
  ago,
  chapterName,
  fmtWhen,
  previewDiff,
  snapshots,
  useSubmissionUpdates,
} from '../../review/shared';
import type {
  LibraryPreview,
  ReviewPage,
  Submission,
  SubmissionDetail,
  SubmissionItem,
} from '../../review/types';
import {
  ActionStatus,
  Async,
  ReasonDialog,
  errorText,
  useAction,
  useAdminViewer,
  useResource,
} from '../ui';

const PAGE = 25;

/** The `manifest` an app or package kind puts in its preview, if any. */
const manifestOf = (p: unknown): unknown =>
  p && typeof p === 'object' ? (p as Record<string, unknown>).manifest : undefined;

/** The theme CSS a package kind puts in its preview (`package.theme_css`), if any. */
const packageCss = (p: unknown): string | undefined => {
  const pkg = p && typeof p === 'object' ? (p as Record<string, unknown>).package : undefined;
  const css =
    pkg && typeof pkg === 'object' ? (pkg as Record<string, unknown>).theme_css : undefined;
  return typeof css === 'string' && css ? css : undefined;
};

type TFn = ReturnType<typeof useT>;
type OptionKeys = { value: string; label: MessageKey }[];
const localize = (t: TFn, list: OptionKeys) =>
  list.map((o) => ({ value: o.value, label: t(o.label) }));

const STATE_FILTERS: OptionKeys = [
  { value: 'waiting', label: 'admin.review.stateWaiting' },
  { value: 'submitted', label: 'admin.review.stateSubmitted' },
  { value: 'in_review', label: 'admin.review.stateInReview' },
  { value: 'changes_requested', label: 'admin.review.stateChangesRequested' },
  { value: 'approved', label: 'admin.review.stateApproved' },
  { value: 'published', label: 'admin.review.statePublished' },
  { value: 'rejected', label: 'admin.review.stateRejected' },
  { value: 'all', label: 'admin.review.stateAll' },
];
const ALL_STATES =
  'draft,submitted,in_review,changes_requested,approved,rejected,published,withdrawn';
const CLAIM_FILTERS: OptionKeys = [
  { value: '', label: 'admin.review.claimAnyone' },
  { value: 'mine', label: 'admin.review.claimMine' },
  { value: 'unclaimed', label: 'admin.review.claimNone' },
];
const SORTS: OptionKeys = [
  { value: 'submitted:asc', label: 'admin.review.sortOldest' },
  { value: 'submitted:desc', label: 'admin.review.sortNewest' },
  { value: 'updated:desc', label: 'admin.review.sortUpdated' },
  { value: 'title:asc', label: 'admin.review.sortTitle' },
];
const AGES: OptionKeys = [
  { value: '', label: 'admin.review.ageAny' },
  { value: '24', label: 'admin.review.ageDay' },
  { value: '72', label: 'admin.review.age3Days' },
  { value: '168', label: 'admin.review.ageWeek' },
];
const KINDS: OptionKeys = [
  { value: '', label: 'admin.review.kindAll' },
  { value: 'library.series', label: 'admin.review.kindSeries' },
  { value: 'library.items', label: 'admin.review.kindItems' },
];

export const REJECT_TEMPLATES = [
  { value: 'Duplicate of an existing title', label: 'Duplicate of an existing title' },
  { value: 'Poor image quality', label: 'Poor image quality' },
  { value: 'Wrong or missing metadata', label: 'Wrong or missing metadata' },
  { value: 'Content we cannot host', label: 'Content we cannot host' },
  { value: 'Other', label: 'Other (explain below)' },
];

/** The `value` is the reason text sent to the API (kept English); only the label is translated. */
const REJECT_LABEL_KEYS: Record<string, MessageKey> = {
  'Duplicate of an existing title': 'admin.review.rejectDuplicate',
  'Poor image quality': 'admin.review.rejectQuality',
  'Wrong or missing metadata': 'admin.review.rejectMetadata',
  'Content we cannot host': 'admin.review.rejectContent',
  Other: 'admin.review.rejectOther',
};

/** Who a lease or an event actor is, from the reviewer's point of view. */
const who = (t: TFn, me: string, id: string | null) =>
  id === null
    ? t('admin.review.someone')
    : id === me
      ? t('admin.review.you')
      : t('admin.review.account', { id });

/** The Library submissions workspace (a tab of the Review Queues hub). */
export function LibraryReviewPanel() {
  // `/admin/review?submission=<id>[&reject=1]` is where the `#review-queue` card sends reviewers (G-68).
  const search = useLocation({ select: (l) => l.searchStr });
  const link = useMemo(() => {
    const q = new URLSearchParams(search);
    const submission = q.get('submission');
    return submission && /^\d+$/.test(submission)
      ? {
          id: submission,
          dialog:
            q.get('reject') === '1'
              ? ('reject' as const)
              : q.get('changes') === '1'
                ? ('changes' as const)
                : null,
        }
      : null;
    // Read once, when the panel opens; later changes of the queue do not touch the address.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [open, setOpen] = useState<string | null>(link?.id ?? null);
  return open ? (
    <ReviewDetail
      id={open}
      dialogOnOpen={open === link?.id ? (link.dialog ?? null) : null}
      onBack={() => setOpen(null)}
    />
  ) : (
    <ReviewQueue onOpen={setOpen} />
  );
}

// ---- queue ----

function ReviewQueue({ onOpen }: { onOpen: (id: string) => void }) {
  const t = useT();
  const viewer = useAdminViewer();
  const [state, setState] = useState('waiting');
  const [claimed, setClaimed] = useState('');
  const [sort, setSort] = useState('submitted:asc');
  const [age, setAge] = useState('');
  const [kind, setKind] = useState('');
  const [text, setText] = useState('');
  const [q, setQ] = useState('');
  const [offset, setOffset] = useState(0);
  const [sortKey, order] = sort.split(':') as ['submitted' | 'updated' | 'title', 'asc' | 'desc'];

  const key = `queue:${state}:${claimed}:${sort}:${age}:${kind}:${q}:${offset}`;
  const res = useResource(key, () =>
    rv.reviewQueue({
      ...(state === 'waiting' ? {} : { state: state === 'all' ? ALL_STATES : state }),
      ...(claimed ? { claimed: claimed as 'mine' | 'unclaimed' } : {}),
      sort: sortKey,
      order,
      ...(age ? { min_age_hours: Number(age) } : {}),
      ...(kind ? { kind } : {}),
      ...(q ? { q } : {}),
      limit: PAGE,
      offset,
    }),
  );
  useSubmissionUpdates(() => void res.reload());
  const reset =
    <T,>(set: (v: T) => void) =>
    (v: T) => {
      set(v);
      setOffset(0);
    };

  return (
    <>
      <form
        className="ss-toolbar"
        role="search"
        aria-label={t('admin.review.filterQueue')}
        onSubmit={(e) => {
          e.preventDefault();
          setQ(text.trim());
          setOffset(0);
        }}
      >
        <TextInput
          label={t('admin.review.titleContains')}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <Select
          aria-label={t('admin.review.filterByState')}
          options={localize(t, STATE_FILTERS)}
          value={state}
          onValueChange={reset(setState)}
        />
        <Select
          aria-label={t('admin.review.filterByKind')}
          options={localize(t, KINDS)}
          value={kind}
          onValueChange={reset(setKind)}
        />
        <Select
          aria-label={t('admin.review.filterByClaim')}
          options={localize(t, CLAIM_FILTERS)}
          value={claimed}
          onValueChange={reset(setClaimed)}
        />
        <Select
          aria-label={t('admin.review.filterByWaiting')}
          options={localize(t, AGES)}
          value={age}
          onValueChange={reset(setAge)}
        />
        <Select
          aria-label={t('admin.review.sort')}
          options={localize(t, SORTS)}
          value={sort}
          onValueChange={reset(setSort)}
        />
        <Button type="submit">{t('admin.review.filter')}</Button>
      </form>
      <Async res={res} label={t('admin.review.loadingQueue')}>
        {(page) =>
          page.items.length === 0 ? (
            <EmptyState
              title={t('admin.review.emptyTitle')}
              description={t('admin.review.emptyDescription')}
            />
          ) : (
            <>
              <div className="rv-table-wrap">
                <table className="rv-table" aria-label={t('admin.review.queueTable')}>
                  <thead>
                    <tr>
                      <th scope="col">{t('admin.review.colSubmission')}</th>
                      <th scope="col">{t('admin.review.colSubmitter')}</th>
                      <th scope="col">{t('admin.review.colState')}</th>
                      <th scope="col">{t('admin.review.colWaiting')}</th>
                      <th scope="col">{t('admin.review.colClaim')}</th>
                      <th scope="col">
                        <span className="sr-only-label">{t('admin.review.open')}</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {page.items.map((s) => (
                      <tr key={s.id}>
                        <td>
                          <strong>{s.title}</strong>
                          <div className="rv-muted">
                            {s.kind === 'library.items'
                              ? t('admin.review.kindItems')
                              : t('admin.review.kindSeries')}
                          </div>
                        </td>
                        <td className="adm-mono">
                          {s.submitter_id ?? t('admin.review.deletedAccount')}
                        </td>
                        <td>
                          <StateChip state={s.state} />
                        </td>
                        <td>{s.submitted_at_ms ? ago(s.submitted_at_ms) : ''}</td>
                        <td>{claimText(t, s, viewer.userId)}</td>
                        <td>
                          <Button
                            size="sm"
                            aria-label={t('admin.review.openAria', { title: s.title })}
                            onClick={() => onOpen(s.id)}
                          >
                            {t('admin.review.open')}
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="rv-pager">
                <span className="rv-muted" aria-live="polite">
                  {t('admin.review.pager', {
                    from: offset + 1,
                    to: offset + page.items.length,
                    total: page.total,
                  })}
                </span>
                <div className="rv-actions">
                  <Button
                    variant="secondary"
                    disabled={offset === 0}
                    onClick={() => setOffset(Math.max(0, offset - PAGE))}
                  >
                    {t('admin.review.previous')}
                  </Button>
                  <Button
                    variant="secondary"
                    disabled={offset + page.items.length >= page.total}
                    onClick={() => setOffset(offset + PAGE)}
                  >
                    {t('admin.review.next')}
                  </Button>
                </div>
              </div>
            </>
          )
        }
      </Async>
    </>
  );
}

function claimText(t: TFn, s: Submission, me: string): string {
  if (!s.claimed_by || !s.claim_expires_at_ms || s.claim_expires_at_ms <= Date.now())
    return t('admin.review.free');
  const by =
    s.claimed_by === me ? t('admin.review.you') : t('admin.review.account', { id: s.claimed_by });
  return t('admin.review.claimUntil', {
    by,
    time: formatTime(s.claim_expires_at_ms),
  });
}

// ---- review page ----

function ReviewDetail({
  id,
  onBack,
  dialogOnOpen = null,
}: {
  id: string;
  onBack: () => void;
  dialogOnOpen?: 'reject' | 'changes' | null;
}) {
  const t = useT();
  const viewer = useAdminViewer();
  const res = useResource(`review:${id}`, () => rv.reviewGet(id));
  const preview = useResource(
    `review-preview:${id}`,
    async () => (await rv.reviewPreview(id)).preview,
  );
  const action = useAction();
  const [dialog, setDialog] = useState<'approve' | 'changes' | 'reject' | 'takeover' | null>(null);
  const [pages, setPages] = useState<{ item: SubmissionItem; pages: ReviewPage[] } | null>(null);

  const reload = useCallback(() => {
    void res.reload();
    void preview.reload();
  }, [res, preview]);
  useSubmissionUpdates(reload);

  // Re-render when the lease runs out.
  const [, tick] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => tick((n) => n + 1), 30_000);
    return () => clearInterval(timer);
  }, []);

  // Coming from a review card's Reject or Request changes button: take the claim if it is free, then open that dialog.
  const autoReject = useRef(dialogOnOpen);
  useEffect(() => {
    const d = res.data;
    if (!autoReject.current || !d) return;
    const s = d.submission;
    const live = !!s.claimed_by && (s.claim_expires_at_ms ?? 0) > Date.now();
    if (live && s.claimed_by === viewer.userId && s.state === 'in_review') {
      const which = autoReject.current;
      autoReject.current = null;
      setDialog(which);
    } else if (s.state === 'submitted' || (s.state === 'in_review' && !live)) {
      autoReject.current = null;
      void action
        .run(() => rv.claim(id), t('admin.review.claimedNotice'))
        .then((claimed) => {
          if (claimed) {
            res.set(claimed);
            autoReject.current = dialogOnOpen;
          }
        });
    } else {
      autoReject.current = null;
    }
  }, [res, res.data, viewer.userId, id, action, t]);

  const apply = async (fn: () => Promise<SubmissionDetail>, done: string) => {
    const d = await action.run(fn, done);
    if (d) {
      res.set(d);
      void preview.reload();
    }
    return d;
  };

  return (
    <Async res={res} label={t('admin.review.loadingSubmission')}>
      {(d) => {
        const s = d.submission;
        const live = !!s.claimed_by && (s.claim_expires_at_ms ?? 0) > Date.now();
        const mine = live && s.claimed_by === viewer.userId;
        const heldByOther = live && !mine;
        const canDecide = mine && s.state === 'in_review';
        const failed =
          s.state === 'approved'
            ? d.events.filter((e) => e.kind === 'publish_failed').at(-1)
            : undefined;
        const snaps = snapshots(d.events);
        const last = snaps.at(-1);
        const prev = snaps.at(-2);
        const items = [...d.items].sort((a, b) => a.position - b.position);
        const pendingLeft = items.filter((i) => i.state === 'pending').length;
        const approveCount = items.filter((i) => i.state !== 'rejected').length;

        return (
          <>
            <div className="rv-row" style={{ marginBottom: 12 }}>
              <Button variant="secondary" onClick={onBack}>
                {t('admin.review.backToQueue')}
              </Button>
            </div>
            <div className="rv-head">
              <h2>{s.title}</h2>
              <StateChip state={s.state} />
              <Badge variant="neutral">{d.kind_label}</Badge>
            </div>
            <dl className="rv-kv" aria-label={t('admin.review.detailsLabel')}>
              <div className="rv-kv-row">
                <dt>{t('admin.review.dtSubmitter')}</dt>
                <dd className="adm-mono">{s.submitter_id ?? t('admin.review.deletedAccount')}</dd>
              </div>
              <div className="rv-kv-row">
                <dt>{t('admin.review.dtSubmitted')}</dt>
                <dd>{fmtWhen(s.submitted_at_ms) || t('admin.review.notYet')}</dd>
              </div>
              <div className="rv-kv-row">
                <dt>{t('admin.review.dtClaim')}</dt>
                <dd>{claimText(t, s, viewer.userId)}</dd>
              </div>
              {s.summary && (
                <div className="rv-kv-row">
                  <dt>{t('admin.review.dtSubmitterNote')}</dt>
                  <dd>{s.summary}</dd>
                </div>
              )}
            </dl>

            <ActionStatus error={action.error} done={action.done} />
            {failed && (
              <Notice
                variant="warning"
                title={t('admin.review.publishFailed')}
                className="ss-status"
              >
                {failed.body}
                <div style={{ marginTop: 8 }}>
                  <Button
                    size="sm"
                    loading={action.busy}
                    onClick={() => void apply(() => rv.publish(id), t('admin.review.published'))}
                  >
                    {t('admin.review.tryPublishAgain')}
                  </Button>
                </div>
              </Notice>
            )}

            <div
              className="rv-actions"
              role="group"
              aria-label={t('admin.review.decisionBar')}
              style={{ margin: '16px 0' }}
            >
              {(s.state === 'submitted' || (s.state === 'in_review' && !live)) && (
                <Button
                  loading={action.busy}
                  onClick={() => void apply(() => rv.claim(id), t('admin.review.claimedNotice'))}
                >
                  {t('admin.review.claim')}
                </Button>
              )}
              {s.state === 'in_review' && heldByOther && viewer.can('PUBLISH_READING') && (
                <Button variant="secondary" onClick={() => setDialog('takeover')}>
                  {t('admin.review.takeOver')}
                </Button>
              )}
              {mine && s.state === 'in_review' && (
                <Button
                  variant="secondary"
                  loading={action.busy}
                  onClick={() => void apply(() => rv.release(id), t('admin.review.released'))}
                >
                  {t('admin.review.release')}
                </Button>
              )}
              <Button disabled={!canDecide} onClick={() => setDialog('approve')}>
                {t('admin.review.approve')}
              </Button>
              <Button
                variant="secondary"
                disabled={!canDecide}
                onClick={() => setDialog('changes')}
              >
                {t('admin.review.requestChanges')}
              </Button>
              <Button variant="danger" disabled={!canDecide} onClick={() => setDialog('reject')}>
                {t('admin.review.reject')}
              </Button>
            </div>
            {!canDecide && s.state === 'in_review' && (
              <p className="rv-muted">
                {heldByOther
                  ? t('admin.review.heldByOther', {
                      id: s.claimed_by ?? '',
                      when: fmtWhen(s.claim_expires_at_ms),
                    })
                  : t('admin.review.claimToDecide')}
              </p>
            )}

            <section className="rv-panel" aria-labelledby="rv-changes-h">
              <h3 id="rv-changes-h">{t('admin.review.whatChanged')}</h3>
              {last && prev ? (
                <ChangeList changes={previewDiff(prev, last)} />
              ) : (
                <p className="rv-muted">{t('admin.review.firstSubmission')}</p>
              )}
            </section>

            <section className="rv-panel" aria-labelledby="rv-meta-h">
              <h3 id="rv-meta-h">{t('admin.review.metadata')}</h3>
              <Async res={preview} label={t('admin.review.loadingPreview')}>
                {(p: LibraryPreview) => (
                  <>
                    <PreviewFacts preview={p} />
                    {!p.new_series && (
                      <p className="rv-muted">
                        {t('admin.review.existingChapters', { count: p.existing_chapters })}
                      </p>
                    )}
                  </>
                )}
              </Async>
            </section>

            <DuplicatePanel id={id} canResolve={canDecide} />
            <VideoProxyPlayer id={id} />
            {manifestOf(last) !== undefined && (
              <ManifestDiff before={manifestOf(prev)} after={manifestOf(last)} />
            )}
            {packageCss(preview.data) && <PackagePreview css={packageCss(preview.data)!} />}

            <section className="rv-panel" aria-labelledby="rv-items-h">
              <h3 id="rv-items-h">{t('admin.review.chapters', { count: items.length })}</h3>
              {items.length === 0 ? (
                <p className="rv-muted">{t('admin.review.noItems')}</p>
              ) : (
                <div className="rv-table-wrap">
                  <table className="rv-table" aria-label={t('admin.review.itemsTable')}>
                    <thead>
                      <tr>
                        <th scope="col">{t('admin.review.colItem')}</th>
                        <th scope="col">{t('admin.review.colDecision')}</th>
                        <th scope="col">{t('admin.review.colNote')}</th>
                        <th scope="col">
                          <span className="sr-only-label">{t('admin.review.colActions')}</span>
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {items.map((it) => (
                        <ItemRow
                          key={it.id}
                          item={it}
                          preview={preview.data}
                          canDecide={canDecide}
                          busy={action.busy}
                          onDecide={(state, note) =>
                            void apply(
                              () => rv.decideItem(id, it.id, state, note),
                              t(
                                state === 'approved'
                                  ? 'admin.review.itemApproved'
                                  : state === 'rejected'
                                    ? 'admin.review.itemRejected'
                                    : 'admin.review.itemPending',
                                { label: it.label },
                              ),
                            )
                          }
                          onPages={async () => {
                            try {
                              const m = await rv.reviewPages(id, it.ref_id);
                              setPages({ item: it, pages: m.pages });
                            } catch (e) {
                              action.setError(errorText(e, t('admin.review.loadPagesError')));
                            }
                          }}
                        />
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </section>

            <section className="rv-panel" aria-labelledby="rv-time-h">
              <h3 id="rv-time-h">{t('admin.review.timelineTitle')}</h3>
              <Timeline events={d.events} actorLabel={(a) => who(t, viewer.userId, a)} />
              <NoteForm
                busy={action.busy}
                onSend={(body, internal) =>
                  apply(() => rv.reviewComment(id, body, internal), t('admin.review.noteAdded'))
                }
              />
            </section>

            <ConfirmDialog
              open={dialog === 'approve'}
              title={t('admin.review.approveTitle')}
              description={
                pendingLeft > 0
                  ? t('admin.review.approveSome', { approved: approveCount, total: items.length })
                  : t('admin.review.approveAll', { approved: approveCount })
              }
              confirmLabel={t('admin.review.approveConfirm')}
              loading={action.busy}
              onCancel={() => setDialog(null)}
              onConfirm={() =>
                void apply(() => rv.approve(id), t('admin.review.approved')).then(() =>
                  setDialog(null),
                )
              }
            />
            <ConfirmDialog
              open={dialog === 'takeover'}
              title={t('admin.review.takeOver')}
              description={t('admin.review.takeOverDescription', { id: s.claimed_by ?? '' })}
              confirmLabel={t('admin.review.takeOverConfirm')}
              loading={action.busy}
              onCancel={() => setDialog(null)}
              onConfirm={() =>
                void apply(() => rv.claim(id, true), t('admin.review.tookOver')).then(() =>
                  setDialog(null),
                )
              }
            />
            <ReasonDialog
              open={dialog === 'changes'}
              title={t('admin.review.requestChanges')}
              description={t('admin.review.changesDescription')}
              confirmLabel={t('admin.review.sendBack')}
              reasonLabel={t('admin.review.changesReason')}
              onCancel={() => setDialog(null)}
              onConfirm={async (reason) => {
                const out = await rv.requestChanges(id, reason);
                res.set(out);
                setDialog(null);
              }}
            />
            <RejectDialog
              open={dialog === 'reject'}
              onCancel={() => setDialog(null)}
              onConfirm={async (reason) => {
                const out = await rv.reject(id, reason);
                res.set(out);
                setDialog(null);
              }}
            />
            <Modal
              open={!!pages}
              onClose={() => setPages(null)}
              title={pages ? pages.item.label : ''}
              size="lg"
              footer={<Button onClick={() => setPages(null)}>{t('admin.review.close')}</Button>}
            >
              {pages && (
                <div className="rv-pages">
                  {pages.pages.slice(0, 24).map((p) => (
                    <img
                      key={p.index}
                      src={p.url}
                      alt={t('admin.review.pageAlt', { n: p.index + 1 })}
                      width={p.width}
                      height={p.height}
                      loading="lazy"
                    />
                  ))}
                </div>
              )}
              {pages && pages.pages.length > 24 && (
                <p className="rv-muted">
                  {t('admin.review.showingPages', { total: pages.pages.length })}
                </p>
              )}
            </Modal>
          </>
        );
      }}
    </Async>
  );
}

function ItemRow({
  item,
  preview,
  canDecide,
  busy,
  onDecide,
  onPages,
}: {
  item: SubmissionItem;
  preview: LibraryPreview | undefined;
  canDecide: boolean;
  busy: boolean;
  onDecide: (state: 'approved' | 'rejected' | 'pending', note?: string) => void;
  onPages: () => void;
}) {
  const t = useT();
  const [rejecting, setRejecting] = useState(false);
  const [note, setNote] = useState('');
  const ch = preview?.chapters.find((c) => c.id === item.ref_id);
  return (
    <>
      <tr>
        <td>
          <strong>{ch ? chapterName(ch) : item.label}</strong>
          {ch && (
            <div className="rv-muted">
              {ch.page_count}{' '}
              {ch.format === 'images' ? t('admin.review.pagesWord') : ch.format.toUpperCase()}
            </div>
          )}
        </td>
        <td>
          <ItemChip state={item.state} />
        </td>
        <td>{item.note}</td>
        <td>
          <div className="rv-actions">
            {ch?.format === 'images' && (
              <Button
                size="sm"
                variant="secondary"
                aria-label={t('admin.review.viewPagesAria', { label: item.label })}
                onClick={onPages}
              >
                {t('admin.review.pages')}
              </Button>
            )}
            {canDecide && item.state !== 'approved' && (
              <Button
                size="sm"
                aria-label={t('admin.review.approveItemAria', { label: item.label })}
                disabled={busy}
                onClick={() => onDecide('approved')}
              >
                {t('admin.review.approve')}
              </Button>
            )}
            {canDecide && item.state !== 'rejected' && (
              <Button
                size="sm"
                variant="danger"
                aria-label={t('admin.review.rejectItemAria', { label: item.label })}
                disabled={busy}
                onClick={() => setRejecting(true)}
              >
                {t('admin.review.reject')}
              </Button>
            )}
            {canDecide && item.state !== 'pending' && (
              <Button
                size="sm"
                variant="ghost"
                aria-label={t('admin.review.resetItemAria', { label: item.label })}
                disabled={busy}
                onClick={() => onDecide('pending')}
              >
                {t('admin.review.reset')}
              </Button>
            )}
          </div>
        </td>
      </tr>
      {rejecting && (
        <tr>
          <td colSpan={4}>
            <form
              className="rv-form"
              onSubmit={(e) => {
                e.preventDefault();
                if (!note.trim()) return;
                onDecide('rejected', note.trim());
                setRejecting(false);
                setNote('');
              }}
            >
              <TextArea
                label={t('admin.review.whyRejected', { label: item.label })}
                value={note}
                rows={2}
                onChange={(e) => setNote(e.target.value)}
              />
              <div className="rv-actions">
                <Button type="submit" variant="danger" disabled={!note.trim()}>
                  {t('admin.review.rejectItem')}
                </Button>
                <Button type="button" variant="ghost" onClick={() => setRejecting(false)}>
                  {t('admin.review.cancel')}
                </Button>
              </div>
            </form>
          </td>
        </tr>
      )}
    </>
  );
}

function NoteForm({
  busy,
  onSend,
}: {
  busy: boolean;
  onSend: (body: string, internal: boolean) => Promise<unknown>;
}) {
  const t = useT();
  const [body, setBody] = useState('');
  const [internal, setInternal] = useState(false);
  return (
    <form
      className="rv-form"
      onSubmit={(e) => {
        e.preventDefault();
        const text = body.trim();
        if (!text) return;
        void onSend(text, internal).then(() => setBody(''));
      }}
    >
      <TextArea
        label={t('admin.review.addNote')}
        value={body}
        rows={3}
        maxLength={4000}
        onChange={(e) => setBody(e.target.value)}
      />
      <Checkbox
        label={t('admin.review.internalNote')}
        description={t('admin.review.internalNoteHint')}
        checked={internal}
        onChange={(e) => setInternal(e.target.checked)}
      />
      <div>
        <Button type="submit" disabled={!body.trim()} loading={busy}>
          {t('admin.review.addNoteButton')}
        </Button>
      </div>
    </form>
  );
}

function RejectDialog({
  open,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  onCancel: () => void;
  onConfirm: (reason: string) => Promise<unknown>;
}) {
  const t = useT();
  const [template, setTemplate] = useState('');
  useEffect(() => {
    if (!open) setTemplate('');
  }, [open]);
  return (
    <ReasonDialog
      open={open}
      title={t('admin.review.rejectTitle')}
      description={t('admin.review.rejectDescription')}
      confirmLabel={t('admin.review.reject')}
      danger
      reasonOptional
      reasonLabel={t('admin.review.rejectNote')}
      valid={template !== ''}
      onCancel={onCancel}
      onConfirm={(note) => onConfirm(note ? `${template}: ${note}` : template)}
    >
      <Select
        aria-label={t('admin.review.reasonLabel')}
        placeholder={t('admin.review.pickReason')}
        options={REJECT_TEMPLATES.map((o) => {
          const key = REJECT_LABEL_KEYS[o.value];
          return { value: o.value, label: key ? t(key) : o.label };
        })}
        value={template}
        onValueChange={setTemplate}
      />
    </ReasonDialog>
  );
}
