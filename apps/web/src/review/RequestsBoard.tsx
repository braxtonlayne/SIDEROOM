// The requests board (REV-REQ-002): per section, sort by votes or newest, filter by status,
// vote, request a title, and say "I'll upload this" with a short note.
import {
  Badge,
  Button,
  EmptyState,
  Notice,
  Select,
  Spinner,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';
import { useT } from '../i18n';
import * as rv from './api';
import { fmtWhen, useSubmissionUpdates } from './shared';
import type { ContentRequest } from './types';
import './review.css';

const PAGE = 20;

function VoteButton({ r, onDone }: { r: ContentRequest; onDone: () => void }) {
  const t = useT();
  const vote = useMutation({
    mutationFn: () => rv.voteRequest(r.id, !r.voted),
    onSuccess: onDone,
  });
  return (
    <Button
      variant={r.voted ? 'primary' : 'secondary'}
      aria-pressed={r.voted}
      aria-label={
        r.voted
          ? t('review.requests.unvote', { title: r.title })
          : t('review.requests.vote', { title: r.title })
      }
      disabled={r.status !== 'open'}
      loading={vote.isPending}
      onClick={() => vote.mutate()}
    >
      {t('review.requests.votes', { n: r.vote_count })}
    </Button>
  );
}

function ClaimBox({ r, onDone }: { r: ContentRequest; onDone: () => void }) {
  const t = useT();
  const [editing, setEditing] = useState(false);
  const [note, setNote] = useState('');
  const claim = useMutation({
    mutationFn: () => rv.claimRequest(r.id, note.trim()),
    onSuccess: () => {
      setEditing(false);
      onDone();
    },
  });
  const release = useMutation({ mutationFn: () => rv.releaseRequest(r.id), onSuccess: onDone });
  if (r.status !== 'open') return null;
  const noteText = r.claim_note
    ? ` · ${t('review.requests.claimNote', { note: r.claim_note })}`
    : '';
  if (r.claimed_by && !r.claimed_by_me) {
    return (
      <p className="rv-muted" style={{ margin: 0 }}>
        {t('review.requests.claimedByOther')}
        {noteText}
      </p>
    );
  }
  if (r.claimed_by_me) {
    return (
      <div className="rv-stack">
        <p style={{ margin: 0 }}>
          <strong>{t('review.requests.claimedByYou')}</strong>
          {noteText}
        </p>
        <div className="rv-row">
          <Link to="/library/submissions">{t('review.requests.upload')}</Link>
          <Button variant="secondary" loading={release.isPending} onClick={() => release.mutate()}>
            {t('review.requests.release')}
          </Button>
        </div>
      </div>
    );
  }
  if (!editing) {
    return (
      <div>
        <Button variant="secondary" onClick={() => setEditing(true)}>
          {t('review.requests.claim')}
        </Button>
      </div>
    );
  }
  return (
    <form
      className="rv-stack"
      onSubmit={(e) => {
        e.preventDefault();
        claim.mutate();
      }}
    >
      <TextInput
        label={t('review.requests.claimNoteLabel')}
        hint={t('review.requests.claimNoteHint')}
        value={note}
        maxLength={500}
        onChange={(e) => setNote(e.target.value)}
      />
      {claim.isError && (
        <Notice variant="danger" role="alert">
          {t('review.requests.claimError')}
        </Notice>
      )}
      <div className="rv-row">
        <Button type="submit" loading={claim.isPending}>
          {t('review.requests.claimSave')}
        </Button>
        <Button type="button" variant="secondary" onClick={() => setEditing(false)}>
          {t('review.requests.claimCancel')}
        </Button>
      </div>
    </form>
  );
}

function RequestRow({ r, onDone }: { r: ContentRequest; onDone: () => void }) {
  const t = useT();
  return (
    <li className="rv-panel" style={{ marginBottom: 0 }}>
      <div className="rv-head" style={{ marginBottom: 4 }}>
        <h2 style={{ fontSize: '1.125rem' }}>
          {r.title}
          {r.year ? ` ${t('review.requests.year', { year: r.year })}` : ''}
        </h2>
        {r.status === 'fulfilled' && (
          <Badge variant="success">{t('review.requests.fulfilled')}</Badge>
        )}
        {r.status === 'declined' && (
          <Badge variant="neutral">{t('review.requests.declined')}</Badge>
        )}
        <VoteButton r={r} onDone={onDone} />
      </div>
      <p className="rv-muted" style={{ margin: 0 }}>
        {r.creator ? `${t('review.requests.byline', { creator: r.creator })} · ` : ''}
        {fmtWhen(r.created_at_ms)}
        {r.link && (
          <>
            {' · '}
            <a href={r.link} target="_blank" rel="noreferrer noopener">
              {t('review.requests.link')}
            </a>
          </>
        )}
      </p>
      {r.note && <p style={{ margin: '8px 0' }}>{r.note}</p>}
      <ClaimBox r={r} onDone={onDone} />
    </li>
  );
}

function NewRequest({ section, onDone }: { section: string; onDone: () => void }) {
  const t = useT();
  const [title, setTitle] = useState('');
  const [creator, setCreator] = useState('');
  const [year, setYear] = useState('');
  const [link, setLink] = useState('');
  const [note, setNote] = useState('');
  const create = useMutation({
    mutationFn: () =>
      rv.createRequest({
        section,
        title: title.trim(),
        ...(creator.trim() ? { creator: creator.trim() } : {}),
        ...(/^\d{4}$/.test(year.trim()) ? { year: Number(year.trim()) } : {}),
        ...(link.trim() ? { link: link.trim() } : {}),
        note: note.trim(),
      }),
    onSuccess: () => {
      setTitle('');
      setCreator('');
      setYear('');
      setLink('');
      setNote('');
      onDone();
    },
  });
  return (
    <form
      className="rv-panel rv-form"
      aria-label={t('review.requests.new')}
      onSubmit={(e) => {
        e.preventDefault();
        if (title.trim()) create.mutate();
      }}
    >
      <TextInput
        label={t('review.requests.form.title')}
        value={title}
        maxLength={200}
        required
        onChange={(e) => setTitle(e.target.value)}
      />
      <TextInput
        label={t('review.requests.form.creator')}
        value={creator}
        maxLength={200}
        onChange={(e) => setCreator(e.target.value)}
      />
      <TextInput
        label={t('review.requests.form.year')}
        inputMode="numeric"
        value={year}
        maxLength={4}
        onChange={(e) => setYear(e.target.value)}
      />
      <TextInput
        label={t('review.requests.form.link')}
        type="url"
        value={link}
        maxLength={500}
        onChange={(e) => setLink(e.target.value)}
      />
      <TextArea
        label={t('review.requests.form.note')}
        value={note}
        maxLength={2000}
        onChange={(e) => setNote(e.target.value)}
      />
      {create.isError && (
        <Notice variant="danger" role="alert">
          {t('review.requests.form.error')}
        </Notice>
      )}
      <div className="rv-row">
        <Button type="submit" disabled={!title.trim()} loading={create.isPending}>
          {t('review.requests.form.submit')}
        </Button>
      </div>
    </form>
  );
}

export function RequestsBoard({ section: initial = 'library' }: { section?: string }) {
  const t = useT();
  const qc = useQueryClient();
  const [section, setSection] = useState(initial);
  const [sort, setSort] = useState('votes');
  const [status, setStatus] = useState('open');
  const [limit, setLimit] = useState(PAGE);
  const [creating, setCreating] = useState(false);
  const refresh = () => void qc.invalidateQueries({ queryKey: ['requests'] });
  useSubmissionUpdates(refresh);
  const list = useQuery({
    queryKey: ['requests', section, sort, status, limit],
    queryFn: () => rv.listRequests({ section, sort, limit, ...(status ? { status } : {}) }),
  });
  return (
    <div>
      <div className="lib-page-head">
        <h1 className="lib-h1">{t('review.requests.title')}</h1>
        <Button onClick={() => setCreating((c) => !c)}>
          {creating ? t('review.requests.closeForm') : t('review.requests.new')}
        </Button>
      </div>
      <p className="rv-muted">{t('review.requests.intro')}</p>
      <div className="rv-row" style={{ marginBottom: 16 }}>
        <Select
          aria-label={t('review.requests.section')}
          options={[
            { value: 'library', label: t('review.requests.sectionLibrary') },
            { value: 'cinema', label: t('review.requests.sectionCinema') },
          ]}
          value={section}
          onValueChange={(v) => {
            setSection(v);
            setLimit(PAGE);
          }}
        />
        <Select
          aria-label={t('review.requests.sort')}
          options={[
            { value: 'votes', label: t('review.requests.sortVotes') },
            { value: 'new', label: t('review.requests.sortNew') },
          ]}
          value={sort}
          onValueChange={setSort}
        />
        <Select
          aria-label={t('review.requests.status')}
          options={[
            { value: 'open', label: t('review.requests.statusOpen') },
            { value: 'fulfilled', label: t('review.requests.statusFulfilled') },
            { value: 'declined', label: t('review.requests.statusDeclined') },
            { value: '', label: t('review.requests.statusAll') },
          ]}
          value={status}
          onValueChange={(v) => {
            setStatus(v);
            setLimit(PAGE);
          }}
        />
      </div>
      {creating && (
        <NewRequest
          section={section}
          onDone={() => {
            setCreating(false);
            refresh();
          }}
        />
      )}
      {list.isPending && <Spinner label={t('review.requests.loading')} />}
      {list.isError && (
        <Notice variant="danger" role="alert">
          {t('review.requests.loadError')}
        </Notice>
      )}
      {list.data && list.data.items.length === 0 && (
        <EmptyState
          title={t('review.requests.emptyTitle')}
          description={t('review.requests.emptyDescription')}
        />
      )}
      {list.data && list.data.items.length > 0 && (
        <ul
          className="rv-stack"
          style={{ listStyle: 'none', padding: 0 }}
          aria-label={t('review.requests.listLabel')}
        >
          {list.data.items.map((r) => (
            <RequestRow key={r.id} r={r} onDone={refresh} />
          ))}
        </ul>
      )}
      {list.data && list.data.total > list.data.items.length && (
        <div className="rv-row" style={{ marginTop: 12 }}>
          <Button variant="secondary" onClick={() => setLimit((l) => Math.min(100, l + PAGE))}>
            {t('review.requests.more')}
          </Button>
        </div>
      )}
    </div>
  );
}
