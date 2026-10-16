// The rest of the series page (P13): Related and Contributors tabs, "Share to Lounge" and the Patron
// download buttons (LIB-SER-002/004/005, LIB-SOC-005, LIB-RDR-016).
import { Button, EmptyState, Icon, Notice, Skeleton, useToast } from '@sideroom/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { ShareToLoungeDialog } from '../lib/ShareToLounge';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import {
  downloadChapter,
  getContributors,
  getDownloadAllowance,
  getRelated,
  type ChapterExport,
} from './p13api';
import { SeriesCard } from './ui';

/** Whether the member holds the Patron download perk; the server enforces it either way (D-075). */
export function useDownloadAllowance() {
  return useQuery({
    queryKey: ['library', 'download-allowance'],
    queryFn: getDownloadAllowance,
    staleTime: 60_000,
    retry: false,
  });
}

/** Start the browser download of a prepared file. */
export function saveExport(file: ChapterExport) {
  const a = document.createElement('a');
  a.href = file.url;
  a.download = file.filename;
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  a.remove();
}

/** A download button for one chapter; renders nothing for members without the perk. */
export function ChapterDownload({
  chapterId,
  label,
  compact,
}: {
  chapterId: string;
  label?: string;
  compact?: boolean;
}) {
  const t = useT();
  const toast = useToast();
  const allowance = useDownloadAllowance();
  const run = useMutation({
    mutationFn: () => downloadChapter(chapterId),
    onSuccess: (file) => {
      saveExport(file);
      void allowance.refetch();
    },
    onError: (e) => {
      const status = e instanceof ApiError ? e.status : 0;
      toast.show({
        title:
          status === 429
            ? t('library.download.limitReached')
            : status === 409
              ? t('library.download.tooLarge')
              : t('library.download.failed'),
        variant: 'warning',
      });
    },
  });
  if (!allowance.data?.entitled) return null;
  const left = allowance.data.left;
  const text = label ?? t('library.download.button');
  if (compact)
    return (
      <button
        type="button"
        className="lib-chapter-upto"
        disabled={run.isPending || left === 0}
        title={left === 0 ? t('library.download.limitReached') : text}
        aria-label={text}
        onClick={() => run.mutate()}
      >
        <Icon name="download" size={16} />
      </button>
    );
  return (
    <Button
      variant="secondary"
      disabled={run.isPending || left === 0}
      title={t('library.download.left', { n: left })}
      onClick={() => run.mutate()}
    >
      <Icon name="download" size={16} />
      {text}
    </Button>
  );
}

/** "Share to Lounge": posts the series link into a channel or DM the person picks. */
export function ShareButton({ seriesId, title }: { seriesId: string; title: string }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button variant="secondary" onClick={() => setOpen(true)}>
        <Icon name="link" size={16} />
        {t('library.series.shareToLounge')}
      </Button>
      {open && (
        <ShareToLoungeDialog
          open
          onClose={() => setOpen(false)}
          target={{
            url: `${window.location.origin}/library/s/${seriesId}`,
            title,
            kind: t('share.kindLibrary'),
          }}
        />
      )}
    </>
  );
}

const REASON_KEY = {
  tags: 'library.related.reasonTags',
  coread: 'library.related.reasonCoread',
} as const;

export function RelatedTab({ seriesId }: { seriesId: string }) {
  const t = useT();
  const q = useQuery({
    queryKey: ['library', 'related', seriesId],
    queryFn: () => getRelated(seriesId),
  });
  if (q.isPending) return <Skeleton height={160} />;
  if (q.isError) return <Notice variant="danger" title={t('library.related.loadError')} />;
  if (q.data.items.length === 0)
    return (
      <EmptyState
        title={t('library.related.emptyTitle')}
        description={t('library.related.emptyDescription')}
      />
    );
  return (
    <section aria-label={t('library.series.tab.related')}>
      <div className="lib-poster-row">
        {q.data.items.map((r) => (
          <div key={r.series.id} className="lib-related-item">
            <SeriesCard series={r.series} />
            <p className="lib-muted">
              {r.reasons
                .map((x) => (x in REASON_KEY ? t(REASON_KEY[x as keyof typeof REASON_KEY]) : x))
                .join(' · ')}
            </p>
          </div>
        ))}
      </div>
    </section>
  );
}

export function ContributorsTab({ seriesId }: { seriesId: string }) {
  const t = useT();
  const q = useQuery({
    queryKey: ['library', 'contributors', seriesId],
    queryFn: () => getContributors(seriesId),
  });
  if (q.isPending) return <Skeleton height={120} />;
  if (q.isError) return <Notice variant="danger" title={t('library.contributors.loadError')} />;
  const { members, groups } = q.data;
  if (members.length === 0 && groups.length === 0)
    return (
      <EmptyState
        title={t('library.contributors.emptyTitle')}
        description={t('library.contributors.emptyDescription')}
      />
    );
  return (
    <section aria-label={t('library.series.tab.contributors')} className="lib-contributors">
      {members.length > 0 && (
        <>
          <h2 className="lib-h2">{t('library.contributors.members')}</h2>
          <ul className="lib-contrib-list">
            {members.map((m) => (
              <li key={m.user_id}>
                <strong>{m.name}</strong>
                <span className="lib-muted">
                  {t('library.contributors.chapters', { n: m.chapters })}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
      {groups.length > 0 && (
        <>
          <h2 className="lib-h2">{t('library.contributors.groups')}</h2>
          <ul className="lib-contrib-list">
            {groups.map((g) => (
              <li key={g.name}>
                <strong>{g.name}</strong>
                <span className="lib-muted">
                  {t('library.contributors.chapters', { n: g.chapters })}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
