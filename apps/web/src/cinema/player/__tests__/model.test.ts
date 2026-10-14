import {
  actionForKey,
  activeMarker,
  cueVars,
  formatBytes,
  formatClock,
  isWatched,
  nextCardStart,
  parseSpriteVtt,
  parseWatchSearch,
  refreshDelayMs,
  resign,
  resumeOffer,
  spokenClock,
  spriteAt,
  stepSpeed,
  watchHref,
} from '../model';

describe('formatting', () => {
  it('formats clocks and spoken times', () => {
    expect(formatClock(75)).toBe('1:15');
    expect(formatClock(3725)).toBe('1:02:05');
    expect(formatClock(-3)).toBe('0:00');
    expect(formatClock(NaN)).toBe('0:00');
    expect(spokenClock(3725)).toBe('1 hour 2 minutes 5 seconds');
    expect(spokenClock(61)).toBe('1 minute 1 second');
    expect(spokenClock(0)).toBe('0 seconds');
  });
  it('formats sizes', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1_500_000)).toBe('1.5 MB');
    expect(formatBytes(2_300_000_000)).toBe('2.3 GB');
  });
});

describe('progress rules', () => {
  it('marks watched at 90 percent', () => {
    expect(isWatched(89, 100)).toBe(false);
    expect(isWatched(90, 100)).toBe(true);
    expect(isWatched(5, 0)).toBe(false);
  });
  it('offers resume only for a real, unfinished position without an explicit start time', () => {
    const p = { position_s: 2530, duration_s: 6000, completed: false };
    expect(resumeOffer(p, undefined)).toBe(2530);
    expect(resumeOffer(p, 120)).toBeNull(); // the link carries ?t=
    expect(resumeOffer(null, undefined)).toBeNull();
    expect(resumeOffer({ ...p, completed: true }, undefined)).toBeNull();
    expect(resumeOffer({ ...p, position_s: 5 }, undefined)).toBeNull();
    expect(resumeOffer({ ...p, position_s: 5500 }, undefined)).toBeNull(); // past 90 percent
    expect(resumeOffer({ ...p, position_s: 5990 }, undefined)).toBeNull();
  });
});

describe('markers and the next-episode card', () => {
  const markers = [
    { kind: 'intro' as const, start_s: 30, end_s: 90 },
    { kind: 'credits' as const, start_s: 5000, end_s: 5400 },
    { kind: 'recap' as const, start_s: 0, end_s: 30 },
  ];
  it('finds the active skippable marker', () => {
    expect(activeMarker(markers, 10)).toBeNull(); // recap is not skippable here
    expect(activeMarker(markers, 45)?.kind).toBe('intro');
    expect(activeMarker(markers, 89.5)).toBeNull(); // about to end anyway
    expect(activeMarker(markers, 5100)?.kind).toBe('credits');
  });
  it('starts the card at the credits marker, else 20 s from the end, never without a next episode', () => {
    const next = { title_id: 't', episode_id: 'e2', season: 1, number: 2, title: 'Two' };
    expect(nextCardStart({ markers, duration_s: 5400, next })).toBe(5000);
    expect(nextCardStart({ markers: [], duration_s: 1500, next })).toBe(1480);
    expect(nextCardStart({ markers, duration_s: 5400, next: null })).toBeNull();
    expect(nextCardStart({ markers: [], duration_s: 10, next })).toBeNull();
  });
});

describe('sprite thumbnails', () => {
  const vtt = `WEBVTT

00:00:00.000 --> 00:00:10.000
sprite-0.jpg#xywh=0,0,160,90

00:00:10.000 --> 00:00:20.000
sprite-0.jpg#xywh=160,0,160,90

00:01:00.000 --> 00:01:10.000
sprite-0.jpg#xywh=0,90,160,90
`;
  it('parses the sprite map and resolves sheet URLs against the map', () => {
    const cues = parseSpriteVtt(vtt, 'https://cdn.test/m1/thumbs/sprites.vtt?sig=z');
    expect(cues).toHaveLength(3);
    expect(cues[1]).toMatchObject({ start: 10, end: 20, x: 160, y: 0, w: 160, h: 90 });
    expect(cues[1]!.url).toBe('https://cdn.test/m1/thumbs/sprite-0.jpg');
    expect(parseSpriteVtt(vtt, '/media/sprites.vtt')[0]!.url).toBe('/media/sprite-0.jpg');
  });
  it('finds the cue for a time', () => {
    const cues = parseSpriteVtt(vtt, '/s.vtt');
    expect(spriteAt(cues, 0)?.x).toBe(0);
    expect(spriteAt(cues, 15)?.x).toBe(160);
    expect(spriteAt(cues, 65)?.y).toBe(90);
    expect(spriteAt([], 5)).toBeNull();
  });
});

describe('keyboard map', () => {
  it('covers space/k, j/l, arrows, f, m, c, digits and speed', () => {
    expect(actionForKey({ key: ' ' })).toEqual({ type: 'toggle' });
    expect(actionForKey({ key: 'k' })).toEqual({ type: 'toggle' });
    expect(actionForKey({ key: 'j' })).toEqual({ type: 'seekBy', seconds: -10 });
    expect(actionForKey({ key: 'l' })).toEqual({ type: 'seekBy', seconds: 10 });
    expect(actionForKey({ key: 'ArrowLeft' })).toEqual({ type: 'seekBy', seconds: -5 });
    expect(actionForKey({ key: 'ArrowUp' })).toEqual({ type: 'volumeBy', delta: 0.05 });
    expect(actionForKey({ key: 'f' })).toEqual({ type: 'fullscreen' });
    expect(actionForKey({ key: 'm' })).toEqual({ type: 'mute' });
    expect(actionForKey({ key: 'c' })).toEqual({ type: 'captions' });
    expect(actionForKey({ key: '4' })).toEqual({ type: 'seekFraction', fraction: 0.4 });
    expect(actionForKey({ key: '<' })).toEqual({ type: 'speed', dir: -1 });
    expect(actionForKey({ key: '>' })).toEqual({ type: 'speed', dir: 1 });
  });
  it('leaves modified keys to the browser', () => {
    expect(actionForKey({ key: 'f', ctrlKey: true })).toBeNull();
    expect(actionForKey({ key: 'ArrowLeft', altKey: true })).toBeNull();
    expect(actionForKey({ key: 'x' })).toBeNull();
  });
  it('steps through the speed ladder and stops at the ends', () => {
    expect(stepSpeed(1, 1)).toBe(1.25);
    expect(stepSpeed(1, -1)).toBe(0.75);
    expect(stepSpeed(2, 1)).toBe(2);
    expect(stepSpeed(0.5, -1)).toBe(0.5);
    expect(stepSpeed(1.1, 1)).toBe(1.5);
  });
});

describe('subtitle style', () => {
  it('maps the style to cue variables', () => {
    expect(cueVars({ size: 'large', color: 'yellow', background: 'none' })).toEqual({
      '--sr-cue-size': '1.35em',
      '--sr-cue-color': '#ffe14d',
      '--sr-cue-bg': 'transparent',
    });
  });
});

describe('signed URL refresh', () => {
  it('refreshes five minutes before expiry, or at half life for short links', () => {
    const now = 1_000_000;
    expect(refreshDelayMs(now + 6 * 3600_000, now)).toBe(6 * 3600_000 - 5 * 60_000);
    expect(refreshDelayMs(now + 8 * 60_000, now)).toBe(4 * 60_000);
    expect(refreshDelayMs(now + 1000, now)).toBe(5000); // never a hot loop
  });
  it('re-signs requests on the same origin only', () => {
    const fresh = 'https://cdn.test/m1/master.m3u8?sig=NEW&exp=9';
    expect(resign('https://cdn.test/m1/720p/seg1.m4s?sig=OLD&exp=1', fresh)).toBe(
      'https://cdn.test/m1/720p/seg1.m4s?sig=NEW&exp=9',
    );
    expect(resign('https://other.test/seg.m4s?sig=OLD', fresh)).toBe(
      'https://other.test/seg.m4s?sig=OLD',
    );
    expect(resign('https://cdn.test/seg.m4s', 'https://cdn.test/m.m3u8')).toBe(
      'https://cdn.test/seg.m4s',
    );
  });
});

describe('route helpers', () => {
  it('parses ep and t from the search string', () => {
    expect(parseWatchSearch({ ep: 'e12', t: 90 })).toEqual({ ep: 'e12', t: 90 });
    expect(parseWatchSearch({ ep: 12, t: '75.9' })).toEqual({ ep: '12', t: 75 });
    expect(parseWatchSearch({ ep: '../x', t: -4 })).toEqual({});
    expect(parseWatchSearch({})).toEqual({});
  });
  it('builds watch links', () => {
    expect(watchHref('t1')).toBe('/cinema/watch/t1');
    expect(watchHref('t1', 'e2', 61.7)).toBe('/cinema/watch/t1?ep=e2&t=61');
  });
});
