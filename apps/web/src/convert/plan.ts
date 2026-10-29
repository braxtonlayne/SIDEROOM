// What `-c copy` can and cannot make of a source: the remux plan, the subtitle plan and the names of the outputs.
// Pure and tested; the converter only runs what this decides.
import { baseName } from '../storage/browser/keys';
import type {
  ConvertedSubtitle,
  ProbeResult,
  RemuxPlan,
  StreamInfo,
  SubtitleFormat,
} from './types';

/** Video that goes into MP4 as is. (`mpeg4`/`mpeg2video`/`vc1` are not browser codecs: they need a transcode.) */
const MP4_VIDEO = new Set(['h264', 'hevc', 'av1', 'vp9']);
/** Video that goes into WebM as is. */
const WEBM_VIDEO = new Set(['vp8', 'vp9', 'av1']);
/** Audio browsers play from MP4. AC-3, E-AC-3, DTS and TrueHD do not play in Chrome or Firefox: not copied. */
const MP4_AUDIO = new Set(['aac', 'mp3', 'flac', 'opus', 'alac']);
const WEBM_AUDIO = new Set(['opus', 'vorbis']);

/** Containers a browser plays as is (the server's `web_native` is the authority; this is for the picker). */
export const NATIVE_EXTENSIONS = new Set([
  'mp4',
  'm4v',
  'webm',
  'mp3',
  'm4a',
  'ogg',
  'oga',
  'opus',
  'flac',
  'wav',
]);
/** Containers remux is offered for. */
export const REMUXABLE_EXTENSIONS = new Set([
  'mkv',
  'avi',
  'mov',
  'ts',
  'm2ts',
  'mts',
  'wmv',
  'flv',
  'mpg',
  'mpeg',
]);

export const extensionOf = (name: string): string => {
  const i = name.lastIndexOf('.');
  return i < 0 ? '' : name.slice(i + 1).toLowerCase();
};

/** True when the file is a video container that usually needs conversion (MKV, AVI, MOV, TS ...). */
export const needsConversion = (name: string): boolean =>
  REMUXABLE_EXTENSIONS.has(extensionOf(name));

/** Decides container and streams. Does not look at what the browser can decode: that is the decider's job. */
export function planRemux(probe: ProbeResult): RemuxPlan {
  const plan: RemuxPlan = {
    container: null,
    map: [],
    dropped: [],
    videoCodec: null,
    audioCodecs: [],
  };
  const videos = probe.streams.filter((s) => s.kind === 'video');
  // Cover art in MP3/MKV shows up as a video stream of codec mjpeg/png: not the film.
  const real = videos.filter((s) => !['mjpeg', 'png', 'bmp', 'gif'].includes(s.codec));
  const video = real[0];
  if (!video) {
    plan.reason = 'no-video';
    return plan;
  }
  const audios = probe.streams.filter((s) => s.kind === 'audio');
  // WebM only for VP8 (MP4 cannot hold it) with WebM audio; every other supported video goes to MP4.
  const webm = video.codec === 'vp8';
  if (!(webm ? WEBM_VIDEO : MP4_VIDEO).has(video.codec)) {
    plan.reason = 'video-codec';
    return plan;
  }
  const okAudio = webm ? WEBM_AUDIO : MP4_AUDIO;
  const keep = audios.filter((a) => okAudio.has(a.codec));
  if (audios.length > 0 && keep.length === 0) {
    plan.reason = 'audio-codec';
    return plan;
  }
  plan.container = webm ? 'webm' : 'mp4';
  plan.videoCodec = video.codec;
  plan.map = [video.index, ...keep.map((a) => a.index)];
  plan.audioCodecs = [...new Set(keep.map((a) => a.codec))];
  for (const v of videos)
    if (v !== video) plan.dropped.push({ index: v.index, reason: 'extra-video' });
  for (const a of audios)
    if (!keep.includes(a)) plan.dropped.push({ index: a.index, reason: 'incompatible-audio' });
  for (const s of probe.streams)
    if (s.kind === 'subtitle' && !s.textual)
      plan.dropped.push({ index: s.index, reason: 'bitmap-subtitle' });
  return plan;
}

/** `Movies/Film (2019).mkv` to `converted/Movies/Film (2019).mp4`. The original keeps its place. */
export function convertedKey(
  originalKey: string,
  ext: 'mp4' | 'webm' | SubtitleFormat | string,
): string {
  const dot = originalKey.lastIndexOf('.');
  const slash = originalKey.lastIndexOf('/');
  const stem = dot > slash + 1 ? originalKey.slice(0, dot) : originalKey;
  return `converted/${stem}.${ext}`;
}

/** The key of a sidecar subtitle beside the converted output: `converted/Film.eng.srt`. */
export function subtitleKey(originalKey: string, s: { name: string }): string {
  const slash = originalKey.lastIndexOf('/');
  const dir = slash < 0 ? '' : originalKey.slice(0, slash + 1);
  return `converted/${dir}${s.name}`;
}

export const MIME: Record<string, string> = {
  mp4: 'video/mp4',
  webm: 'video/webm',
  srt: 'application/x-subrip',
  ass: 'text/x-ssa',
};

const ASS_CODECS = new Set(['ass', 'ssa']);

export interface SubtitlePlanItem {
  stream: StreamInfo;
  format: SubtitleFormat;
  name: string;
}

/** One output per text subtitle stream; names `<stem>.<lang>.srt`, numbered when two share a language. */
export function planSubtitles(
  probe: ProbeResult,
  sourceName: string,
): { items: SubtitlePlanItem[]; skipped: StreamInfo[] } {
  const stem = baseName(sourceName).replace(/\.[^.]*$/, '') || 'subtitles';
  const subs = probe.streams.filter((s) => s.kind === 'subtitle');
  const skipped = subs.filter((s) => !s.textual);
  const seen = new Map<string, number>();
  const items: SubtitlePlanItem[] = [];
  for (const stream of subs.filter((s) => s.textual)) {
    const format: SubtitleFormat = ASS_CODECS.has(stream.codec) ? 'ass' : 'srt';
    const lang = (stream.language ?? 'und').replace(/[^A-Za-z0-9-]/g, '') || 'und';
    const n = (seen.get(lang) ?? 0) + 1;
    seen.set(lang, n);
    items.push({ stream, format, name: `${stem}.${lang}${n > 1 ? `.${n}` : ''}.${format}` });
  }
  return { items, skipped };
}

/** For the picker: describes a subtitle track to a person ("English", "eng 2"). */
export const subtitleLabel = (s: Pick<ConvertedSubtitle, 'language' | 'title'>): string =>
  s.title || s.language || 'und';
