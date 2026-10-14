// Seasons and episodes tables with inline edit (CIN-ADM-001). Deleting needs DELETE_CONTENT; everything else
// PUBLISH_MEDIA. An episode publishes only with a linked asset (the API answers 409 otherwise and we show it).
import { Badge, Button, ConfirmDialog, EmptyState, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useAdminViewer } from '../ui';
import * as api from './api';
import { ArtworkPicker } from './ArtworkPicker';
import { Banner, intOrNull, useRun } from './hooks';

interface Props {
  detail: api.AdminTitleDetail;
  reload: () => Promise<void>;
}

export function SeasonsPanel({ detail, reload }: Props) {
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
            Add season
          </Button>
        </div>
      )}
      {adding && (
        <SeasonForm
          initial={{ number: nextNumber }}
          saveLabel="Add season"
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
          title="No seasons yet"
          description="Add a season, then its episodes. A series publishes once an episode with a video is published."
        />
      )}
      {seasons.map((s) => (
        <section key={s.id} className="ss-card" aria-label={`Season ${s.number}`}>
          {editSeason === s.id ? (
            <SeasonForm
              initial={s}
              titleId={detail.title.id}
              saveLabel="Save season"
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
                  Season {s.number}
                  {s.name ? `: ${s.name}` : ''}
                </h2>
                <div className="ss-meta">
                  {s.episode_count} published episode{s.episode_count === 1 ? '' : 's'}
                </div>
              </div>
              {canEdit && (
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={`Edit season ${s.number}`}
                  onClick={() => setEditSeason(s.id)}
                >
                  Edit
                </Button>
              )}
              {canDelete && (
                <Button
                  size="sm"
                  variant="danger"
                  aria-label={`Delete season ${s.number}`}
                  onClick={() => setRemoveSeason(s)}
                >
                  Delete
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
        title={`Delete season ${removeSeason?.number ?? ''}`}
        description="The season and all its episodes are removed. If that was the last published content, the title is unpublished."
        confirmLabel="Delete season"
        loading={busy}
        onCancel={() => setRemoveSeason(null)}
        onConfirm={() => {
          const s = removeSeason;
          if (!s) return;
          void run(async () => {
            await api.deleteSeason(s.id);
            await reload();
            return true;
          }, `Season ${s.number} deleted.`).then(() => setRemoveSeason(null));
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
      aria-label="Season"
      onSubmit={(e) => {
        e.preventDefault();
        void run(() => onSave(body()));
      }}
    >
      <TextInput
        label="Season number"
        type="number"
        value={number}
        onChange={(e) => setNumber(e.target.value)}
      />
      <TextInput label="Season name" value={name} onChange={(e) => setName(e.target.value)} />
      <TextArea
        label="Season synopsis"
        rows={2}
        value={synopsis}
        onChange={(e) => setSynopsis(e.target.value)}
      />
      {titleId && initial.id && (
        <ArtworkPicker
          label="Season poster"
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
          Cancel
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
      `Episode ${e.number} ${e.published ? 'unpublished' : 'published'}.`,
    );

  return (
    <div className="adm-table-wrap">
      <Banner error={error} notice={notice} />
      <table className="adm-table" aria-label={`Season ${season.number} episodes`}>
        <thead>
          <tr>
            <th scope="col">No.</th>
            <th scope="col">Title</th>
            <th scope="col">Aired</th>
            <th scope="col">Runtime</th>
            <th scope="col">Status</th>
            <th scope="col">Actions</th>
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
                <td>{e.title || <span className="ss-na">Untitled</span>}</td>
                <td>{e.air_date ?? ''}</td>
                <td>{e.runtime_min ? `${e.runtime_min} min` : ''}</td>
                <td>
                  <Badge variant={e.published ? 'success' : 'neutral'}>
                    {e.published ? 'published' : 'draft'}
                  </Badge>{' '}
                  {!e.asset_id && <Badge variant="warning">no video</Badge>}
                </td>
                <td>
                  <div className="adm-actions">
                    {canEdit && (
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={`Edit episode ${e.number} of season ${season.number}`}
                        onClick={() => setEditing(e.id)}
                      >
                        Edit
                      </Button>
                    )}
                    {(canEdit || e.published) && (
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy}
                        aria-label={`${e.published ? 'Unpublish' : 'Publish'} episode ${e.number} of season ${season.number}`}
                        onClick={() => toggle(e)}
                      >
                        {e.published ? 'Unpublish' : 'Publish'}
                      </Button>
                    )}
                    {canDelete && (
                      <Button
                        size="sm"
                        variant="danger"
                        aria-label={`Delete episode ${e.number} of season ${season.number}`}
                        onClick={() => setRemove(e)}
                      >
                        Delete
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
          aria-label={`Add episode to season ${season.number}`}
        >
          Add episode
        </Button>
      )}
      <ConfirmDialog
        open={!!remove}
        danger
        title={`Delete episode ${remove?.number ?? ''}`}
        description={`${remove?.title || 'This episode'} is removed with its skip markers and member progress.`}
        confirmLabel="Delete episode"
        loading={busy}
        onCancel={() => setRemove(null)}
        onConfirm={() => {
          const e = remove;
          if (!e) return;
          void run(async () => {
            await api.deleteEpisode(e.id);
            await reload();
            return true;
          }, `Episode ${e.number} deleted.`).then(() => setRemove(null));
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
  const [number, setNumber] = useState(String(episode?.number ?? nextNumber ?? 1));
  const [absolute, setAbsolute] = useState(episode?.absolute_number?.toString() ?? '');
  const [title, setTitle] = useState(episode?.title ?? '');
  const [synopsis, setSynopsis] = useState(episode?.synopsis ?? '');
  const [aired, setAired] = useState(episode?.air_date ?? '');
  const [runtime, setRuntime] = useState(episode?.runtime_min?.toString() ?? '');
  const [stillKey, setStillKey] = useState<string | null>(api.keyFromUrl(episode?.still_url));
  const [stillUrl, setStillUrl] = useState<string | null>(episode?.still_url ?? null);
  const { busy, error, run } = useRun();
  const where = episode ? `episode ${episode.number}` : 'new episode';
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
          aria-label={`Edit ${where}`}
          onSubmit={(e) => {
            e.preventDefault();
            void run(() => onSave(body()));
          }}
        >
          <div className="adm-cin-grid">
            <TextInput
              label="Episode number"
              type="number"
              value={number}
              onChange={(e) => setNumber(e.target.value)}
            />
            <TextInput
              label="Absolute number"
              type="number"
              value={absolute}
              onChange={(e) => setAbsolute(e.target.value)}
            />
            <TextInput
              label="Episode title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
            <TextInput
              label="Air date"
              type="date"
              value={aired}
              onChange={(e) => setAired(e.target.value)}
            />
            <TextInput
              label="Runtime (minutes)"
              type="number"
              value={runtime}
              onChange={(e) => setRuntime(e.target.value)}
            />
          </div>
          <TextArea
            label="Episode synopsis"
            rows={3}
            value={synopsis}
            onChange={(e) => setSynopsis(e.target.value)}
          />
          <ArtworkPicker
            label="Episode still"
            slot="still"
            currentUrl={stillUrl}
            onUpload={async (file, progress) => {
              const key = await api.uploadArtwork(titleId, 'still', file, progress);
              setStillKey(key);
              setStillUrl(URL.createObjectURL(file));
            }}
            {...(stillKey ? { onClear: async () => (setStillKey(null), setStillUrl(null)) } : {})}
          />
          <p className="ss-meta">A new still is attached when you save the episode.</p>
          <Banner error={error} />
          <div className="adm-actions">
            <Button type="submit" loading={busy} disabled={intOrNull(number) === null}>
              {episode ? 'Save episode' : 'Add episode'}
            </Button>
            <Button variant="ghost" onClick={onCancel} disabled={busy}>
              Cancel
            </Button>
          </div>
        </form>
      </td>
    </tr>
  );
}
