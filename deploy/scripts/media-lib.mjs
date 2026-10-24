// Helpers of the media pipeline smoke (deploy/scripts/media-smoke.mjs): HLS playlist parsing and the live
// numbering check. Pure functions, no I/O, so `node --test deploy/scripts/media-lib.test.mjs` covers them
// without ffmpeg, a server or a network (docs/dev/media-pipeline-test.md).

/** `KEY=VALUE,KEY="quoted,value"` attribute lists of HLS tags. */
export function parseAttrs(text) {
  const out = {};
  const re = /([A-Z0-9-]+)=("([^"]*)"|[^,]*)/g;
  let m;
  while ((m = re.exec(text))) out[m[1]] = m[3] !== undefined ? m[3] : m[2];
  return out;
}

/** A master playlist: the video variants and the `EXT-X-MEDIA` renditions. */
export function parseMaster(text) {
  const lines = text.split(/\r?\n/).map((l) => l.trim());
  if (lines[0] !== '#EXTM3U') throw new Error('not an HLS playlist (no #EXTM3U)');
  const variants = [];
  const media = [];
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    if (l.startsWith('#EXT-X-STREAM-INF:')) {
      const attrs = parseAttrs(l.slice('#EXT-X-STREAM-INF:'.length));
      let uri = '';
      for (let j = i + 1; j < lines.length; j++) {
        if (lines[j] && !lines[j].startsWith('#')) {
          uri = lines[j];
          break;
        }
      }
      const [w, h] = (attrs.RESOLUTION ?? '0x0').split('x').map(Number);
      variants.push({ attrs, uri, width: w, height: h, bandwidth: Number(attrs.BANDWIDTH ?? 0) });
    } else if (l.startsWith('#EXT-X-MEDIA:')) {
      media.push(parseAttrs(l.slice('#EXT-X-MEDIA:'.length)));
    }
  }
  return { variants, media };
}

/**
 * A media playlist (VOD or live window). Each segment carries its media sequence number `seq`, the
 * discontinuity counter `disc` in force for it, the init segment `map` in force and whether a
 * discontinuity tag introduced it.
 */
export function parseMedia(text) {
  const lines = text.split(/\r?\n/).map((l) => l.trim());
  if (lines[0] !== '#EXTM3U') throw new Error('not an HLS playlist (no #EXTM3U)');
  let mediaSequence = 0;
  let discSeq = 0;
  let targetDuration = 0;
  let map = null;
  let playlistType = null;
  let endlist = false;
  let pending = null;
  let discTag = false;
  let disc = 0;
  const segments = [];
  for (const l of lines) {
    if (l.startsWith('#EXT-X-MEDIA-SEQUENCE:')) mediaSequence = Number(l.split(':')[1]);
    else if (l.startsWith('#EXT-X-DISCONTINUITY-SEQUENCE:')) {
      discSeq = Number(l.split(':')[1]);
      disc = discSeq;
    } else if (l.startsWith('#EXT-X-TARGETDURATION:')) targetDuration = Number(l.split(':')[1]);
    else if (l.startsWith('#EXT-X-PLAYLIST-TYPE:')) playlistType = l.split(':')[1];
    else if (l === '#EXT-X-ENDLIST') endlist = true;
    else if (l === '#EXT-X-DISCONTINUITY') {
      discTag = true;
      disc += 1;
    } else if (l.startsWith('#EXT-X-MAP:')) map = parseAttrs(l.slice('#EXT-X-MAP:'.length)).URI ?? null;
    else if (l.startsWith('#EXTINF:')) pending = Number(l.slice('#EXTINF:'.length).split(',')[0]);
    else if (l && !l.startsWith('#') && pending !== null) {
      segments.push({
        uri: l,
        duration: pending,
        seq: mediaSequence + segments.length,
        disc,
        map,
        afterDiscontinuity: discTag,
      });
      pending = null;
      discTag = false;
    }
  }
  return { mediaSequence, discSeq, targetDuration, playlistType, endlist, map, segments };
}

/** A URL without its query: signed URLs change on every playlist fetch, the object does not. */
export function stable(uri) {
  return uri.split('?')[0];
}

/**
 * The Live TV numbering rule (D-267, D-285) over samples of a channel's live video playlist taken in
 * time order: `[{ at, media }]`, `media` as `parseMedia` returns it. `kindOf(segment)` names the
 * programme a segment belongs to (for example `slate` or `episode`).
 *
 * Returns the list of problems (empty when the rule holds):
 * - one number always names the same object, whatever else is in the window;
 * - the newest number never goes backwards between samples, nor does the discontinuity counter;
 * - every number from the first to the last seen was listed at some point (no jump);
 * - wherever the kind changes between neighbouring numbers, a discontinuity tag introduces the new one.
 */
export function checkLiveNumbering(samples, kindOf) {
  const problems = [];
  const seen = new Map();
  let lastNewest = -Infinity;
  let lastDisc = -Infinity;
  samples.forEach((s, n) => {
    const segs = s.media.segments;
    if (!segs.length) {
      problems.push(`sample ${n}: empty window`);
      return;
    }
    const newest = segs[segs.length - 1];
    if (newest.seq < lastNewest) {
      problems.push(`sample ${n}: newest number ${newest.seq} went back from ${lastNewest}`);
    }
    if (newest.disc < lastDisc) {
      problems.push(`sample ${n}: discontinuity counter ${newest.disc} went back from ${lastDisc}`);
    }
    lastNewest = Math.max(lastNewest, newest.seq);
    lastDisc = Math.max(lastDisc, newest.disc);
    for (const g of segs) {
      const key = stable(g.uri);
      const prev = seen.get(g.seq);
      if (prev && prev.key !== key) {
        problems.push(`number ${g.seq} named ${prev.key} and then ${key}`);
      } else if (!prev) {
        seen.set(g.seq, { key, kind: kindOf(g), disc: g.disc, afterDiscontinuity: g.afterDiscontinuity });
      } else if (g.afterDiscontinuity) {
        prev.afterDiscontinuity = true;
      }
    }
  });
  const nums = [...seen.keys()].sort((a, b) => a - b);
  for (let i = 1; i < nums.length; i++) {
    if (nums[i] !== nums[i - 1] + 1) {
      problems.push(`numbers jump from ${nums[i - 1]} to ${nums[i]}`);
    }
    const a = seen.get(nums[i - 1]);
    const b = seen.get(nums[i]);
    if (nums[i] === nums[i - 1] + 1 && a.kind !== b.kind && !b.afterDiscontinuity && b.disc <= a.disc) {
      problems.push(`no discontinuity between ${a.kind} (${nums[i - 1]}) and ${b.kind} (${nums[i]})`);
    }
  }
  return problems;
}

/** Kinds in order of first appearance by number, for example `['slate', 'episode', 'slate']`. */
export function kindRuns(samples, kindOf) {
  const byNum = new Map();
  for (const s of samples) for (const g of s.media.segments) byNum.set(g.seq, kindOf(g));
  const runs = [];
  for (const n of [...byNum.keys()].sort((a, b) => a - b)) {
    const k = byNum.get(n);
    if (runs[runs.length - 1] !== k) runs.push(k);
  }
  return runs;
}

/** Atom (box) names at the top level of an MP4, in file order, from the first `limit` bytes. */
export function topLevelBoxes(buf, limit = buf.length) {
  const out = [];
  let pos = 0;
  while (pos + 8 <= Math.min(buf.length, limit)) {
    let size = buf.readUInt32BE(pos);
    const name = buf.toString('latin1', pos + 4, pos + 8);
    out.push(name);
    if (size === 1) {
      if (pos + 16 > buf.length) break;
      size = Number(buf.readBigUInt64BE(pos + 8));
    } else if (size === 0) {
      break;
    }
    if (size < 8) break;
    pos += size;
  }
  return out;
}

/** A line-oriented summary of a WebVTT file: cue count and the texts, or throws on a bad header. */
export function parseVtt(text) {
  if (!text.startsWith('WEBVTT')) throw new Error('missing WEBVTT header');
  const cues = [];
  const blocks = text.split(/\r?\n\r?\n/);
  for (const b of blocks) {
    const lines = b.split(/\r?\n/);
    const i = lines.findIndex((l) => l.includes('-->'));
    if (i >= 0) cues.push({ timing: lines[i].trim(), text: lines.slice(i + 1).join('\n') });
  }
  return cues;
}
