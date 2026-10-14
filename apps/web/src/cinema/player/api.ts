// Typed fetchers for the player and download routes. Components never build URLs. What the player calls:
//
// Documented (docs/specs/cinema-media.md section 8, catalog by p6-cinema-data, social by p6-cinema-social):
//   GET /cinema/titles/{id}/play[?variant=]     -> PlayTarget (403 code 900011 until age confirmation, D-184)
//   GET /cinema/episodes/{id}/play              -> PlayTarget
//   GET /cinema/episodes/{id}                   -> Episode (the "up next" card)
//   GET /cinema/titles/{id}                     -> TitleDetail (trailers, for `?trailer=1`)
//   progress: see progress.ts
//
// Implemented in phase 7 (spec section 10, `sr-api` routes/cinema/playback.rs; times in unix SECONDS, 403 code 900011
// for a restricted title):
//   GET  /cinema/assets/{asset_id}/playback -> {master_url, expires_at, refresh_at, duration_secs?, subtitles[{id,
//                                               language, label, forced, is_default, vtt_url, ass_url?}],
//                                               audio_tracks[], sprites_url?, sprite?, markers[]}
//   POST /cinema/assets/{asset_id}/refresh  -> {master_url, expires_at, refresh_at}
//
// ASSUMED (p6-cinema-ingest CIN-DL, no code yet):
//   GET  /cinema/titles/{id}/downloads[?episode=]  -> DownloadOptions   (403: not a Patron)
//   POST /cinema/titles/{id}/downloads             {rendition_id, episode_id?} -> DownloadJob
//   GET  /cinema/downloads/{jobId}                 -> DownloadJob
import { ApiError, apiFetch } from '../../api/client';
import { AGE_CONFIRMATION_REQUIRED } from '../../lib/ageGate';
import { LIVE_MEDIA_PREFIX, parseSpriteVtt } from './model';
import { progressApi } from './progress';
import type {
  DownloadJob,
  DownloadOptions,
  Marker,
  NextEpisode,
  PlaybackInfo,
  RefreshedSource,
  SpriteCue,
  SubtitleTrackInfo,
  WireAssetPlayback,
  WireEpisode,
  WireGrant,
  WirePlayTarget,
} from './types';

const enc = encodeURIComponent;

export interface PlaybackRequest {
  titleId: string;
  episodeId?: string | null;
  variantId?: string | null;
  /** Play the title's first trailer instead of the feature. */
  trailer?: boolean;
}

const secondsToMs = (s: number) => s * 1000;

export const getAssetPlayback = (assetId: string) =>
  apiFetch<WireAssetPlayback>(`/cinema/assets/${enc(assetId)}/playback`, { retries: 0 });

export async function refreshSource(assetId: string): Promise<RefreshedSource> {
  // Live TV tunes by channel, not asset: its media id is `tv:<channelId>` (p8-livetv-web).
  if (assetId.startsWith(LIVE_MEDIA_PREFIX)) {
    const { refreshTune } = await import('../tv/api');
    return refreshTune(assetId.slice(LIVE_MEDIA_PREFIX.length));
  }
  const g = await apiFetch<WireGrant>(`/cinema/assets/${enc(assetId)}/refresh`, {
    method: 'POST',
    retries: 0,
  });
  return {
    master_url: g.master_url,
    expires_at_ms: secondsToMs(g.expires_at),
    refresh_at_ms: g.refresh_at ? secondsToMs(g.refresh_at) : null,
  };
}

const subtitles = (w: WireAssetPlayback): SubtitleTrackInfo[] =>
  (w.subtitles ?? []).map((s) => ({
    id: s.id,
    language: s.language ?? '',
    label: s.label || s.language || 'Subtitles',
    vtt_url: s.vtt_url,
    ass_url: s.ass_url ?? null,
    is_default: s.is_default,
    forced: s.forced,
  }));

const markers = (m: WirePlayTarget['markers']): Marker[] =>
  m.map((x) => ({ kind: x.kind, start_s: x.start_ms / 1000, end_s: x.end_ms / 1000 }));

async function nextEpisode(id: string | null | undefined): Promise<NextEpisode | null> {
  if (!id) return null;
  try {
    const e = await apiFetch<WireEpisode>(`/cinema/episodes/${enc(id)}`, { retries: 0 });
    return {
      title_id: String(e.title_id),
      episode_id: String(e.id),
      season: e.season_number,
      number: e.number,
      title: e.title,
      still_url: e.still_url ?? null,
    };
  } catch {
    return null; // the card is a nicety: playback never waits on it
  }
}

const episodeLabel = (t: WirePlayTarget) =>
  t.episode_id
    ? [
        t.season_number != null && t.episode_number != null
          ? `S${t.season_number} · E${t.episode_number}`
          : null,
        t.episode_title,
      ]
        .filter(Boolean)
        .join(' · ') || null
    : null;

async function trailerTarget(titleId: string): Promise<WirePlayTarget> {
  const d = await apiFetch<{
    title: { title: string; age_restricted: boolean; runtime_min?: number | null };
    trailers: { id: string; playable: boolean; asset_id?: string | null }[];
  }>(`/cinema/titles/${enc(titleId)}`, { retries: 0 });
  const t = d.trailers.find((x) => x.playable);
  if (!t) throw new ApiError(404, 'No trailer');
  // The catalog withholds the asset of a restricted title until the account confirmed its age.
  if (!t.asset_id) {
    throw new ApiError(403, 'Age confirmation required', {
      code: AGE_CONFIRMATION_REQUIRED,
      message: 'Age confirmation required',
    });
  }
  return {
    title_id: titleId,
    asset_id: String(t.asset_id),
    title: d.title.title,
    age_restricted: d.title.age_restricted,
    runtime_min: d.title.runtime_min,
    markers: [],
  };
}

/**
 * One call for the watch route: the catalog's play target (which also enforces the age gate), the ingest grant for
 * its asset, the viewer's saved position and the next episode, composed into `PlaybackInfo`.
 */
export async function getPlayback(r: PlaybackRequest): Promise<PlaybackInfo> {
  const target = r.trailer
    ? await trailerTarget(r.titleId)
    : r.episodeId
      ? await apiFetch<WirePlayTarget>(`/cinema/episodes/${enc(r.episodeId)}/play`, { retries: 0 })
      : await apiFetch<WirePlayTarget>(
          `/cinema/titles/${enc(r.titleId)}/play${r.variantId ? `?variant=${enc(r.variantId)}` : ''}`,
          { retries: 0 },
        );
  const episodeId = target.episode_id ? String(target.episode_id) : null;
  const [asset, progress, next] = await Promise.all([
    getAssetPlayback(String(target.asset_id)),
    r.trailer ? Promise.resolve(null) : progressApi.get(r.titleId, episodeId).catch(() => null),
    r.trailer ? Promise.resolve(null) : nextEpisode(target.next_episode_id),
  ]);
  return {
    title_id: r.titleId,
    episode_id: episodeId,
    variant_id: target.variant_id ? String(target.variant_id) : null,
    media_id: String(target.asset_id),
    title: target.title,
    episode_label: episodeLabel(target),
    kind: episodeId ? 'series' : 'movie',
    poster_url: null,
    age_restricted: target.age_restricted,
    duration_s: asset.duration_secs ?? (target.runtime_min ? target.runtime_min * 60 : 0),
    master_url: asset.master_url,
    expires_at_ms: secondsToMs(asset.expires_at),
    refresh_at_ms: asset.refresh_at ? secondsToMs(asset.refresh_at) : null,
    subtitle_tracks: subtitles(asset),
    markers: markers(target.markers),
    sprites_url: asset.sprites_url ?? null,
    next,
    progress,
    trailer: r.trailer || undefined,
  };
}

/** The sprite map is a signed WebVTT file: fetched as text, not through the JSON client. */
export async function getSprites(url: string, signal?: AbortSignal): Promise<SpriteCue[]> {
  try {
    const res = await fetch(url, { signal, credentials: 'include' });
    if (!res.ok) return [];
    return parseSpriteVtt(await res.text(), url);
  } catch {
    return [];
  }
}

const epQuery = (episodeId?: string | null) => (episodeId ? `?episode=${enc(episodeId)}` : '');

export const getDownloadOptions = (titleId: string, episodeId?: string | null) =>
  apiFetch<DownloadOptions>(`/cinema/titles/${enc(titleId)}/downloads${epQuery(episodeId)}`, {
    retries: 0,
  });

export const startDownload = (titleId: string, renditionId: string, episodeId?: string | null) =>
  apiFetch<DownloadJob>(`/cinema/titles/${enc(titleId)}/downloads`, {
    method: 'POST',
    json: { rendition_id: renditionId, episode_id: episodeId ?? undefined },
    retries: 0,
  });

export const getDownloadJob = (jobId: string) =>
  apiFetch<DownloadJob>(`/cinema/downloads/${enc(jobId)}`, { retries: 0 });
