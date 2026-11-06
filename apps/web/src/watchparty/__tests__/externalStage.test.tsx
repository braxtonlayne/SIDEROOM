// The theatre's stage for external media (PRF-UNF-001): the gate, the frame, and who may talk to the player.
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { resetStore, useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { ExternalStage } from '../ExternalStage';
import { externalHost } from '../externalHost';
import type { PartyMedia } from '../types';

const reapply = vi.fn(async () => undefined);
vi.mock('../runtime', () => ({ partyController: () => ({ reapply }) }));

// D-518: the HLS seam. The test build lists no media host, so the list is pinned here; native HLS is switchable.
const hls = vi.hoisted(() => ({
  native: false,
  dispose: vi.fn(),
  attach: vi.fn(),
}));
vi.mock('../../media/hlsAttach', async (orig) => {
  const real = await orig<typeof import('../../media/hlsAttach')>();
  return {
    ...real,
    nativeHls: () => hls.native,
    canPlayListedHls: (url: string | null | undefined) =>
      real.canPlayListedHls(url, ['tube.example.org', 'cdn.example.net']),
    attachHls: (...args: unknown[]) =>
      (hls.attach(...args) as Promise<() => void> | undefined) ?? Promise.resolve(hls.dispose),
  };
});

const YT: PartyMedia = {
  title_id: '0',
  episode_id: '0',
  variant_id: null,
  asset_id: '0',
  title: 'A song',
  episode_label: null,
  duration_ms: null,
  age_restricted: false,
  live: false,
  kind: 'embed',
  url: 'https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ',
  provider: 'youtube',
};
const FILE: PartyMedia = {
  ...YT,
  title: 'A mix',
  kind: 'direct',
  url: 'https://cdn.example.org/mix.mp3',
  provider: 'direct',
};

beforeAll(() => {
  // jsdom does not implement media playback.
  HTMLMediaElement.prototype.pause = () => undefined;
});

const host = externalHost();
afterEach(() => {
  host.reset();
  resetStore();
  reapply.mockClear();
});

const post = (frame: HTMLIFrameElement, data: unknown, origin: string, from?: Window | null) =>
  act(() => {
    window.dispatchEvent(
      new MessageEvent('message', {
        data,
        origin,
        source: from === undefined ? frame.contentWindow : from,
      }),
    );
  });

describe('the gate', () => {
  it('shows the host and loads nothing until Load', async () => {
    host.setMedia(YT);
    const { container } = render(<ExternalStage />);
    expect(container.querySelector('iframe,video,audio')).toBeNull();
    expect(screen.getByText(/www\.youtube-nocookie\.com/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Load' }));
    const frame = container.querySelector('iframe')!;
    const src = new URL(frame.getAttribute('src')!);
    expect(src.origin).toBe('https://www.youtube-nocookie.com');
    expect(src.searchParams.get('enablejsapi')).toBe('1');
    expect(src.searchParams.get('origin')).toBe(window.location.origin);
  });

  it('has no axe violations at the gate', async () => {
    host.setMedia(FILE);
    const { container } = render(<ExternalStage />);
    expect(await axeViolations(container)).toEqual([]);
  });

  it('a file party fetches nothing until Load either', async () => {
    host.setMedia(FILE);
    const { container } = render(<ExternalStage />);
    expect(container.querySelector('video,audio')).toBeNull();
    expect(screen.getByText(/cdn\.example\.org/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Load' }));
    const el = container.querySelector('video')!;
    expect(el.getAttribute('src')).toBe('https://cdn.example.org/mix.mp3');
    expect(el).not.toHaveAttribute('controls');
    expect(el.getAttribute('controlslist')).toContain('nodownload');
  });

  it('the remembered setting is the standing click', async () => {
    useStore.setState({ userSettings: { version: 1, settings: { load_embeds: 'always' } } });
    host.setMedia(YT);
    const { container } = render(<ExternalStage />);
    await waitFor(() => expect(container.querySelector('iframe')).not.toBeNull());
  });

  it('ticking "always" with Load saves the setting', async () => {
    const patches: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        patches.push(await request.json());
        return HttpResponse.json({ version: 2, settings: { load_embeds: 'always' } });
      }),
    );
    host.setMedia(YT);
    render(<ExternalStage />);
    await userEvent.click(screen.getByRole('checkbox'));
    await userEvent.click(screen.getByRole('button', { name: 'Load' }));
    await waitFor(() => expect(patches).toEqual([{ settings: { load_embeds: 'always' } }]));
  });

  it('refuses an embed address that is not one of the privacy-enhanced hosts', () => {
    host.setMedia({ ...YT, url: 'https://www.youtube.com/embed/dQw4w9WgXcQ' });
    const { container } = render(<ExternalStage />);
    expect(container.querySelector('iframe')).toBeNull();
    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Load' })).toBeNull();
  });

  it('renders nothing for a Cinema title', () => {
    host.setMedia({ ...YT, kind: 'cinema', url: null });
    const { container } = render(<ExternalStage />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe('talking to the frame', () => {
  async function loaded() {
    host.setMedia(YT);
    const r = render(<ExternalStage />);
    await userEvent.click(screen.getByRole('button', { name: 'Load' }));
    const frame = r.container.querySelector('iframe')!;
    await waitFor(() => expect(host.store.getState().attached).toBe(true));
    return frame;
  }

  it('listens to its own frame at the provider origin and drives the controller player from it', async () => {
    const frame = await loaded();
    post(
      frame,
      { event: 'infoDelivery', info: { currentTime: 30, playerState: 1 } },
      'https://www.youtube-nocookie.com',
    );
    expect(host.player.paused()).toBe(false);
    expect(host.player.positionMs()).toBeGreaterThanOrEqual(30_000);
    // The first report brings the controller to the party state.
    expect(reapply).toHaveBeenCalledTimes(1);
  });

  it('ignores messages from another origin or another window', async () => {
    const frame = await loaded();
    const info = { currentTime: 99, playerState: 1 };
    post(frame, { event: 'infoDelivery', info }, 'https://evil.example');
    post(frame, { event: 'infoDelivery', info }, 'https://www.youtube.com');
    post(frame, { event: 'infoDelivery', info }, 'https://www.youtube-nocookie.com', window);
    post(frame, { event: 'infoDelivery', info }, 'https://www.youtube-nocookie.com', null);
    expect(host.player.positionMs()).toBe(0);
    expect(host.player.paused()).toBe(true);
    expect(reapply).not.toHaveBeenCalled();
  });

  it('commands go to the frame with the exact provider origin as the target', async () => {
    const frame = await loaded();
    const spy = vi.spyOn(frame.contentWindow!, 'postMessage');
    host.player.pause();
    host.player.seekMs(5_000);
    const targets = spy.mock.calls.map((c) => c[1]);
    expect(targets.length).toBeGreaterThan(0);
    expect(new Set(targets)).toEqual(new Set(['https://www.youtube-nocookie.com']));
  });

  it('unmounting releases the player', async () => {
    host.setMedia(YT);
    const r = render(<ExternalStage />);
    await userEvent.click(screen.getByRole('button', { name: 'Load' }));
    await waitFor(() => expect(host.store.getState().attached).toBe(true));
    r.unmount();
    expect(host.store.getState().attached).toBe(false);
    expect(await host.player.play()).toBe(false);
  });
});

describe('PeerTube parties and non-Safari HLS (D-516, D-518)', () => {
  const PT_HLS: PartyMedia = {
    ...YT,
    title: 'Night walk',
    kind: 'direct',
    url: 'https://cdn.example.net/hls/a/master.m3u8',
    provider: 'peertube',
    page_url: 'https://tube.example.org/w/9c9de5e8-0a1e',
    format: 'hls',
  };
  afterEach(() => {
    hls.native = false;
    hls.attach.mockClear();
    hls.dispose.mockClear();
  });

  it('attaches the engine after Load and never before, and links the page', async () => {
    host.setMedia(PT_HLS);
    const { container, unmount } = render(<ExternalStage />);
    expect(hls.attach).not.toHaveBeenCalled();
    expect(container.querySelector('video')).toBeNull();
    const link = screen.getByRole('link', { name: 'Open on tube.example.org' });
    expect(link).toHaveAttribute('href', 'https://tube.example.org/w/9c9de5e8-0a1e');
    expect(link).toHaveAttribute('rel', 'noopener noreferrer');
    await userEvent.click(screen.getByRole('button', { name: 'Load' }));
    const el = container.querySelector('video')!;
    await waitFor(() => expect(hls.attach).toHaveBeenCalledTimes(1));
    expect(hls.attach.mock.calls[0]![0]).toBe(el);
    expect(hls.attach.mock.calls[0]![1]).toBe(PT_HLS.url);
    // hls.js feeds the element; it carries no src of its own.
    expect(el.hasAttribute('src')).toBe(false);
    await waitFor(() => expect(host.store.getState().attached).toBe(true));
    unmount();
    expect(hls.dispose).toHaveBeenCalledTimes(1);
  });

  it('never attaches hls.js to an unlisted host', async () => {
    host.setMedia({ ...PT_HLS, url: 'https://objects.elsewhere.example/a/master.m3u8' });
    const { container } = render(<ExternalStage />);
    await userEvent.click(screen.getByRole('button', { name: 'Load' }));
    expect(container.querySelector('video')!.getAttribute('src')).toBe(
      'https://objects.elsewhere.example/a/master.m3u8',
    );
    expect(hls.attach).not.toHaveBeenCalled();
  });

  it('leaves the native path untouched (Safari)', async () => {
    hls.native = true;
    host.setMedia(PT_HLS);
    const { container } = render(<ExternalStage />);
    await userEvent.click(screen.getByRole('button', { name: 'Load' }));
    expect(container.querySelector('video')!.getAttribute('src')).toBe(PT_HLS.url);
    expect(hls.attach).not.toHaveBeenCalled();
  });

  it('a file party is a plain element whatever the provider', async () => {
    host.setMedia({ ...PT_HLS, url: 'https://tube.example.org/a-720.mp4', format: 'file' });
    const { container } = render(<ExternalStage />);
    await userEvent.click(screen.getByRole('button', { name: 'Load' }));
    expect(container.querySelector('video')!.getAttribute('src')).toBe(
      'https://tube.example.org/a-720.mp4',
    );
    expect(hls.attach).not.toHaveBeenCalled();
  });

  it('shows the error line where hls.js cannot run', async () => {
    hls.attach.mockImplementationOnce(() => Promise.reject(new Error('unsupported')));
    host.setMedia(PT_HLS);
    render(<ExternalStage />);
    await userEvent.click(screen.getByRole('button', { name: 'Load' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'This file could not be played in your browser.',
    );
  });
});
