// Typed fetchers for the player and download routes. Components never build URLs. Routes are assumptions
// until docs/specs/cinema-media.md lists them (see docs/dev/handoff/feat-p6-cinema-player.md):
//   GET  /cinema/titles/{id}/playback[?episode=]      -> PlaybackInfo   (403 code 900011: age confirmation)
//   POST /cinema/media/{mediaId}/refresh              -> {master_url, expires_at_ms}
//   GET  /cinema/titles/{id}/downloads[?episode=]     -> DownloadOptions (403: not a Patron)
//   POST /cinema/titles/{id}/downloads                {rendition_id, episode_id?} -> DownloadJob
//   GET  /cinema/downloads/{jobId}                    -> DownloadJob
import { apiFetch } from '../../api/client';
import { parseSpriteVtt } from './model';
import type {
  DownloadJob,
  DownloadOptions,
  PlaybackInfo,
  RefreshedSource,
  SpriteCue,
} from './types';

const enc = encodeURIComponent;
const epQuery = (episodeId?: string | null) => (episodeId ? `?episode=${enc(episodeId)}` : '');

export const getPlayback = (titleId: string, episodeId?: string | null) =>
  apiFetch<PlaybackInfo>(`/cinema/titles/${enc(titleId)}/playback${epQuery(episodeId)}`, {
    retries: 0,
  });

export const refreshSource = (mediaId: string) =>
  apiFetch<RefreshedSource>(`/cinema/media/${enc(mediaId)}/refresh`, {
    method: 'POST',
    retries: 0,
  });

/** The sprite map is a signed, public-path WebVTT file: fetched as text, not through the JSON client. */
export async function getSprites(url: string, signal?: AbortSignal): Promise<SpriteCue[]> {
  try {
    const res = await fetch(url, { signal, credentials: 'include' });
    if (!res.ok) return [];
    return parseSpriteVtt(await res.text(), url);
  } catch {
    return [];
  }
}

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
