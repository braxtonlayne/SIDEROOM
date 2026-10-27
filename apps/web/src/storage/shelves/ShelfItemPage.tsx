import { Button, EmptyState, Notice, Spinner, Tag } from '@sideroom/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { storageErrorText } from '../../settings/user/storage/errors';
import {
  getIndexStatus,
  getShelfItem,
  playableKind,
  shelfKeys,
  startIndex,
  type Playable,
  type ShelfDetail,
  type ShelfSection,
} from './api';
import { CbzReader } from './CbzReader';
import { MetadataEditor } from './MetadataEditor';
import { PresignedReader } from './PresignedReader';
import { ShelfChip } from './ShelfChip';
import { ShelfPlayer } from './ShelfPlayer';
import './shelves.css';

interface Opened {
  key: string;
  kind: Exclude<Playable, 'none'>;
  title: string;
}

/** The button of one file: Play or Read, or the greyed "Convert to play" for a file a browser cannot play. */
function OpenButton({
  container,
  webNative,
  title,
  objectKey,
  onOpen,
}: {
  container: string;
  webNative: boolean;
  title: string;
  objectKey: string;
  onOpen: (o: Opened) => void;
}) {
  const t = useT();
  const kind = playableKind(container, webNative);
  if (kind === 'none') {
    return (
      <Button variant="secondary" size="sm" disabled title={t('shelves.convertHint')}>
        {t('shelves.convert')}
      </Button>
    );
  }
  const label =
    kind === 'video' || kind === 'audio'
      ? t(kind === 'audio' ? 'shelves.listen' : 'shelves.play')
      : t('shelves.read');
  return (
    <Button size="sm" onClick={() => onOpen({ key: objectKey, kind, title })}>
      {label}
    </Button>
  );
}

/**
 * One film, series, book, comic or track from a mount (BYOS-CAT-002): the source chip, details with the metadata
 * provider's attribution when there is one, playback or reading of web-native files straight from the bucket,
 * "Convert to play" (greyed until wave 29) for the rest, and for the mount's owner the editor and Re-index.
 */
export function ShelfItemPage({ section, id }: { section: ShelfSection; id: string }) {
  const t = useT();
  const qc = useQueryClient();
  const [opened, setOpened] = useState<Opened | null>(null);
  const [editing, setEditing] = useState(false);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);
  const q = useQuery({
    queryKey: shelfKeys.item(section, id),
    queryFn: () => getShelfItem(section, id),
    retry: false,
  });
  const detail: ShelfDetail | undefined = q.data;
  const mountId = detail?.item.source.mount_id;
  const status = useQuery({
    queryKey: shelfKeys.index(mountId ?? ''),
    queryFn: () => getIndexStatus(mountId ?? ''),
    enabled: !!detail?.can_edit && !!mountId,
    retry: false,
  });
  const reindex = useMutation({
    mutationFn: () => startIndex(mountId ?? ''),
    onSuccess: (s) => {
      setNote({
        ok: true,
        text: s.queued ? t('shelves.reindexQueued') : t('shelves.reindexRunning'),
      });
      qc.setQueryData(shelfKeys.index(mountId ?? ''), s);
    },
    onError: (e) =>
      setNote({
        ok: false,
        text: e instanceof ApiError ? storageErrorText(e) : t('shelves.reindexFailed'),
      }),
  });

  const back = (
    <Link to={section === 'library' ? '/library/shelves' : '/cinema/shelves'} className="shv-muted">
      {t('shelves.back')}
    </Link>
  );
  if (q.isPending)
    return (
      <section className="shv-page">
        <Spinner label={t('shelves.loading')} />
      </section>
    );
  if (q.isError || !detail)
    return (
      <section className="shv-page">
        {back}
        <EmptyState title={t('shelves.notFound')} />
      </section>
    );

  const { item, parts } = detail;
  const single = item.object_key && parts.length === 0;
  const last = status.data && status.data.state !== 'never' ? status.data : null;
  return (
    <section className="shv-page" aria-labelledby="shv-item-h">
      {back}
      <div className="shv-detail">
        <div>
          {item.poster_url ? (
            <img className="shv-poster" src={item.poster_url} alt="" />
          ) : (
            <div className="shv-poster" aria-hidden="true" />
          )}
        </div>
        <div className="shv-head">
          <h1 id="shv-item-h">{item.title}</h1>
          <p className="shv-muted">
            {[t(`shelves.kind.${item.kind}` as 'shelves.kind.movie'), item.year, item.subtitle]
              .filter(Boolean)
              .join(' · ')}
          </p>
          <p>
            <ShelfChip source={item.source} />
          </p>
          {item.genres.length + item.tags.length > 0 && (
            <p>
              {[...item.genres, ...item.tags].map((g) => (
                <Tag key={g}>{g}</Tag>
              ))}
            </p>
          )}
          {item.synopsis && <p>{item.synopsis}</p>}
          {item.attribution && (
            <p className="shv-attribution">
              {t('shelves.attribution', { text: item.attribution })}
            </p>
          )}
          {item.overridden.length > 0 && <p className="shv-attribution">{t('shelves.edited')}</p>}
          <div className="shv-actions">
            {single && item.object_key && (
              <OpenButton
                container={item.container}
                webNative={item.web_native}
                title={item.title}
                objectKey={item.object_key}
                onOpen={setOpened}
              />
            )}
            {detail.can_edit && (
              <>
                <Button variant="secondary" size="sm" onClick={() => setEditing((v) => !v)}>
                  {editing ? t('shelves.editClose') : t('shelves.edit')}
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  loading={reindex.isPending}
                  onClick={() => reindex.mutate()}
                >
                  {t('shelves.reindex')}
                </Button>
              </>
            )}
          </div>
          {note && <Notice variant={note.ok ? 'info' : 'danger'}>{note.text}</Notice>}
          {last && (
            <p className="shv-attribution">
              {t('shelves.reindexLast', {
                objects: last.objects,
                added: last.added,
                vanished: last.vanished,
              })}
              {last.truncated ? ` ${t('shelves.reindexTruncated')}` : ''}
            </p>
          )}
        </div>
      </div>

      {opened && (
        <section aria-label={opened.title}>
          {opened.kind === 'video' || opened.kind === 'audio' ? (
            <ShelfPlayer
              key={opened.key}
              mountId={item.source.mount_id}
              objectKey={opened.key}
              kind={opened.kind}
              title={opened.title}
            />
          ) : opened.kind === 'cbz' ? (
            <CbzReader
              key={opened.key}
              mountId={item.source.mount_id}
              objectKey={opened.key}
              title={opened.title}
            />
          ) : (
            <PresignedReader
              key={opened.key}
              mountId={item.source.mount_id}
              objectKey={opened.key}
              kind={opened.kind}
              title={opened.title}
              seriesId={item.id}
            />
          )}
        </section>
      )}

      {editing && detail.can_edit && (
        <MetadataEditor key={String(item.overridden)} detail={detail} />
      )}

      {parts.length > 0 && (
        <section aria-labelledby="shv-parts-h">
          <h2 id="shv-parts-h">
            {t(section === 'cinema' ? 'shelves.episodes' : 'shelves.chapters')}
          </h2>
          <ul className="shv-parts">
            {parts.map((p) => (
              <li key={p.id} className="shv-part" data-current={opened?.key === p.object_key}>
                <span>
                  {section === 'cinema' ? `S${p.season ?? 1}E${p.number}` : `#${p.number}`}
                  {p.title ? ` ${p.title}` : ''}
                </span>
                <OpenButton
                  container={p.container}
                  webNative={p.web_native}
                  title={`${item.title} ${section === 'cinema' ? `S${p.season ?? 1}E${p.number}` : `#${p.number}`}`}
                  objectKey={p.object_key}
                  onOpen={setOpened}
                />
              </li>
            ))}
          </ul>
        </section>
      )}
    </section>
  );
}
