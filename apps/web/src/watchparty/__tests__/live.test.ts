// A Live TV party (G-16): the party's media names the TV channel, and every participant tunes it themselves.
import { describe, expect, it, vi } from 'vitest';
import { http, HttpResponse, server } from '../../test/server';
import { loadPartyMedia } from '../runtime';
import type { PartyMedia } from '../types';

const media: PartyMedia = {
  title_id: '3',
  episode_id: '0',
  variant_id: null,
  asset_id: '3',
  title: '101 Toonami',
  episode_label: 'Cowboy Bebop',
  duration_ms: null,
  age_restricted: false,
  live: true,
};

function fakeSession(matching = false) {
  return {
    matches: vi.fn(() => matching),
    start: vi.fn(async () => undefined),
    setMode: vi.fn(),
    state: { status: matching ? 'playing' : 'idle' },
  };
}

function tuneApi(tunes: string[] = []) {
  server.use(
    http.get('*/api/v1/tv/channels/:id/tune', ({ params }) => {
      tunes.push(String(params.id));
      return HttpResponse.json({
        channel_id: params.id,
        state: 'live',
        master_url: `/api/v1/tv/channels/${String(params.id)}/live/master.m3u8`,
        current: null,
        next: null,
        offset_ms: 0,
        viewers: 1,
      });
    }),
    http.get('*/api/v1/tv/channels/:id', ({ params }) =>
      HttpResponse.json({
        id: params.id,
        number: 101,
        name: 'Toonami',
        call_sign: 'TOON',
        kind: 'rule_based',
        enabled: true,
        age_restricted: false,
      }),
    ),
  );
  return tunes;
}

describe('loadPartyMedia for a live party', () => {
  it('tunes the channel in the player session and follows the live edge (no start offset)', async () => {
    const tunes = tuneApi();
    const session = fakeSession();
    await loadPartyMedia(session as never, media, 12_345, true);
    expect(tunes).toEqual(['3']);
    expect(session.matches).toHaveBeenCalledWith('tv:3', null);
    const [info, opts] = session.start.mock.calls[0] as unknown as [
      { master_url: string; live: { channel_id: string } },
      { startAt?: number; autoplay: boolean },
    ];
    expect(info.master_url).toBe('/api/v1/tv/channels/3/live/master.m3u8');
    expect(info.live.channel_id).toBe('3');
    expect(opts.startAt).toBeUndefined();
    expect(opts.autoplay).toBe(true);
    expect(session.setMode).toHaveBeenCalledWith('full');
  });

  it('keeps the channel that is already open', async () => {
    const tunes = tuneApi();
    const session = fakeSession(true);
    await loadPartyMedia(session as never, media, 0, true);
    expect(tunes).toEqual([]);
    expect(session.start).not.toHaveBeenCalled();
    expect(session.setMode).toHaveBeenCalledWith('full');
  });
});
