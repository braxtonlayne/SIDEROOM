// `probe` for the ffmpeg.wasm core, which has no usable ffprobe output of its own for us: the log of
// `ffmpeg -i <file>` names the container, the duration and every stream. Pure functions, so they are tested
// against real log text without the core.
import type { ProbeResult, StreamInfo } from './types';

/** Subtitle codecs that are text: they become `.srt` or `.ass`. Bitmap formats (PGS, DVD, DVB, XSUB) do not. */
export const TEXT_SUBTITLE_CODECS = new Set([
  'subrip',
  'srt',
  'ass',
  'ssa',
  'mov_text',
  'webvtt',
  'text',
  'subviewer',
  'microdvd',
  'jacosub',
  'sami',
  'realtext',
]);

const STREAM =
  /^\s*Stream #\d+:(\d+)(?:\[[^\]]*\])?(?:\(([^)]*)\))?[^:]*:\s*(Video|Audio|Subtitle|Data|Attachment):\s*(.*)$/;

/** `00:01:02.50` to seconds. */
export function clock(s: string): number | null {
  const m = /^(\d+):(\d{2}):(\d{2}(?:\.\d+)?)$/.exec(s.trim());
  return m ? Number(m[1]) * 3600 + Number(m[2]) * 60 + Number(m[3]) : null;
}

function codecOf(rest: string): { codec: string; profile?: string } {
  const m = /^([A-Za-z0-9_]+)\s*(?:\(([^)]*)\))?/.exec(rest);
  const codec = (m?.[1] ?? '').toLowerCase();
  const profile = m?.[2];
  // "(High)", "(LC)", "(Main 10)" are profiles; "([0][0][0][0] / 0x2000)" is a tag and means nothing to us.
  return profile && !profile.includes('[') && !profile.includes('/')
    ? { codec, profile }
    : { codec };
}

/** Parses the lines ffmpeg wrote for `-i <file>` with no output. */
export function parseProbeLog(lines: string[]): ProbeResult {
  const out: ProbeResult = { container: '', durationSec: null, bitrateKbps: null, streams: [] };
  let current: StreamInfo | null = null;
  let inStreamMeta = false;
  for (const line of lines) {
    const input = /^\s*Input #0,\s*(.*?),\s*from\b/.exec(line);
    if (input) {
      out.container = input[1] ?? '';
      continue;
    }
    const dur =
      /^\s*Duration:\s*([\d:.]+|N\/A)(?:,\s*start:\s*[-\d.]+)?(?:,\s*bitrate:\s*(\d+)\s*kb\/s|,\s*bitrate:\s*N\/A)?/.exec(
        line,
      );
    if (dur) {
      out.durationSec = dur[1] === 'N/A' ? null : clock(dur[1] ?? '');
      out.bitrateKbps = dur[2] ? Number(dur[2]) : null;
      continue;
    }
    const s = STREAM.exec(line);
    if (s) {
      const [, idx, lang, type, rest = ''] = s;
      const { codec, profile } = codecOf(rest);
      const kind =
        type === 'Video'
          ? 'video'
          : type === 'Audio'
            ? 'audio'
            : type === 'Subtitle'
              ? 'subtitle'
              : 'other';
      const info: StreamInfo = { index: Number(idx), kind, codec, ...(profile ? { profile } : {}) };
      if (lang && lang !== 'und') info.language = lang;
      if (kind === 'video') {
        // Skip the hex tag "0x31637661" before looking for WxH.
        const wh = /(?:^|[\s,])(\d{2,5})x(\d{2,5})(?=[\s,[]|$)/.exec(rest);
        if (wh) {
          info.width = Number(wh[1]);
          info.height = Number(wh[2]);
        }
        const fps = /([\d.]+)\s*fps/.exec(rest);
        if (fps) info.fps = Number(fps[1]);
      } else if (kind === 'audio') {
        const hz = /(\d+)\s*Hz/.exec(rest);
        if (hz) info.sampleRate = Number(hz[1]);
        const ch = /Hz,\s*([^,]+)/.exec(rest)?.[1]?.trim();
        if (ch) info.channels = channelCount(ch);
      } else if (kind === 'subtitle') {
        info.textual = TEXT_SUBTITLE_CODECS.has(codec);
      }
      const br = /(\d+)\s*kb\/s/.exec(rest);
      if (br) info.bitrateKbps = Number(br[1]);
      out.streams.push(info);
      current = info;
      inStreamMeta = false;
      continue;
    }
    if (current) {
      if (/^\s{4}Metadata:\s*$/.test(line)) inStreamMeta = true;
      else if (inStreamMeta) {
        const t = /^\s{6,}title\s*:\s*(.+?)\s*$/i.exec(line);
        if (t) current.title = t[1];
      }
    }
  }
  return out;
}

function channelCount(layout: string): number | undefined {
  if (/^mono/.test(layout)) return 1;
  if (/^stereo/.test(layout)) return 2;
  const dotted = /^(\d+)\.(\d)/.exec(layout);
  if (dotted) return Number(dotted[1]) + Number(dotted[2]);
  const n = /^(\d+) channels?/.exec(layout);
  return n ? Number(n[1]) : undefined;
}

/** Parses ffmpeg's progress text (`... time=00:00:12.34 ...`) to seconds of output so far, or `null`. */
export function progressSeconds(line: string): number | null {
  const m = /\btime=\s*(\d+:\d{2}:\d{2}(?:\.\d+)?)/.exec(line);
  return m ? clock(m[1] ?? '') : null;
}
