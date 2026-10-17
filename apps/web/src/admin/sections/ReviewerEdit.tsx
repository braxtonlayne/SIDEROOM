// Metadata edit-in-place on the review page (REV-RV-002, K-06, D-255): the reviewer who holds the claim fixes the
// draft through the submission routes (never the publisher routes). The submitter's editor components are reused:
// the series metadata form, the artwork field and the chapter rename form. Every write shows on the timeline
// for the submitter.
import { Badge, Button, Notice } from '@sideroom/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useCallback, useEffect, useState } from 'react';
import { useT } from '../../i18n';
import * as rv from '../../review/api';
import { ArtworkField } from '../../review/ArtworkField';
import { ChapterRename } from '../../review/ChapterRename';
import { chapterUpdateBody } from '../../review/chapterEdit';
import { DuplicateWarning, SeriesMetaEditor } from '../../review/SeriesMetaEditor';
import { fmtWhen } from '../../review/shared';
import type {
  DraftChapter,
  DraftChapterList,
  LibraryPreview,
  Submission,
} from '../../review/types';

// The series form ("Fetch details") uses react-query; the admin shell has no provider of its own.
const editClient = new QueryClient();

export function ReviewerEdit(props: ReviewerEditProps) {
  return (
    <QueryClientProvider client={editClient}>
      <ReviewerEditBody {...props} />
    </QueryClientProvider>
  );
}

interface ReviewerEditProps {
  submission: Submission;
  /** The review preview; the series form shows for a new series only. */
  preview: LibraryPreview | undefined;
  /** Called after a write so the page reloads the preview and the timeline. */
  onChanged: () => void;
}

function ReviewerEditBody({ submission, preview, onChanged }: ReviewerEditProps) {
  const t = useT();
  const id = submission.id;
  const [list, setList] = useState<DraftChapterList | null>(null);
  const [failed, setFailed] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [problems, setProblems] = useState<string[]>([]);
  const newSeries = submission.kind === 'library.series';

  const load = useCallback(() => {
    rv.draftChapters(id).then(
      (l) => {
        setList(l);
        setFailed(false);
      },
      () => setFailed(true),
    );
  }, [id]);
  useEffect(load, [load]);

  const changed = () => {
    load();
    onChanged();
  };
  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setProblems([]);
    try {
      await fn();
      setEditing(null);
      changed();
    } catch (e) {
      setProblems(rv.errorMessages(e));
    } finally {
      setBusy(false);
    }
  };

  const rows = (list?.chapters ?? [])
    .filter((c) => !c.published)
    .sort((a, b) => a.number - b.number);
  const volumes = submission.kind === 'library.items' ? (list?.volumes ?? []) : [];
  const nameOf = (c: DraftChapter) =>
    c.title
      ? t('library.submissions.chapters.namedTitled', { n: c.number, title: c.title })
      : t('library.submissions.chapters.named', { n: c.number });

  return (
    <section className="rv-panel" aria-labelledby="rv-edit-h">
      <div className="rv-head">
        <h3 id="rv-edit-h">{t('review.reviewerEdit.title')}</h3>
        <Badge variant="accent">{t('review.reviewerEdit.badge')}</Badge>
      </div>
      <p className="rv-muted">{t('review.reviewerEdit.intro')}</p>
      {submission.claim_expires_at_ms && (
        <p className="rv-muted">
          {t('review.reviewerEdit.claimLive', { time: fmtWhen(submission.claim_expires_at_ms) })}
        </p>
      )}
      {problems.length > 0 && (
        <Notice variant="danger">
          {problems.map((p) => (
            <div key={p}>{p}</div>
          ))}
        </Notice>
      )}
      {newSeries && preview && (
        <>
          <DuplicateWarning submissionId={id} />
          <SeriesMetaEditor
            key={JSON.stringify(preview.series)}
            submissionId={id}
            preview={preview}
            onSaved={onChanged}
          />
          <h4>{t('review.reviewerEdit.cover')}</h4>
          <ArtworkField submissionId={id} slot="cover" onSaved={onChanged} />
        </>
      )}
      <h4>{t('review.reviewerEdit.chapters')}</h4>
      {failed && <Notice variant="warning">{t('review.reviewerEdit.chaptersError')}</Notice>}
      {list && rows.length === 0 && (
        <p className="rv-muted">{t('review.reviewerEdit.noChapters')}</p>
      )}
      {rows.length > 0 && (
        <ul className="rv-changes">
          {rows.map((c) => (
            <li key={c.id}>
              {editing === c.id ? (
                <ChapterRename
                  chapter={c}
                  volumes={volumes}
                  busy={busy}
                  onCancel={() => setEditing(null)}
                  onSave={(patch) =>
                    void run(() => rv.updateDraftChapter(id, c.id, chapterUpdateBody(c, patch)))
                  }
                />
              ) : (
                <>
                  <strong>{nameOf(c)}</strong>
                  <span className="rv-row">
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={t('review.reviewerEdit.renameLabel', { name: nameOf(c) })}
                      onClick={() => setEditing(c.id)}
                    >
                      {t('review.reviewerEdit.rename')}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={t('review.reviewerEdit.removeLabel', { name: nameOf(c) })}
                      disabled={busy}
                      onClick={() => void run(() => rv.deleteDraftChapter(id, c.id))}
                    >
                      {t('review.reviewerEdit.remove')}
                    </Button>
                  </span>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
