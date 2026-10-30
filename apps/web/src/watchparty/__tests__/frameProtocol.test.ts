import {
  FRAME_ORIGINS,
  cardEmbedSrc,
  commandMessage,
  isCardEmbedSrc,
  listenMessages,
  parseMessage,
  providerOfSrc,
  syncedFrameSrc,
} from '../frameProtocol';

const YT = 'https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ';
const VM = 'https://player.vimeo.com/video/76979871?dnt=1';

describe('embed addresses', () => {
  it('accepts only the two privacy-enhanced embeds', () => {
    expect(providerOfSrc(YT)).toBe('youtube');
    expect(providerOfSrc(`${YT}?start=5`)).toBe('youtube');
    expect(providerOfSrc(VM)).toBe('vimeo');
    for (const bad of [
      'https://www.youtube.com/embed/dQw4w9WgXcQ',
      'https://youtube.com/watch?v=dQw4w9WgXcQ',
      'http://www.youtube-nocookie.com/embed/dQw4w9WgXcQ',
      'https://www.youtube-nocookie.com.evil.example/embed/dQw4w9WgXcQ',
      'https://www.youtube-nocookie.com:8443/embed/dQw4w9WgXcQ',
      'https://user@www.youtube-nocookie.com/embed/dQw4w9WgXcQ',
      'https://www.youtube-nocookie.com/embed/short',
      'https://www.youtube-nocookie.com/watch?v=dQw4w9WgXcQ',
      'https://vimeo.com/76979871',
      'https://player.vimeo.com/video/abc',
      'javascript:alert(1)',
      '',
      null,
      undefined,
    ]) {
      expect(providerOfSrc(bad), String(bad)).toBeNull();
    }
  });

  it('a card may also show a PeerTube-style embed path, but no other page', () => {
    expect(isCardEmbedSrc(YT)).toBe(true);
    expect(isCardEmbedSrc('https://tube.example.org/videos/embed/9c9de5e8-0a1e-484a')).toBe(true);
    expect(isCardEmbedSrc('https://tube.example.org/w/9c9de5e8-0a1e-484a')).toBe(false);
    expect(isCardEmbedSrc('https://tube.example.org/videos/embed/9c9de5e8-0a1e?script=1')).toBe(
      false,
    );
    expect(isCardEmbedSrc('http://tube.example.org/videos/embed/9c9de5e8-0a1e')).toBe(false);
  });

  it('a card embed autoplays after the click, nothing else is added to Vimeo', () => {
    const y = new URL(cardEmbedSrc(YT));
    expect(y.origin + y.pathname).toBe(YT);
    expect(y.searchParams.get('autoplay')).toBe('1');
    const v = new URL(cardEmbedSrc(VM));
    expect(v.searchParams.get('dnt')).toBe('1');
    expect(v.searchParams.get('autoplay')).toBe('1');
  });

  it('a synced frame switches on the message API and hides the provider controls', () => {
    const y = new URL(syncedFrameSrc('youtube', YT, 'https://chat.example.org'));
    expect(y.origin).toBe(FRAME_ORIGINS.youtube);
    expect(y.searchParams.get('enablejsapi')).toBe('1');
    expect(y.searchParams.get('origin')).toBe('https://chat.example.org');
    expect(y.searchParams.get('controls')).toBe('0');
    const v = new URL(syncedFrameSrc('vimeo', VM, 'https://chat.example.org'));
    expect(v.searchParams.get('api')).toBe('1');
    expect(v.searchParams.get('dnt')).toBe('1');
  });
});

describe('commands', () => {
  it('YouTube takes its documented command envelope', () => {
    expect(JSON.parse(commandMessage('youtube', { cmd: 'play' }))).toEqual({
      event: 'command',
      func: 'playVideo',
      args: [],
    });
    expect(JSON.parse(commandMessage('youtube', { cmd: 'pause' })).func).toBe('pauseVideo');
    expect(JSON.parse(commandMessage('youtube', { cmd: 'seek', seconds: 12.5 }))).toMatchObject({
      func: 'seekTo',
      args: [12.5, true],
    });
    expect(JSON.parse(commandMessage('youtube', { cmd: 'rate', rate: 1.25 })).func).toBe(
      'setPlaybackRate',
    );
    expect(JSON.parse(listenMessages('youtube', 'load')[0]!)).toMatchObject({ event: 'listening' });
    expect(listenMessages('youtube', 'ready')).toEqual([]);
  });

  it('Vimeo takes method messages and subscribes to events after ready', () => {
    expect(JSON.parse(commandMessage('vimeo', { cmd: 'play' }))).toEqual({ method: 'play' });
    expect(JSON.parse(commandMessage('vimeo', { cmd: 'seek', seconds: 3 }))).toEqual({
      method: 'setCurrentTime',
      value: 3,
    });
    expect(JSON.parse(commandMessage('vimeo', { cmd: 'rate', rate: 1.05 }))).toEqual({
      method: 'setPlaybackRate',
      value: 1.05,
    });
    expect(listenMessages('vimeo', 'load')).toEqual([]);
    const subs = listenMessages('vimeo', 'ready').map((m) => JSON.parse(m).value);
    expect(subs).toEqual(expect.arrayContaining(['play', 'pause', 'timeupdate', 'seeked']));
  });
});

describe('incoming messages', () => {
  it('YouTube: ready, state, time and rate in one infoDelivery', () => {
    expect(parseMessage('youtube', JSON.stringify({ event: 'onReady' }))).toEqual([
      { type: 'ready' },
    ]);
    expect(
      parseMessage('youtube', {
        event: 'infoDelivery',
        info: { currentTime: 4.5, duration: 200, playerState: 1, playbackRate: 1.25 },
      }),
    ).toEqual([
      { type: 'time', seconds: 4.5, duration: 200 },
      { type: 'state', state: 'playing' },
      { type: 'rate', rate: 1.25 },
    ]);
    expect(parseMessage('youtube', { event: 'onStateChange', info: 3 })).toEqual([
      { type: 'state', state: 'buffering' },
    ]);
    expect(parseMessage('youtube', { event: 'onStateChange', info: 0 })).toEqual([
      { type: 'state', state: 'ended' },
    ]);
  });

  it('Vimeo: events carry seconds, a buffer end only resumes', () => {
    expect(parseMessage('vimeo', JSON.stringify({ event: 'ready' }))).toEqual([{ type: 'ready' }]);
    expect(
      parseMessage('vimeo', { event: 'play', data: { seconds: 2, duration: 90, percent: 0.02 } }),
    ).toEqual([
      { type: 'state', state: 'playing' },
      { type: 'time', seconds: 2, duration: 90 },
    ]);
    expect(parseMessage('vimeo', { event: 'timeupdate', data: { seconds: 3 } })).toEqual([
      { type: 'time', seconds: 3 },
    ]);
    expect(parseMessage('vimeo', { event: 'bufferend', data: { seconds: 3 } })).toEqual([
      { type: 'state', state: 'resumed' },
      { type: 'time', seconds: 3 },
    ]);
    expect(
      parseMessage('vimeo', { event: 'playbackratechange', data: { playbackRate: 1.5 } }),
    ).toEqual([{ type: 'rate', rate: 1.5 }]);
  });

  it('ignores junk', () => {
    for (const p of ['youtube', 'vimeo'] as const) {
      for (const junk of ['', 'nope', '[]', 'null', 42, null, undefined, { event: 'weird' }]) {
        expect(parseMessage(p, junk), String(junk)).toEqual([]);
      }
    }
    expect(parseMessage('youtube', { event: 'infoDelivery', info: { currentTime: 'x' } })).toEqual(
      [],
    );
  });
});
