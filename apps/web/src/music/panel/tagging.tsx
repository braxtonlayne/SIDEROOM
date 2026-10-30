/**
 * Wave-30 integration (MUS-IDX-001 in the MUS-LIB-001 panel): the lazy tagger starts the first time the library
 * panel lists tracks, and "Rescan tags" is offered for every bucket in the listing whose tags the member may
 * write (`can_edit`). The tagger runs in the background, four files at a time; when it has written tags the
 * panel's lists are refetched so the new titles and covers show.
 */
import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo } from 'react';
import { useT } from '../../i18n';
import { getTagger, RescanTagsButton } from '../index';
import type { LibraryTrack } from '../queue/types';

let started = false;

/** Start the tagger once per page load, on the first listing that holds a track. */
export function useStartTagger(tracks: readonly LibraryTrack[] | undefined): void {
  const qc = useQueryClient();
  const listed = (tracks?.length ?? 0) > 0;
  useEffect(() => {
    if (!listed || started) return;
    started = true;
    void getTagger()
      .start()
      .then(
        (p) => {
          if (p.tagged > 0) void qc.invalidateQueries({ queryKey: ['music'] });
        },
        () => undefined,
      );
  }, [listed, qc]);
}

/** The buckets of a listing whose tags the member may write, in listing order. */
export function editableMounts(tracks: readonly LibraryTrack[]): string[] {
  const out: string[] = [];
  for (const t of tracks) if (t.canEdit && !out.includes(t.mountId)) out.push(t.mountId);
  return out;
}

export function RescanBar({ tracks }: { tracks: readonly LibraryTrack[] }) {
  const t = useT();
  const mounts = useMemo(() => editableMounts(tracks), [tracks]);
  if (mounts.length === 0) return null;
  return (
    <div className="sr-mlib-bar" role="group" aria-label={t('music.lib.tags.title')}>
      {mounts.map((m, i) => (
        <span key={m} className="sr-mlib-rescan">
          {mounts.length > 1 && <span>{t('music.lib.tags.bucket', { n: i + 1 })}</span>}
          <RescanTagsButton mountId={m} />
        </span>
      ))}
    </div>
  );
}

/** Test hook. */
export function __resetTaggerStart(): void {
  started = false;
}
