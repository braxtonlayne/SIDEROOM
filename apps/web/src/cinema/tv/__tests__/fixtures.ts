// MSW fixtures for the Live TV tests, in the server's wire shapes (spec cinema-live-tv section 8): a four-channel
// guide around a fixed "now" (Tue 10 Mar 2026, 20:15 local).
import { http, HttpResponse, server } from '../../../test/server';
import { toChannel, type WireBlock, type WireChannel } from '../api';
import { MIN, startOfDay } from '../model';
import type { TuneGrant, TvBlock, TvChannel, TvGuide } from '../types';

export const NOW = new Date(2026, 2, 10, 20, 15, 0).getTime();
export const SLOT0 = new Date(2026, 2, 10, 20, 0, 0).getTime();

export const WIRE_CHANNELS: WireChannel[] = [
  {
    id: 'c101',
    number: 101,
    call_sign: '[adult swim]',
    name: 'Adult Swim',
    era_label: 'Adult Swim',
    kind: 'rule_based',
    timezone: 'America/New_York',
    age_restricted: false,
  },
  {
    id: 'c102',
    number: 102,
    call_sign: 'CARTOON NETWORK',
    name: 'Cartoon Network',
    era_label: 'Cartoon Network',
    kind: 'rule_based',
    timezone: 'UTC',
    age_restricted: false,
  },
  {
    id: 'c105',
    number: 105,
    call_sign: 'MOVIES',
    name: 'Movie Channel',
    kind: 'rule_based',
    timezone: 'UTC',
    age_restricted: true,
  },
  {
    id: 'c106',
    number: 106,
    call_sign: 'THE GHIBLI ROOM',
    name: 'Ghibli Room',
    kind: 'manual',
    timezone: 'UTC',
    age_restricted: false,
  },
];
export const CHANNELS: TvChannel[] = WIRE_CHANNELS.map(toChannel);

/** Which channels the UI classifies as anime/series/movies/custom when the server sends no category. */
let seq = 0;
export const block = (
  channel: string,
  startMin: number,
  lenMin: number,
  o: Partial<TvBlock> = {},
): TvBlock => ({
  id: `b${++seq}`,
  channel_id: channel,
  start_ms: SLOT0 + startMin * MIN,
  end_ms: SLOT0 + (startMin + lenMin) * MIN,
  title: 'Programme',
  kind: 'series',
  year: 2000,
  ...o,
});

export function guide(): TvGuide {
  seq = 0;
  return {
    now_ms: NOW,
    channels: CHANNELS,
    footer_note: null,
    blocks: {
      c101: [
        block('c101', -30, 30, { title: 'Earlier show', kind: 'anime', year: 1997 }),
        block('c101', 0, 30, {
          id: 'bebop',
          title: 'Cowboy Bebop',
          kind: 'anime',
          year: 1998,
          title_id: '3',
          episode_label: 'S1 · E4 · Gateway Shuffle',
        }),
        block('c101', 30, 30, {
          id: 'champloo',
          title: 'Samurai Champloo',
          kind: 'anime',
          year: 2004,
        }),
        block('c101', 90, 30, {
          id: 'bebop2',
          title: 'Cowboy Bebop',
          kind: 'anime',
          title_id: '3',
        }),
      ],
      c102: [
        block('c102', 0, 60, {
          id: 'dexter',
          title: "Dexter's Laboratory",
          year: 1996,
          title_id: '7',
        }),
        block('c102', 60, 30, { id: 'batman', title: 'Batman: The Animated Series', year: 1992 }),
      ],
      c105: [
        block('c105', 0, 120, {
          id: 'alien',
          title: 'Alien',
          kind: 'movie',
          year: 1979,
          age_restricted: true,
          title_id: '4',
        }),
      ],
      c106: [
        block('c106', 0, 120, {
          id: 'spirited',
          title: 'Spirited Away',
          kind: 'movie',
          year: 2001,
        }),
      ],
    },
  };
}

/** A UI block as the server sends it (filler becomes a slate). */
export function wireBlock(b: TvBlock): WireBlock {
  const m = b.episode_label?.match(/^S(\d+) · E(\d+)(?: · (.*))?$/);
  return {
    id: b.id,
    channel_id: b.channel_id,
    start_ms: b.start_ms,
    end_ms: b.end_ms,
    kind: b.kind === 'filler' ? 'slate' : 'program',
    title: b.title,
    ...(b.kind !== 'filler' ? { media_kind: b.kind } : {}),
    ...(m
      ? { season_number: Number(m[1]), episode_number: Number(m[2]), episode_title: m[3] ?? null }
      : {}),
    title_id: b.title_id ?? null,
    year: b.year ?? null,
    age_restricted: !!b.age_restricted,
    playable: b.kind !== 'filler',
    source: 'generated',
  };
}

export function wireGuide(g: TvGuide = guide()) {
  return {
    from_ms: startOfDay(NOW),
    to_ms: startOfDay(NOW) + 86_400_000,
    now_ms: g.now_ms,
    channels: WIRE_CHANNELS.filter((c) => g.channels.some((x) => x.id === c.id)).map((c) => ({
      channel: c,
      blocks: (g.blocks[c.id] ?? []).map(wireBlock),
    })),
  };
}

export const CATALOG_TITLE = {
  title: {
    id: '3',
    kind: 'anime',
    title: 'Cowboy Bebop',
    year: 1998,
    runtime_min: 30,
    content_rating: 'TV-14',
    synopsis: 'Spike and the crew hunt bounties across the solar system.',
    genres: ['anime'],
    poster_url: null,
    backdrop_url: '/cdn/bebop-backdrop.webp',
    age_restricted: false,
    has_trailer: false,
    season_count: 1,
  },
  seasons: [],
  variants: [],
};

export const grant = (o: Partial<TuneGrant> = {}): TuneGrant => ({
  channel: CHANNELS[0]!,
  master_url: '/api/v1/tv/channels/c101/live/master.m3u8',
  expires_at_ms: NOW + 24 * 3600_000,
  refresh_at_ms: null,
  now: guide().blocks.c101![1]!,
  next: guide().blocks.c101![2]!,
  server_time_ms: NOW,
  dvr_window_s: null,
  ...o,
});

export interface TvCalls {
  guide: string[];
  settingsPatches: unknown[];
  tunes: string[];
  reminders: string[];
}

/** Handlers for a signed-in member and the guide. `perms` is the admin bitfield (default: not staff). */
export function useTvApi(
  o: {
    guide?: TvGuide;
    guideStatus?: number;
    favourites?: string[];
    perms?: string | null;
    /** Override the tune answer for a channel (return undefined for the default). */
    tune?: (id: string) => Response | undefined;
  } = {},
) {
  const calls: TvCalls = { guide: [], settingsPatches: [], tunes: [], reminders: [] };
  let favourites = o.favourites ?? [];
  const g = () => o.guide ?? guide();
  const wireNow = (id: string) => {
    const list = g().blocks[id] ?? [];
    const cur = list.find((b) => b.start_ms <= NOW && NOW < b.end_ms);
    const next = list.find((b) => b.start_ms > NOW);
    return {
      channel_id: id,
      at_ms: NOW,
      offset_ms: cur ? NOW - cur.start_ms : null,
      current: cur ? wireBlock(cur) : null,
      next: next ? wireBlock(next) : null,
      viewers: 0,
    };
  };
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/users/@me/settings', () =>
      HttpResponse.json({ version: 1, settings: { cinema: { tv: { favourites } } } }),
    ),
    http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
      const body = (await request.json()) as {
        settings: { cinema: { tv: { favourites: string[] } } };
      };
      calls.settingsPatches.push(body);
      favourites = body.settings.cinema.tv.favourites;
      return HttpResponse.json({ version: 2, settings: body.settings });
    }),
    http.get('*/api/v1/admin/me', () =>
      o.perms
        ? HttpResponse.json({ user_id: '1', is_owner: false, platform_permissions: o.perms })
        : HttpResponse.json({ message: 'Forbidden' }, { status: 403 }),
    ),
    http.get('*/api/v1/cinema/titles/:id', ({ params }) =>
      params.id === '3'
        ? HttpResponse.json(CATALOG_TITLE)
        : HttpResponse.json({ message: 'nf' }, { status: 404 }),
    ),
    http.get('*/api/v1/tv/guide', ({ request }) => {
      calls.guide.push(new URL(request.url).search);
      if (o.guideStatus) return new HttpResponse(null, { status: o.guideStatus });
      return HttpResponse.json(wireGuide(g()));
    }),
    http.get('*/api/v1/tv/channels/:id/now', ({ params }) =>
      WIRE_CHANNELS.some((c) => c.id === params.id)
        ? HttpResponse.json(wireNow(String(params.id)))
        : HttpResponse.json({ message: 'nf' }, { status: 404 }),
    ),
    http.get('*/api/v1/tv/channels/:id/tune', ({ params }) => {
      calls.tunes.push(String(params.id));
      const custom = o.tune?.(String(params.id));
      if (custom) return custom;
      const w = wireNow(String(params.id));
      return HttpResponse.json({
        channel_id: params.id,
        state: 'live',
        master_url: `/api/v1/tv/channels/${String(params.id)}/live/master.m3u8`,
        current: w.current,
        offset_ms: w.offset_ms,
        next: w.next,
        viewers: 1,
      });
    }),
    http.get('*/api/v1/tv/channels/:id', ({ params }) => {
      const c = WIRE_CHANNELS.find((x) => x.id === params.id);
      return c ? HttpResponse.json(c) : HttpResponse.json({ message: 'nf' }, { status: 404 });
    }),
    http.put('*/api/v1/tv/reminders/:id', ({ params }) => {
      calls.reminders.push(String(params.id));
      return new HttpResponse(null, { status: 204 });
    }),
  );
  return calls;
}

export { startOfDay };
