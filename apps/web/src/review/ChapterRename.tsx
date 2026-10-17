// Inline edit of one draft chapter's number, title and (for a series that already exists) volume (REV-ED-001,
// K-07). Shared by the submitter's editor and the reviewer's edit-in-place panel (K-06).
import { Button, Select, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../i18n';
import type { DraftChapter, DraftVolume } from './types';

const NO_VOLUME = '';

export function ChapterRename({
  chapter,
  volumes = [],
  busy,
  onSave,
  onCancel,
}: {
  chapter: DraftChapter;
  /** The volumes the chapter may move to; the select shows only when there is any. */
  volumes?: DraftVolume[];
  busy: boolean;
  onSave: (patch: { number: number; title: string | null; volume_id: string | null }) => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [number, setNumber] = useState(String(chapter.number));
  const [title, setTitle] = useState(chapter.title ?? '');
  const [volume, setVolume] = useState(chapter.volume_id ?? NO_VOLUME);
  const ok = number.trim() !== '' && Number.isFinite(Number(number)) && Number(number) >= 0;
  const mine = volumes.filter((v) => v.edition_id === chapter.edition_id);
  return (
    <form
      className="rv-form"
      aria-label={t('library.submissions.chapters.renameForm')}
      onSubmit={(e) => {
        e.preventDefault();
        if (ok)
          onSave({
            number: Number(number),
            title: title.trim() || null,
            volume_id: volume === NO_VOLUME ? null : volume,
          });
      }}
    >
      <TextInput
        label={t('library.submissions.add.number')}
        inputMode="decimal"
        value={number}
        onChange={(e) => setNumber(e.target.value)}
      />
      <TextInput
        label={t('library.submissions.add.title')}
        value={title}
        maxLength={200}
        onChange={(e) => setTitle(e.target.value)}
      />
      {mine.length > 0 && (
        <Select
          aria-label={t('review.chapter.volume')}
          options={[
            { value: NO_VOLUME, label: t('review.chapter.noVolume') },
            ...mine.map((v) => ({
              value: v.id,
              label: v.title
                ? t('review.chapter.volumeTitled', { n: v.number, title: v.title })
                : t('review.chapter.volumeNumber', { n: v.number }),
            })),
          ]}
          value={volume}
          onValueChange={setVolume}
        />
      )}
      <div className="rv-row">
        <Button type="submit" size="sm" disabled={!ok} loading={busy}>
          {t('library.submissions.chapters.saveRename')}
        </Button>
        <Button type="button" size="sm" variant="secondary" onClick={onCancel}>
          {t('library.submissions.chapters.cancelRename')}
        </Button>
      </div>
    </form>
  );
}
