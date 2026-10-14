import { ApiError } from '../../../api/client';
import { isAgeGateError } from '../../../lib/ageGate';
import { http, HttpResponse, server } from '../../../test/server';
import { getPlayback, refreshSource } from '../api';
import { setProgressApi } from '../progress';

afterEach(() => setProgressApi(null));

const target = (o: Record<string, unknown> = {}) => ({
  title_id: '7',
  episode_id: null,
  asset_id: '70',
  title: 'Paprika',
  runtime_min: 90,
  age_restricted: false,
  markers: [],
  ...o,
});
const asset = (o: Record<string, unknown> = {}) => ({
  master_url: 'https://cdn.test/cdn/hls/70/master.m3u8?ex=1&sig=x',
  expires_at: 1_800_000_000,
  refresh_at: 1_799_999_400,
  duration_secs: 5400,
  sprites_url: 'https://cdn.test/cdn/hls/70/sprites.vtt',
  subtitles: [
    {
      id: 's1',
      language: 'en',
      label: 'English',
      vtt_url: 'https://cdn.test/cdn/hls/70/subs/en.vtt',
      ass_url: 'https://cdn.test/cdn/hls/70/subs/en.ass',
      is_default: true,
    },
  ],
  ...o,
});

describe('getPlayback', () => {
  it('composes a movie from the play target, the ingest grant and the saved position', async () => {
    const seen: string[] = [];
    server.use(
      http.get('*/api/v1/cinema/titles/7/play', ({ request }) => {
        seen.push(new URL(request.url).search);
        return HttpResponse.json(
          target({
            markers: [{ kind: 'intro', start_ms: 30_000, end_ms: 90_000 }],
            variant_id: '71',
          }),
        );
      }),
      http.get('*/api/v1/cinema/assets/70/playback', () => HttpResponse.json(asset())),
      http.get('*/api/v1/cinema/titles/7/progress', () =>
        HttpResponse.json({
          title_id: '7',
          episode_id: '0',
          position_secs: 1200,
          duration_secs: 5400,
          percent: 22,
          completed: false,
          updated_at_ms: 9,
        }),
      ),
    );
    const info = await getPlayback({ titleId: '7', variantId: '71' });
    expect(seen).toEqual(['?variant=71']);
    expect(info).toMatchObject({
      title_id: '7',
      episode_id: null,
      media_id: '70',
      title: 'Paprika',
      kind: 'movie',
      duration_s: 5400,
      master_url: 'https://cdn.test/cdn/hls/70/master.m3u8?ex=1&sig=x',
      expires_at_ms: 1_800_000_000_000, // seconds on the wire, milliseconds in the player
      refresh_at_ms: 1_799_999_400_000,
      sprites_url: 'https://cdn.test/cdn/hls/70/sprites.vtt',
      markers: [{ kind: 'intro', start_s: 30, end_s: 90 }],
      next: null,
      progress: { position_s: 1200, duration_s: 5400, completed: false },
    });
    expect(info.subtitle_tracks[0]).toMatchObject({
      id: 's1',
      label: 'English',
      vtt_url: 'https://cdn.test/cdn/hls/70/subs/en.vtt',
      ass_url: 'https://cdn.test/cdn/hls/70/subs/en.ass',
    });
  });

  it('labels an episode and fills the up-next card from the next episode', async () => {
    server.use(
      http.get('*/api/v1/cinema/episodes/e5/play', () =>
        HttpResponse.json(
          target({
            title_id: '8',
            episode_id: 'e5',
            asset_id: '80',
            title: 'Frieren',
            episode_title: 'The Journey',
            season_number: 1,
            episode_number: 5,
            next_episode_id: 'e6',
          }),
        ),
      ),
      http.get('*/api/v1/cinema/episodes/e6', () =>
        HttpResponse.json({
          id: 'e6',
          title_id: '8',
          season_number: 1,
          number: 6,
          title: 'Next Stop',
          still_url: '/cdn/still.jpg',
        }),
      ),
      http.get('*/api/v1/cinema/assets/80/playback', () => HttpResponse.json(asset())),
      http.get('*/api/v1/cinema/titles/8/episodes/e5/progress', () =>
        HttpResponse.json({}, { status: 404 }),
      ),
    );
    const info = await getPlayback({ titleId: '8', episodeId: 'e5' });
    expect(info).toMatchObject({
      episode_id: 'e5',
      kind: 'series',
      episode_label: 'S1 · E5 · The Journey',
      progress: null,
      next: { title_id: '8', episode_id: 'e6', season: 1, number: 6, title: 'Next Stop' },
    });
  });

  it('plays anyway when the next episode or the saved position cannot be read', async () => {
    server.use(
      http.get('*/api/v1/cinema/episodes/e5/play', () =>
        HttpResponse.json(target({ title_id: '8', episode_id: 'e5', next_episode_id: 'e6' })),
      ),
      http.get('*/api/v1/cinema/episodes/e6', () => HttpResponse.json({}, { status: 500 })),
      http.get('*/api/v1/cinema/assets/70/playback', () => HttpResponse.json(asset())),
      http.get('*/api/v1/cinema/titles/8/episodes/e5/progress', () =>
        HttpResponse.json({}, { status: 500 }),
      ),
    );
    const info = await getPlayback({ titleId: '8', episodeId: 'e5' });
    expect(info.next).toBeNull();
    expect(info.progress).toBeNull();
  });

  it('lets the age gate error through so the page can ask', async () => {
    server.use(
      http.get('*/api/v1/cinema/titles/7/play', () =>
        HttpResponse.json({ code: 900011, message: 'Age confirmation required' }, { status: 403 }),
      ),
    );
    const err = await getPlayback({ titleId: '7' }).catch((e: unknown) => e);
    expect(isAgeGateError(err)).toBe(true);
  });

  it('plays the first playable trailer without progress or next episode', async () => {
    server.use(
      http.get('*/api/v1/cinema/titles/7', () =>
        HttpResponse.json({
          title: { title: 'Paprika', age_restricted: false, runtime_min: 90 },
          trailers: [
            { id: 't0', playable: false, asset_id: '90' },
            { id: 't1', playable: true, asset_id: '91' },
          ],
        }),
      ),
      http.get('*/api/v1/cinema/assets/91/playback', () =>
        HttpResponse.json(asset({ duration_secs: 120 })),
      ),
    );
    const info = await getPlayback({ titleId: '7', trailer: true });
    expect(info).toMatchObject({
      media_id: '91',
      trailer: true,
      progress: null,
      next: null,
      duration_s: 120,
    });
  });

  it('asks the age question for a restricted trailer whose asset is withheld', async () => {
    server.use(
      http.get('*/api/v1/cinema/titles/7', () =>
        HttpResponse.json({
          title: { title: 'Dark', age_restricted: true },
          trailers: [{ id: 't1', playable: true }],
        }),
      ),
    );
    const err = await getPlayback({ titleId: '7', trailer: true }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(isAgeGateError(err)).toBe(true);
  });
});

describe('refreshSource', () => {
  it('turns the grant into milliseconds', async () => {
    server.use(
      http.post('*/api/v1/cinema/assets/70/refresh', () =>
        HttpResponse.json({
          master_url: 'https://cdn.test/m?sig=2',
          expires_at: 2000,
          refresh_at: 1400,
        }),
      ),
    );
    expect(await refreshSource('70')).toEqual({
      master_url: 'https://cdn.test/m?sig=2',
      expires_at_ms: 2_000_000,
      refresh_at_ms: 1_400_000,
    });
  });
});
