import { loadSigningMap, playlistRefs } from '../signing';

const MASTER = `#EXTM3U
#EXT-X-VERSION:7
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="English",URI="/cdn/hls/9/a/en.m3u8?ex=2&sig=aa"
#EXT-X-STREAM-INF:BANDWIDTH=1500000,RESOLUTION=854x480
/cdn/hls/9/v/480p/index.m3u8?ex=2&sig=v4
#EXT-X-STREAM-INF:BANDWIDTH=6000000,RESOLUTION=1920x1080
/cdn/hls/9/v/1080p/index.m3u8?ex=2&sig=v1
`;
const VIDEO_480 = `#EXTM3U
#EXT-X-MAP:URI="/cdn/hls/9/v/480p/init.mp4?ex=2&sig=i4"
#EXTINF:6.0,
/cdn/hls/9/v/480p/seg_00001.m4s?ex=2&sig=s4a
#EXTINF:6.0,
/cdn/hls/9/v/480p/seg_00002.m4s?ex=2&sig=s4b
#EXT-X-ENDLIST
`;
const VIDEO_1080 = `#EXTM3U
#EXT-X-MAP:URI="/cdn/hls/9/v/1080p/init.mp4?ex=2&sig=i1"
#EXTINF:6.0,
/cdn/hls/9/v/1080p/seg_00001.m4s?ex=2&sig=s1a
#EXT-X-ENDLIST
`;
const AUDIO = `#EXTM3U
#EXTINF:6.0,
/cdn/hls/9/a/seg_00001.m4s?ex=2&sig=sa
#EXT-X-ENDLIST
`;

const files: Record<string, string> = {
  '/cdn/hls/9/master.m3u8': MASTER,
  '/cdn/hls/9/v/480p/index.m3u8': VIDEO_480,
  '/cdn/hls/9/v/1080p/index.m3u8': VIDEO_1080,
  '/cdn/hls/9/a/en.m3u8': AUDIO,
};
const fetchText = vi.fn((url: string) => {
  const body = files[new URL(url).pathname];
  return body ? Promise.resolve(body) : Promise.reject(new Error(`404 ${url}`));
});

describe('playlistRefs', () => {
  it('finds bare lines and URI attributes, resolved against the playlist, skipping comments and data URIs', () => {
    expect(
      playlistRefs(
        '#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI="data:text/plain;base64,AAAA"\n#EXT-X-MAP:URI="init.mp4?sig=1"\n#EXTINF:6,\nseg1.m4s?sig=2\n',
        'https://cdn.test/hls/9/v/index.m3u8?sig=0',
      ),
    ).toEqual([
      'https://cdn.test/hls/9/v/init.mp4?sig=1',
      'https://cdn.test/hls/9/v/seg1.m4s?sig=2',
    ]);
  });
});

describe('loadSigningMap (CIN-PLY-010)', () => {
  it('maps every file of every rung to its freshly signed URL', async () => {
    fetchText.mockClear();
    const rewrite = await loadSigningMap(
      'https://cdn.test/cdn/hls/9/master.m3u8?ex=3&sig=NEW',
      fetchText,
    );
    // The requests hls.js makes still carry the old signature; the map swaps in the new one by path.
    const stale = 'https://cdn.test/cdn/hls/9/v/1080p/seg_00001.m4s?ex=1&sig=OLD';
    expect(rewrite(stale)).toBe('https://cdn.test/cdn/hls/9/v/1080p/seg_00001.m4s?ex=2&sig=s1a');
    expect(rewrite('https://cdn.test/cdn/hls/9/v/480p/init.mp4?ex=1&sig=OLD')).toBe(
      'https://cdn.test/cdn/hls/9/v/480p/init.mp4?ex=2&sig=i4',
    );
    expect(rewrite('https://cdn.test/cdn/hls/9/a/seg_00001.m4s?sig=OLD')).toBe(
      'https://cdn.test/cdn/hls/9/a/seg_00001.m4s?ex=2&sig=sa',
    );
    expect(rewrite('https://cdn.test/cdn/hls/9/master.m3u8?sig=OLD')).toBe(
      'https://cdn.test/cdn/hls/9/master.m3u8?ex=3&sig=NEW',
    );
    expect(fetchText).toHaveBeenCalledTimes(4);
  });

  it('leaves unknown files and other hosts alone', async () => {
    const rewrite = await loadSigningMap('https://cdn.test/cdn/hls/9/master.m3u8?sig=N', fetchText);
    expect(rewrite('https://cdn.test/cdn/hls/9/other.m4s?sig=old')).toBe(
      'https://cdn.test/cdn/hls/9/other.m4s?sig=old',
    );
    expect(rewrite('https://elsewhere.test/cdn/hls/9/v/480p/seg_00001.m4s?sig=old')).toBe(
      'https://elsewhere.test/cdn/hls/9/v/480p/seg_00001.m4s?sig=old',
    );
  });

  it('fails (so the session retries) when a playlist cannot be read', async () => {
    const broken = vi.fn((url: string) =>
      url.includes('master') ? Promise.resolve(MASTER) : Promise.reject(new Error('offline')),
    );
    // Not `.rejects.toThrow('offline')`: under vitest 2.x that matcher throws "Cannot read properties of
    // undefined (reading 'indexOf')" for any rejection, which looked like a flaky signing test whenever a stale
    // vitest 2 was resolved for apps/web (see src/test/vitestVersion.test.ts). Reading the rejection ourselves
    // works on every version and asserts more.
    const error = await loadSigningMap('https://cdn.test/cdn/hls/9/master.m3u8?sig=N', broken).then(
      () => null,
      (e: unknown) => e,
    );
    expect(error).toBeInstanceOf(Error);
    expect((error as Error).message).toBe('offline');
  });

  it('stops at the playlist limit', async () => {
    fetchText.mockClear();
    await loadSigningMap('https://cdn.test/cdn/hls/9/master.m3u8?sig=N', fetchText, 1);
    expect(fetchText).toHaveBeenCalledTimes(1);
  });
});
