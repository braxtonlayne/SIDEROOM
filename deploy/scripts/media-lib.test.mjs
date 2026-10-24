// Run with: node --test deploy/scripts/media-lib.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  checkLiveNumbering,
  kindRuns,
  parseAttrs,
  parseMaster,
  parseMedia,
  parseVtt,
  topLevelBoxes,
} from './media-lib.mjs';

const MASTER = `#EXTM3U
#EXT-X-VERSION:7
#EXT-X-INDEPENDENT-SEGMENTS
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="English",LANGUAGE="eng",DEFAULT=YES,AUTOSELECT=YES,CHANNELS="2",URI="a/00-eng-stereo/index.m3u8?sig=1"
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="Spanish, Spain",LANGUAGE="spa",DEFAULT=NO,URI="a/01-spa-stereo/index.m3u8?sig=1"
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English",LANGUAGE="eng",URI="s/00-eng.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=3500000,RESOLUTION=1280x720,CODECS="avc1.64001f,mp4a.40.2",AUDIO="aud",SUBTITLES="subs"
v/720p/index.m3u8?sig=1
#EXT-X-STREAM-INF:BANDWIDTH=1800000,RESOLUTION=854x480,CODECS="avc1.64001e,mp4a.40.2",AUDIO="aud",SUBTITLES="subs"
v/480p/index.m3u8?sig=1
`;

test('master playlists', () => {
  const m = parseMaster(MASTER);
  assert.deepEqual(
    m.variants.map((v) => [v.height, v.uri]),
    [
      [720, 'v/720p/index.m3u8?sig=1'],
      [480, 'v/480p/index.m3u8?sig=1'],
    ],
  );
  const audio = m.media.filter((x) => x.TYPE === 'AUDIO');
  assert.deepEqual(
    audio.map((x) => [x.LANGUAGE, x.DEFAULT]),
    [
      ['eng', 'YES'],
      ['spa', 'NO'],
    ],
  );
  assert.equal(audio[1].NAME, 'Spanish, Spain');
  assert.equal(parseAttrs('A=1,B="x,y",C=z').B, 'x,y');
  assert.throws(() => parseMaster('nope'));
});

const VOD = `#EXTM3U
#EXT-X-VERSION:7
#EXT-X-TARGETDURATION:6
#EXT-X-MEDIA-SEQUENCE:0
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-MAP:URI="init.mp4?sig=2"
#EXTINF:6.000000,
seg_00000.m4s?sig=2
#EXTINF:2.000000,
seg_00001.m4s?sig=2
#EXT-X-ENDLIST
`;

test('vod media playlists', () => {
  const p = parseMedia(VOD);
  assert.equal(p.endlist, true);
  assert.equal(p.map, 'init.mp4?sig=2');
  assert.deepEqual(
    p.segments.map((s) => [s.seq, s.duration]),
    [
      [0, 6],
      [1, 2],
    ],
  );
});

/** A live window of `n` segments ending at number `newest`; `kindAt(seq)` names the object. */
function window(newest, n, kindAt, discAt) {
  const first = newest - n + 1;
  let text = `#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:6\n#EXT-X-MEDIA-SEQUENCE:${first}\n`;
  text += `#EXT-X-DISCONTINUITY-SEQUENCE:${discAt(first)}\n`;
  for (let s = first; s <= newest; s++) {
    if (s > first && discAt(s) !== discAt(s - 1)) text += '#EXT-X-DISCONTINUITY\n';
    text += `#EXT-X-MAP:URI="/cdn/${kindAt(s)}/init.mp4?x=${s}"\n#EXTINF:6.0,\n/cdn/${kindAt(s)}/seg${s}.m4s?x=${s}\n`;
  }
  return parseMedia(text);
}

const kindOf = (g) => (g.uri.includes('/cdn/slate/') ? 'slate' : 'episode');
// slate up to 12, episode 13..15, slate again from 16.
const kindAt = (s) => (s >= 13 && s <= 15 ? 'episode' : 'slate');
const discAt = (s) => (s < 13 ? 0 : s <= 15 ? 1 : 2);

test('numbering holds across the slate boundary', () => {
  const samples = [];
  for (let newest = 9; newest <= 19; newest++) {
    samples.push({ at: newest, media: window(newest, 4, kindAt, discAt) });
  }
  assert.deepEqual(checkLiveNumbering(samples, kindOf), []);
  assert.deepEqual(kindRuns(samples, kindOf), ['slate', 'episode', 'slate']);
});

test('numbering problems are named', () => {
  const ok = [];
  for (let newest = 9; newest <= 19; newest++) {
    ok.push({ at: newest, media: window(newest, 4, kindAt, discAt) });
  }
  // The newest number goes back.
  const back = [...ok.slice(0, 5), ok[2]];
  assert.ok(checkLiveNumbering(back, kindOf).some((p) => p.includes('went back')));
  // A skipped sample leaves a hole in the numbers.
  const hole = [ok[0], ok[1], ok[6], ok[7]];
  assert.ok(checkLiveNumbering(hole, kindOf).some((p) => p.includes('jump')));
  // The same number names another object later.
  const renamed = parseMedia('#EXTM3U\n#EXT-X-MEDIA-SEQUENCE:9\n#EXTINF:6,\n/cdn/other/seg9.m4s\n');
  assert.ok(
    checkLiveNumbering([ok[0], { at: 1, media: renamed }], kindOf).some((p) => p.includes('named')),
  );
  // The kind changes without a discontinuity.
  const flat = [];
  for (let newest = 9; newest <= 19; newest++) {
    flat.push({ at: newest, media: window(newest, 4, kindAt, () => 0) });
  }
  assert.ok(checkLiveNumbering(flat, kindOf).some((p) => p.includes('no discontinuity')));
});

test('mp4 atoms and webvtt', () => {
  const box = (name, size) => {
    const b = Buffer.alloc(size);
    b.writeUInt32BE(size, 0);
    b.write(name, 4, 'latin1');
    return b;
  };
  const faststart = Buffer.concat([box('ftyp', 24), box('moov', 100), box('mdat', 500)]);
  assert.deepEqual(topLevelBoxes(faststart), ['ftyp', 'moov', 'mdat']);
  const cues = parseVtt(
    'WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nHello\n\n00:00:05.000 --> 00:00:06.000\nBye\n',
  );
  assert.equal(cues.length, 2);
  assert.equal(cues[0].text, 'Hello');
  assert.throws(() => parseVtt('1\n00:00:01,000 --> 00:00:02,000\nx'));
});

test('the media workflow calls files and stages that exist', async () => {
  const { readFileSync, existsSync } = await import('node:fs');
  const { fileURLToPath } = await import('node:url');
  const root = fileURLToPath(new URL('../../', import.meta.url));
  const wf = readFileSync(`${root}.github/workflows/media.yml`, 'utf8');
  for (const m of wf.matchAll(/deploy\/scripts\/[\w./-]+\.mjs/g)) {
    assert.ok(existsSync(`${root}${m[0]}`), `${m[0]} is referenced by media.yml but missing`);
  }
  assert.match(wf, /workflow_dispatch:/);
  assert.match(wf, /schedule:/);
  assert.match(wf, /apt-get install -y --no-install-recommends ffmpeg/);
  const driver = readFileSync(`${root}deploy/scripts/media-smoke.mjs`, 'utf8');
  for (const stage of ['pipeline', 'retranscode', 'download', 'tv', 'resilience']) {
    assert.ok(
      driver.includes(`async function stage${stage[0].toUpperCase()}${stage.slice(1)}`),
      stage,
    );
  }
  // The packages the job builds are the ones it runs.
  for (const bin of ['sr-server', 'sr-cli', 'sr-transcoder']) assert.ok(wf.includes(bin), bin);
});
