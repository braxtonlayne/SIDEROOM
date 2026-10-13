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
import { useCallback, useEffect, useState } from 'react';
import * as rv from '../../review/api';
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
  SectionHeader,
  errorText,
  useAction,
  useAdminViewer,
  useResource,
} from '../ui';

const PAGE = 25;

const STATE_FILTERS = [
  { value: 'waiting', label: 'Waiting and in review' },
  { value: 'submitted', label: 'Waiting for review' },
  { value: 'in_review', label: 'In review' },
  { value: 'changes_requested', label: 'Changes requested' },
  { value: 'approved', label: 'Approved' },
  { value: 'published', label: 'Published' },
  { value: 'rejected', label: 'Rejected' },
  { value: 'all', label: 'Every state' },
];
const ALL_STATES =
  'draft,submitted,in_review,changes_requested,approved,rejected,published,withdrawn';
const CLAIM_FILTERS = [
  { value: '', label: 'Anyone' },
  { value: 'mine', label: 'Claimed by me' },
  { value: 'unclaimed', label: 'Not claimed' },
];
const SORTS = [
  { value: 'submitted:asc', label: 'Oldest waiting first' },
  { value: 'submitted:desc', label: 'Newest first' },
  { value: 'updated:desc', label: 'Recently updated' },
  { value: 'title:asc', label: 'Title A to Z' },
];
const AGES = [
  { value: '', label: 'Any age' },
  { value: '24', label: 'Waiting a day or more' },
  { value: '72', label: 'Waiting 3 days or more' },
  { value: '168', label: 'Waiting a week or more' },
];
const KINDS = [
  { value: '', label: 'Every kind' },
  { value: 'library.series', label: 'New series' },
  { value: 'library.items', label: 'New chapters' },
];

export const REJECT_TEMPLATES = [
  { value: 'Duplicate of an existing title', label: 'Duplicate of an existing title' },
  { value: 'Poor image quality', label: 'Poor image quality' },
  { value: 'Wrong or missing metadata', label: 'Wrong or missing metadata' },
  { value: 'Content we cannot host', label: 'Content we cannot host' },
  { value: 'Other', label: 'Other (explain below)' },
];

/** Who a lease or an event actor is, from the reviewer's point of view. */
const who = (me: string, id: string | null) =>
  id === null ? 'Someone' : id === me ? 'You' : `Account ${id}`;

export function ReviewSection() {
  const [open, setOpen] = useState<string | null>(null);
  return (
    <>
      <SectionHeader
        title="Review queues"
        description="Submissions waiting for a decision. Claim one to review it; a claim lasts 30 minutes."
      />
      {open ? (
        <ReviewDetail id={open} onBack={() => setOpen(null)} />
      ) : (
        <ReviewQueue onOpen={setOpen} />
      )}
    </>
  );
}

// ---- queue ----

function ReviewQueue({ onOpen }: { onOpen: (id: string) => void }) {
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
        aria-label="Filter the review queue"
        onSubmit={(e) => {
          e.preventDefault();
          setQ(text.trim());
          setOffset(0);
        }}
      >
        <TextInput label="Title contains" value={text} onChange={(e) => setText(e.target.value)} />
        <Select
          aria-label="Filter by state"
          options={STATE_FILTERS}
          value={state}
          onValueChange={reset(setState)}
        />
        <Select
          aria-label="Filter by kind"
          options={KINDS}
          value={kind}
          onValueChange={reset(setKind)}
        />
        <Select
          aria-label="Filter by claim"
          options={CLAIM_FILTERS}
          value={claimed}
          onValueChange={reset(setClaimed)}
        />
        <Select
          aria-label="Filter by waiting time"
          options={AGES}
          value={age}
          onValueChange={reset(setAge)}
        />
        <Select aria-label="Sort" options={SORTS} value={sort} onValueChange={reset(setSort)} />
        <Button type="submit">Filter</Button>
      </form>
      <Async res={res} label="Loading the queue">
        {(page) =>
          page.items.length === 0 ? (
            <EmptyState
              title="Nothing to review"
              description="No submission matches this filter right now."
            />
          ) : (
            <>
              <div className="rv-table-wrap">
                <table className="rv-table" aria-label="Review queue">
                  <thead>
                    <tr>
                      <th scope="col">Submission</th>
                      <th scope="col">Submitter</th>
                      <th scope="col">State</th>
                      <th scope="col">Waiting</th>
                      <th scope="col">Claim</th>
                      <th scope="col">
                        <span className="sr-only-label">Open</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {page.items.map((s) => (
                      <tr key={s.id}>
                        <td>
                          <strong>{s.title}</strong>
                          <div className="rv-muted">
                            {s.kind === 'library.items' ? 'New chapters' : 'New series'}
                          </div>
                        </td>
                        <td className="adm-mono">{s.submitter_id ?? 'Deleted account'}</td>
                        <td>
                          <StateChip state={s.state} />
                        </td>
                        <td>{s.submitted_at_ms ? ago(s.submitted_at_ms) : ''}</td>
                        <td>{claimText(s, viewer.userId)}</td>
                        <td>
                          <Button
                            size="sm"
                            aria-label={`Open ${s.title}`}
                            onClick={() => onOpen(s.id)}
                          >
                            Open
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="rv-pager">
                <span className="rv-muted" aria-live="polite">
                  {offset + 1} to {offset + page.items.length} of {page.total}
                </span>
                <div className="rv-actions">
                  <Button
                    variant="secondary"
                    disabled={offset === 0}
                    onClick={() => setOffset(Math.max(0, offset - PAGE))}
                  >
                    Previous
                  </Button>
                  <Button
                    variant="secondary"
                    disabled={offset + page.items.length >= page.total}
                    onClick={() => setOffset(offset + PAGE)}
                  >
                    Next
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

function claimText(s: Submission, me: string): string {
  if (!s.claimed_by || !s.claim_expires_at_ms || s.claim_expires_at_ms <= Date.now()) return 'Free';
  const by = s.claimed_by === me ? 'You' : `Account ${s.claimed_by}`;
  return `${by} until ${new Date(s.claim_expires_at_ms).toLocaleTimeString(undefined, { timeStyle: 'short' })}`;
}

// ---- review page ----

function ReviewDetail({ id, onBack }: { id: string; onBack: () => void }) {
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
    const t = setInterval(() => tick((n) => n + 1), 30_000);
    return () => clearInterval(t);
  }, []);

  const apply = async (fn: () => Promise<SubmissionDetail>, done: string) => {
    const d = await action.run(fn, done);
    if (d) {
      res.set(d);
      void preview.reload();
    }
    return d;
  };

  return (
    <Async res={res} label="Loading the submission">
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
                Back to the queue
              </Button>
            </div>
            <div className="rv-head">
              <h2>{s.title}</h2>
              <StateChip state={s.state} />
              <Badge variant="neutral">{d.kind_label}</Badge>
            </div>
            <dl className="rv-kv" aria-label="Submission details">
              <div className="rv-kv-row">
                <dt>Submitter</dt>
                <dd className="adm-mono">{s.submitter_id ?? 'Deleted account'}</dd>
              </div>
              <div className="rv-kv-row">
                <dt>Submitted</dt>
                <dd>{fmtWhen(s.submitted_at_ms) || 'Not yet'}</dd>
              </div>
              <div className="rv-kv-row">
                <dt>Claim</dt>
                <dd>{claimText(s, viewer.userId)}</dd>
              </div>
              {s.summary && (
                <div className="rv-kv-row">
                  <dt>Submitter note</dt>
                  <dd>{s.summary}</dd>
                </div>
              )}
            </dl>

            <ActionStatus error={action.error} done={action.done} />
            {failed && (
              <Notice variant="warning" title="Publishing failed" className="ss-status">
                {failed.body}
                <div style={{ marginTop: 8 }}>
                  <Button
                    size="sm"
                    loading={action.busy}
                    onClick={() => void apply(() => rv.publish(id), 'Published.')}
                  >
                    Try publishing again
                  </Button>
                </div>
              </Notice>
            )}

            <div
              className="rv-actions"
              role="group"
              aria-label="Decision bar"
              style={{ margin: '16px 0' }}
            >
              {(s.state === 'submitted' || (s.state === 'in_review' && !live)) && (
                <Button
                  loading={action.busy}
                  onClick={() =>
                    void apply(() => rv.claim(id), 'You claimed this submission for 30 minutes.')
                  }
                >
                  Claim
                </Button>
              )}
              {s.state === 'in_review' && heldByOther && viewer.can('PUBLISH_READING') && (
                <Button variant="secondary" onClick={() => setDialog('takeover')}>
                  Take over the claim
                </Button>
              )}
              {mine && s.state === 'in_review' && (
                <Button
                  variant="secondary"
                  loading={action.busy}
                  onClick={() => void apply(() => rv.release(id), 'Returned to the queue.')}
                >
                  Release
                </Button>
              )}
              <Button disabled={!canDecide} onClick={() => setDialog('approve')}>
                Approve
              </Button>
              <Button
                variant="secondary"
                disabled={!canDecide}
                onClick={() => setDialog('changes')}
              >
                Request changes
              </Button>
              <Button variant="danger" disabled={!canDecide} onClick={() => setDialog('reject')}>
                Reject
              </Button>
            </div>
            {!canDecide && s.state === 'in_review' && (
              <p className="rv-muted">
                {heldByOther
                  ? `Account ${s.claimed_by} is reviewing this until ${fmtWhen(s.claim_expires_at_ms)}.`
                  : 'Claim this submission to decide on it.'}
              </p>
            )}

            <section className="rv-panel" aria-labelledby="rv-changes-h">
              <h3 id="rv-changes-h">What changed</h3>
              {last && prev ? (
                <ChangeList changes={previewDiff(prev, last)} />
              ) : (
                <p className="rv-muted">First submission: everything below is new.</p>
              )}
            </section>

            <section className="rv-panel" aria-labelledby="rv-meta-h">
              <h3 id="rv-meta-h">Metadata</h3>
              <Async res={preview} label="Loading the preview">
                {(p: LibraryPreview) => (
                  <>
                    <PreviewFacts preview={p} />
                    {!p.new_series && (
                      <p className="rv-muted">
                        The series already has {p.existing_chapters} published chapters.
                      </p>
                    )}
                  </>
                )}
              </Async>
            </section>

            <section className="rv-panel" aria-labelledby="rv-items-h">
              <h3 id="rv-items-h">Chapters ({items.length})</h3>
              {items.length === 0 ? (
                <p className="rv-muted">No items.</p>
              ) : (
                <div className="rv-table-wrap">
                  <table className="rv-table" aria-label="Items under review">
                    <thead>
                      <tr>
                        <th scope="col">Item</th>
                        <th scope="col">Decision</th>
                        <th scope="col">Note</th>
                        <th scope="col">
                          <span className="sr-only-label">Actions</span>
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
                              `${it.label} ${state}.`,
                            )
                          }
                          onPages={async () => {
                            try {
                              const m = await rv.reviewPages(id, it.ref_id);
                              setPages({ item: it, pages: m.pages });
                            } catch (e) {
                              action.setError(errorText(e, 'Could not load the pages.'));
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
              <h3 id="rv-time-h">Timeline and notes</h3>
              <Timeline events={d.events} actorLabel={(a) => who(viewer.userId, a)} />
              <NoteForm
                busy={action.busy}
                onSend={(body, internal) =>
                  apply(() => rv.reviewComment(id, body, internal), 'Note added.')
                }
              />
            </section>

            <ConfirmDialog
              open={dialog === 'approve'}
              title="Approve this submission"
              description={
                pendingLeft > 0
                  ? `${approveCount} of ${items.length} items will be approved and published. Items you rejected stay out.`
                  : `${approveCount} approved items will be published now.`
              }
              confirmLabel="Approve and publish"
              loading={action.busy}
              onCancel={() => setDialog(null)}
              onConfirm={() =>
                void apply(() => rv.approve(id), 'Approved.').then(() => setDialog(null))
              }
            />
            <ConfirmDialog
              open={dialog === 'takeover'}
              title="Take over the claim"
              description={`Account ${s.claimed_by ?? ''} is still reviewing this. They will lose the claim and cannot decide any more.`}
              confirmLabel="Take over"
              loading={action.busy}
              onCancel={() => setDialog(null)}
              onConfirm={() =>
                void apply(() => rv.claim(id, true), 'You took over the claim.').then(() =>
                  setDialog(null),
                )
              }
            />
            <ReasonDialog
              open={dialog === 'changes'}
              title="Request changes"
              description="The submitter sees this note and can fix the submission and send it again."
              confirmLabel="Send back"
              reasonLabel="What needs to change"
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
              footer={<Button onClick={() => setPages(null)}>Close</Button>}
            >
              {pages && (
                <div className="rv-pages">
                  {pages.pages.slice(0, 24).map((p) => (
                    <img
                      key={p.index}
                      src={p.url}
                      alt={`Page ${p.index + 1}`}
                      width={p.width}
                      height={p.height}
                      loading="lazy"
                    />
                  ))}
                </div>
              )}
              {pages && pages.pages.length > 24 && (
                <p className="rv-muted">Showing 24 of {pages.pages.length} pages.</p>
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
              {ch.page_count} {ch.format === 'images' ? 'pages' : ch.format.toUpperCase()}
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
                aria-label={`View pages of ${item.label}`}
                onClick={onPages}
              >
                Pages
              </Button>
            )}
            {canDecide && item.state !== 'approved' && (
              <Button
                size="sm"
                aria-label={`Approve ${item.label}`}
                disabled={busy}
                onClick={() => onDecide('approved')}
              >
                Approve
              </Button>
            )}
            {canDecide && item.state !== 'rejected' && (
              <Button
                size="sm"
                variant="danger"
                aria-label={`Reject ${item.label}`}
                disabled={busy}
                onClick={() => setRejecting(true)}
              >
                Reject
              </Button>
            )}
            {canDecide && item.state !== 'pending' && (
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Reset ${item.label}`}
                disabled={busy}
                onClick={() => onDecide('pending')}
              >
                Reset
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
                label={`Why is ${item.label} rejected?`}
                value={note}
                rows={2}
                onChange={(e) => setNote(e.target.value)}
              />
              <div className="rv-actions">
                <Button type="submit" variant="danger" disabled={!note.trim()}>
                  Reject item
                </Button>
                <Button type="button" variant="ghost" onClick={() => setRejecting(false)}>
                  Cancel
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
        label="Add a note"
        value={body}
        rows={3}
        maxLength={4000}
        onChange={(e) => setBody(e.target.value)}
      />
      <Checkbox
        label="Internal note"
        description="Only reviewers see it. Leave it off to tell the submitter."
        checked={internal}
        onChange={(e) => setInternal(e.target.checked)}
      />
      <div>
        <Button type="submit" disabled={!body.trim()} loading={busy}>
          Add note
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
  const [template, setTemplate] = useState('');
  useEffect(() => {
    if (!open) setTemplate('');
  }, [open]);
  return (
    <ReasonDialog
      open={open}
      title="Reject this submission"
      description="Rejecting is final. The submitter sees the reason."
      confirmLabel="Reject"
      danger
      reasonOptional
      reasonLabel="Note for the submitter"
      valid={template !== ''}
      onCancel={onCancel}
      onConfirm={(note) => onConfirm(note ? `${template}: ${note}` : template)}
    >
      <Select
        aria-label="Reason"
        placeholder="Pick a reason"
        options={REJECT_TEMPLATES}
        value={template}
        onValueChange={setTemplate}
      />
    </ReasonDialog>
  );
}
