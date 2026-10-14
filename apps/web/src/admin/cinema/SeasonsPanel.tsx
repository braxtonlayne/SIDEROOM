// Seasons and episodes tables with inline edit (CIN-ADM-001). Deleting needs DELETE_CONTENT; everything else
// PUBLISH_MEDIA. An episode publishes only with a linked asset (the API answers 409 otherwise and we show it).
import { Badge, Button, ConfirmDialog, EmptyState, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { useAdminViewer } from '../ui';
import * as api from './api';
import { ArtworkPicker } from './ArtworkPicker';
import { Banner, intOrNull, useRun } from './hooks';

interface Props {
  detail: api.AdminTitleDetail;
  reload: () => Promise<void>;
}

export function SeasonsPanel({ detail, reload }: Props) {
  const t = useT();
  const viewer = useAdminViewer();
  const canEdit = viewer.can('PUBLISH_MEDIA');
  const canDelete = viewer.can('DELETE_CONTENT');
  const { busy, error, notice, run } = useRun();
  const [adding, setAdding] = useState(false);
  const [editSeason, setEditSeason] = useState<string | null>(null);
  const [removeSeason, setRemoveSeason] = useState<api.Season | null>(null);
  const seasons = [...detail.seasons].sort((a, b) => a.number - b.number);
  const nextNumber = (seasons.at(-1)?.number ?? 0) + 1;

  return (
    <div className="adm-cin-seasons">
      <Banner error={error} notice={notice} />
      {canEdit && (
        <div className="adm-actions">
          <Button variant="secondary" onClick={() => setAdding(true)} disabled={adding}>
            {t('admin.seasonsPanel.add')}
          </Button>
        </div>
      )}
      {adding && (
        <SeasonForm
          initial={{ number: nextNumber }}
          saveLabel={t('admin.seasonsPanel.add')}
          onCancel={() => setAdding(false)}
          onSave={async (b) => {
            await api.createSeason(detail.title.id, b);
            setAdding(false);
            await reload();
          }}
        />
      )}
      {seasons.length === 0 && !adding && (
        <EmptyState
          title={t('admin.seasonsPanel.empty.title')}
          description={t('admin.seasonsPanel.empty.description')}
        />
      )}
      {seasons.map((s) => (
        <section
          key={s.id}
          className="ss-card"
          aria-label={t('admin.seasonsPanel.season', { n: s.number })}
        >
          {editSeason === s.id ? (
            <SeasonForm
              initial={s}
              titleId={detail.title.id}
              saveLabel={t('admin.seasonsPanel.save')}
              onCancel={() => setEditSeason(null)}
              onSave={async (b) => {
                await api.updateSeason(s.id, b);
                setEditSeason(null);
                await reload();
              }}
              posterUrl={s.poster_url}
            />
          ) : (
            <div className="ss-row">
              <div className="ss-grow">
                <h2 className="ss-sub">
                  {s.name
                    ? t('admin.seasonsPanel.seasonNamed', { n: s.number, name: s.name })
                    : t('admin.seasonsPanel.season', { n: s.number })}
                </h2>
                <div className="ss-meta">
                  {t('admin.seasonsPanel.publishedCount', { n: s.episode_count })}
                </div>
              </div>
              {canEdit && (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={t('admin.seasonsPanel.editAria', { n: s.number })}
                  onClick={() => setEditSeason(s.id)}
                >
                  {t('admin.seasonsPanel.edit')}
                </Button>
              )}
              {canDelete && (
                <Button
                  size="sm"
                  variant="danger"
                  aria-label={t('admin.seasonsPanel.deleteAria', { n: s.number })}
                  onClick={() => setRemoveSeason(s)}
                >
                  {t('admin.seasonsPanel.delete')}
                </Button>
              )}
            </div>
          )}
          <EpisodesTable
            season={s}
            episodes={detail.episodes.filter((e) => e.season_id === s.id)}
            titleId={detail.title.id}
            canEdit={canEdit}
            canDelete={canDelete}
            reload={reload}
          />
        </section>
      ))}
      <ConfirmDialog
        open={!!removeSeason}
        danger
        title={t('admin.seasonsPanel.deleteTitle', { n: removeSeason?.number ?? '' })}
        description={t('admin.seasonsPanel.deleteDescription')}
        confirmLabel={t('admin.seasonsPanel.deleteConfirm')}
        loading={busy}
        onCancel={() => setRemoveSeason(null)}
        onConfirm={() => {
          const s = removeSeason;
          if (!s) return;
          void run(
            async () => {
              await api.deleteSeason(s.id);
              await reload();
              return true;
            },
            t('admin.seasonsPanel.deleted', { n: s.number }),
          ).then(() => setRemoveSeason(null));
        }}
      />
    </div>
  );
}

function SeasonForm({
  initial,
  titleId,
  posterUrl,
  saveLabel,
  onSave,
  onCancel,
}: {
  initial: Partial<api.Season> & { number: number };
  titleId?: string;
  posterUrl?: string | null;
  saveLabel: string;
  onSave: (b: api.SeasonInput) => Promise<void>;
  onCancel: () => void;
}) {
  const t = useT();
  const [number, setNumber] = useState(String(initial.number));
  const [name, setName] = useState(initial.name ?? '');
  const [synopsis, setSynopsis] = useState(initial.synopsis ?? '');
  const { busy, error, run } = useRun();
  const posterKey = api.keyFromUrl(posterUrl);
  const body = (poster_key: string | null | undefined = posterKey): api.SeasonInput => ({
    number: intOrNull(number) ?? 0,
    name: name.trim() || null,
    synopsis: synopsis.trim() || null,
    poster_key: poster_key ?? null,
  });
  return (
    <form
      className="adm-form"
      aria-label={t('admin.seasonsPanel.form.aria')}
      onSubmit={(e) => {
        e.preventDefault();
        void run(() => onSave(body()));
      }}
    >
      <TextInput
        label={t('admin.seasonsPanel.form.number')}
        type="number"
        value={number}
        onChange={(e) => setNumber(e.target.value)}
      />
      <TextInput
        label={t('admin.seasonsPanel.form.name')}
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      <TextArea
        label={t('admin.seasonsPanel.form.synopsis')}
        rows={2}
        value={synopsis}
        onChange={(e) => setSynopsis(e.target.value)}
      />
      {titleId && initial.id && (
        <ArtworkPicker
          label={t('admin.seasonsPanel.form.poster')}
          slot="season-poster"
          currentUrl={posterUrl}
          onUpload={async (file, progress) => {
            const key = await api.uploadArtwork(titleId, 'season-poster', file, progress);
            await onSave(body(key));
          }}
        />
      )}
      <Banner error={error} />
      <div className="adm-actions">
        <Button type="submit" loading={busy} disabled={intOrNull(number) === null}>
          {saveLabel}
        </Button>
        <Button variant="ghost" onClick={onCancel} disabled={busy}>
          {t('admin.seasonsPanel.cancel')}
        </Button>
      </div>
    </form>
  );
}

function EpisodesTable({
  season,
  episodes,
  titleId,
  canEdit,
  canDelete,
  reload,
}: {
  season: api.Season;
  episodes: api.Episode[];
  titleId: string;
  canEdit: boolean;
  canDelete: boolean;
  reload: () => Promise<void>;
}) {
  const t = useT();
  const { busy, error, notice, run } = useRun();
  const [editing, setEditing] = useState<string | 'new' | null>(null);
  const [remove, setRemove] = useState<api.Episode | null>(null);
  const sorted = [...episodes].sort((a, b) => a.number - b.number);
  const nextNumber = (sorted.at(-1)?.number ?? 0) + 1;

  const toggle = (e: api.Episode) =>
    void run(
      async () => {
        if (e.published) await api.unpublishEpisode(e.id);
        else await api.publishEpisode(e.id);
        await reload();
        return true;
      },
      e.published
        ? t('admin.seasonsPanel.ep.unpublished', { n: e.number })
        : t('admin.seasonsPanel.ep.published', { n: e.number }),
    );

  return (
    <div className="adm-table-wrap">
      <Banner error={error} notice={notice} />
      <table
        className="adm-table"
        aria-label={t('admin.seasonsPanel.ep.tableAria', { n: season.number })}
      >
        <thead>
          <tr>
            <th scope="col">{t('admin.seasonsPanel.col.no')}</th>
            <th scope="col">{t('admin.seasonsPanel.col.title')}</th>
            <th scope="col">{t('admin.seasonsPanel.col.aired')}</th>
            <th scope="col">{t('admin.seasonsPanel.col.runtime')}</th>
            <th scope="col">{t('admin.seasonsPanel.col.status')}</th>
            <th scope="col">{t('admin.seasonsPanel.col.actions')}</th>
          </tr>
        </thead>
        <tbody>
          {sorted.map((e) =>
            editing === e.id ? (
              <EpisodeEditRow
                key={e.id}
                episode={e}
                titleId={titleId}
                onCancel={() => setEditing(null)}
                onSave={async (b) => {
                  await api.updateEpisode(e.id, b);
                  setEditing(null);
                  await reload();
                }}
              />
            ) : (
              <tr key={e.id}>
                <td>{e.number}</td>
                <td>
                  {e.title || <span className="ss-na">{t('admin.seasonsPanel.ep.untitled')}</span>}
                </td>
                <td>{e.air_date ?? ''}</td>
                <td>
                  {e.runtime_min ? t('admin.seasonsPanel.ep.minutes', { n: e.runtime_min }) : ''}
                </td>
                <td>
                  <Badge variant={e.published ? 'success' : 'neutral'}>
                    {e.published
                      ? t('admin.seasonsPanel.ep.statusPublished')
                      : t('admin.seasonsPanel.ep.statusDraft')}
                  </Badge>{' '}
                  {!e.asset_id && (
                    <Badge variant="warning">{t('admin.seasonsPanel.ep.noVideo')}</Badge>
                  )}
                </td>
                <td>
                  <div className="adm-actions">
                    {canEdit && (
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('admin.seasonsPanel.ep.editAria', {
                          n: e.number,
                          season: season.number,
                        })}
                        onClick={() => setEditing(e.id)}
                      >
                        {t('admin.seasonsPanel.edit')}
                      </Button>
                    )}
                    {(canEdit || e.published) && (
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy}
                        aria-label={
                          e.published
                            ? t('admin.seasonsPanel.ep.unpublishAria', {
                                n: e.number,
                                season: season.number,
                              })
                            : t('admin.seasonsPanel.ep.publishAria', {
                                n: e.number,
                                season: season.number,
                              })
                        }
                        onClick={() => toggle(e)}
                      >
                        {e.published
                          ? t('admin.seasonsPanel.ep.unpublish')
                          : t('admin.seasonsPanel.ep.publish')}
                      </Button>
                    )}
                    {canDelete && (
                      <Button
                        size="sm"
                        variant="danger"
                        aria-label={t('admin.seasonsPanel.ep.deleteAria', {
                          n: e.number,
                          season: season.number,
                        })}
                        onClick={() => setRemove(e)}
                      >
                        {t('admin.seasonsPanel.delete')}
                      </Button>
                    )}
                  </div>
                </td>
              </tr>
            ),
          )}
          {editing === 'new' && (
            <EpisodeEditRow
              titleId={titleId}
              nextNumber={nextNumber}
              onCancel={() => setEditing(null)}
              onSave={async (b) => {
                await api.createEpisode(season.id, b);
                setEditing(null);
                await reload();
              }}
            />
          )}
        </tbody>
      </table>
      {canEdit && editing !== 'new' && (
        <Button
          size="sm"
          variant="secondary"
          onClick={() => setEditing('new')}
          aria-label={t('admin.seasonsPanel.ep.addAria', { n: season.number })}
        >
          {t('admin.seasonsPanel.ep.add')}
        </Button>
      )}
      <ConfirmDialog
        open={!!remove}
        danger
        title={t('admin.seasonsPanel.ep.deleteTitle', { n: remove?.number ?? '' })}
        description={t('admin.seasonsPanel.ep.deleteDescription', {
          name: remove?.title || t('admin.seasonsPanel.ep.thisEpisode'),
        })}
        confirmLabel={t('admin.seasonsPanel.ep.deleteConfirm')}
        loading={busy}
        onCancel={() => setRemove(null)}
        onConfirm={() => {
          const e = remove;
          if (!e) return;
          void run(
            async () => {
              await api.deleteEpisode(e.id);
              await reload();
              return true;
            },
            t('admin.seasonsPanel.ep.deleted', { n: e.number }),
          ).then(() => setRemove(null));
        }}
      />
    </div>
  );
}

function EpisodeEditRow({
  episode,
  titleId,
  nextNumber,
  onSave,
  onCancel,
}: {
  episode?: api.Episode;
  titleId: string;
  nextNumber?: number;
  onSave: (b: api.EpisodeInput) => Promise<void>;
  onCancel: () => void;
}) {
  const t = useT();
  const [number, setNumber] = useState(String(episode?.number ?? nextNumber ?? 1));
  const [absolute, setAbsolute] = useState(episode?.absolute_number?.toString() ?? '');
  const [title, setTitle] = useState(episode?.title ?? '');
  const [synopsis, setSynopsis] = useState(episode?.synopsis ?? '');
  const [aired, setAired] = useState(episode?.air_date ?? '');
  const [runtime, setRuntime] = useState(episode?.runtime_min?.toString() ?? '');
  const [stillKey, setStillKey] = useState<string | null>(api.keyFromUrl(episode?.still_url));
  const [stillUrl, setStillUrl] = useState<string | null>(episode?.still_url ?? null);
  const { busy, error, run } = useRun();
  const body = (): api.EpisodeInput => ({
    number: intOrNull(number) ?? 0,
    absolute_number: intOrNull(absolute),
    title: title.trim(),
    synopsis,
    air_date: aired.trim() || null,
    runtime_min: intOrNull(runtime),
    still_key: stillKey,
  });
  return (
    <tr className="adm-cin-editrow">
      <td colSpan={6}>
        <form
          className="adm-form"
          aria-label={
            episode
              ? t('admin.seasonsPanel.row.editAria', { n: episode.number })
              : t('admin.seasonsPanel.row.editNewAria')
          }
          onSubmit={(e) => {
            e.preventDefault();
            void run(() => onSave(body()));
          }}
        >
          <div className="adm-cin-grid">
            <TextInput
              label={t('admin.seasonsPanel.row.number')}
              type="number"
              value={number}
              onChange={(e) => setNumber(e.target.value)}
            />
            <TextInput
              label={t('admin.seasonsPanel.row.absolute')}
              type="number"
              value={absolute}
              onChange={(e) => setAbsolute(e.target.value)}
            />
            <TextInput
              label={t('admin.seasonsPanel.row.title')}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
            <TextInput
              label={t('admin.seasonsPanel.row.airDate')}
              type="date"
              value={aired}
              onChange={(e) => setAired(e.target.value)}
            />
            <TextInput
              label={t('admin.seasonsPanel.row.runtime')}
              type="number"
              value={runtime}
              onChange={(e) => setRuntime(e.target.value)}
            />
          </div>
          <TextArea
            label={t('admin.seasonsPanel.row.synopsis')}
            rows={3}
            value={synopsis}
            onChange={(e) => setSynopsis(e.target.value)}
          />
          <ArtworkPicker
            label={t('admin.seasonsPanel.row.still')}
            slot="still"
            currentUrl={stillUrl}
            onUpload={async (file, progress) => {
              const key = await api.uploadArtwork(titleId, 'still', file, progress);
              setStillKey(key);
              setStillUrl(URL.createObjectURL(file));
            }}
            {...(stillKey ? { onClear: async () => (setStillKey(null), setStillUrl(null)) } : {})}
          />
          <p className="ss-meta">{t('admin.seasonsPanel.row.stillNote')}</p>
          <Banner error={error} />
          <div className="adm-actions">
            <Button type="submit" loading={busy} disabled={intOrNull(number) === null}>
              {episode ? t('admin.seasonsPanel.row.save') : t('admin.seasonsPanel.ep.add')}
            </Button>
            <Button variant="ghost" onClick={onCancel} disabled={busy}>
              {t('admin.seasonsPanel.cancel')}
            </Button>
          </div>
        </form>
      </td>
    </tr>
  );
}
