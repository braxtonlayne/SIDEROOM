// "Play here" on link cards (PRF-UNF-001): nothing third-party loads on render, the click creates the
// privacy-enhanced frame or the native player, and the remembered "Load embeds" setting is the standing click.
import { render, screen, waitFor } from '@testing-library/react';
import { vi } from 'vitest';
import userEvent from '@testing-library/user-event';
import type { Embed } from '../data';
import { EmbedList } from '../msg/Embeds';
import { LoadEmbedsCard } from '../msg/EmbedsSettings';
import { usablePlay } from '../msg/EmbedsPlay';
import { resetStore, useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';

// D-518: the HLS seam. The test build lists no media host, so the list is pinned here; native HLS is switchable.
const hls = vi.hoisted(() => ({ native: false, dispose: vi.fn(), attach: vi.fn() }));
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

// D-532: what the card hands the start dialog.
vi.mock('../../watchparty/StartPartyDialog', () => ({
  StartPartyDialog: ({ external }: { external?: unknown }) => (
    <div data-testid="start-party">{JSON.stringify(external)}</div>
  ),
}));

const YT = 'https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ';

const youtube: Embed = {
  type: 'link',
  title: 'A song',
  url: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
  provider: { name: 'YouTube' },
  sr_play: { mode: 'embed', provider: 'youtube', src: YT, sync: true },
};

const mp3: Embed = {
  type: 'audio',
  title: 'A mix',
  url: 'https://cdn.example.org/mix.mp3',
  provider: { name: 'cdn.example.org' },
  sr_play: {
    mode: 'direct',
    provider: 'direct',
    src: 'https://cdn.example.org/mix.mp3',
    media: 'audio',
    sync: true,
  },
};

const mp4: Embed = {
  type: 'video',
  url: 'https://cdn.example.org/clip.mp4',
  provider: { name: 'cdn.example.org' },
  video: { url: 'https://cdn.example.org/clip.mp4' },
  sr_play: {
    mode: 'direct',
    provider: 'direct',
    src: 'https://cdn.example.org/clip.mp4',
    media: 'video',
    sync: true,
  },
};

afterEach(() => resetStore());

const setPref = (v: unknown) =>
  useStore.setState({
    userSettings: { version: 1, settings: v === undefined ? {} : { load_embeds: v } },
  });

describe('nothing third-party loads on render', () => {
  it('a provider card has no iframe and no media element, only a Play here button and a notice', () => {
    const { container } = render(<EmbedList embeds={[youtube]} />);
    expect(container.querySelector('iframe')).toBeNull();
    expect(container.querySelector('video,audio,img')).toBeNull();
    expect(screen.getByRole('button', { name: 'Play here' })).toBeInTheDocument();
    expect(screen.getByText(/only after you press Play/)).toHaveTextContent(
      'www.youtube-nocookie.com',
    );
    // The only link on the card is the title, to the page the author wrote.
    expect(screen.getByRole('link', { name: 'A song' })).toHaveAttribute(
      'href',
      'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
    );
  });

  it('a direct file has no media element until the click', () => {
    const { container } = render(<EmbedList embeds={[mp3, mp4]} />);
    expect(container.querySelector('video,audio,iframe,source')).toBeNull();
    expect(screen.getAllByRole('button', { name: 'Play here' })).toHaveLength(2);
  });

  it('has no axe violations while gated', async () => {
    const { container } = render(<EmbedList embeds={[youtube, mp3]} />);
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe('the click', () => {
  it('creates the privacy-enhanced frame, nothing else', async () => {
    const { container } = render(<EmbedList embeds={[youtube]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Play here' }));
    const frame = container.querySelector('iframe')!;
    const src = new URL(frame.getAttribute('src')!);
    expect(src.origin + src.pathname).toBe(YT);
    expect(src.searchParams.get('autoplay')).toBe('1');
    expect(frame.getAttribute('sandbox')).not.toContain('allow-top-navigation');
    expect(frame.getAttribute('referrerpolicy')).toBe('strict-origin-when-cross-origin');
    expect(screen.queryByRole('button', { name: 'Play here' })).toBeNull();
  });

  it('plays a direct audio file in the native player without a download control', async () => {
    const { container } = render(<EmbedList embeds={[mp3]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Play here' }));
    const audio = container.querySelector('audio')!;
    expect(audio.getAttribute('src')).toBe('https://cdn.example.org/mix.mp3');
    expect(audio).toHaveAttribute('controls');
    expect(audio.getAttribute('controlslist')).toContain('nodownload');
    expect(container.querySelector('a[download]')).toBeNull();
  });

  it('plays a direct video file inline', async () => {
    const { container } = render(<EmbedList embeds={[mp4]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Play here' }));
    const video = container.querySelector('video')!;
    expect(video.getAttribute('src')).toBe('https://cdn.example.org/clip.mp4');
    expect(video.getAttribute('controlslist')).toContain('nodownload');
    // The old click-to-load frame for a page video address is not used for a play offer.
    expect(container.querySelector('iframe')).toBeNull();
  });

  it('offers Watch together only where a party can drive the player', async () => {
    const peertube = (sync: boolean): Embed => ({
      ...youtube,
      sr_play: {
        mode: 'embed',
        provider: 'peertube',
        src: 'https://tube.example.org/videos/embed/9c9de5e8-0a1e',
        sync,
      },
    });
    const { unmount } = render(<EmbedList embeds={[youtube]} />);
    expect(screen.getByRole('button', { name: 'Watch together' })).toBeInTheDocument();
    unmount();
    // D-515: the server resolves a PeerTube video for the party, so the card offers it (sync from the server).
    const pt = render(<EmbedList embeds={[peertube(true)]} />);
    expect(screen.getByRole('button', { name: 'Play here' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Watch together' })).toBeInTheDocument();
    pt.unmount();
    // An older server that says it cannot: no offer.
    render(<EmbedList embeds={[peertube(false)]} />);
    expect(screen.queryByRole('button', { name: 'Watch together' })).toBeNull();
  });
});

describe('the party title (D-532)', () => {
  const peertube = (title?: string): Embed => {
    const rest: Embed = { ...youtube };
    delete rest.title;
    return {
      ...rest,
      ...(title ? { title } : {}),
      sr_play: {
        mode: 'embed',
        provider: 'peertube',
        src: 'https://tube.example.org/videos/embed/9c9de5e8-0a1e',
        sync: true,
      },
    };
  };

  it('passes the title the unfurl carried, and nothing when it only has its generic label', async () => {
    const a = render(<EmbedList embeds={[peertube('My talk')]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Watch together' }));
    expect(JSON.parse((await screen.findByTestId('start-party')).textContent ?? '')).toEqual({
      url: 'https://tube.example.org/videos/embed/9c9de5e8-0a1e',
      label: 'My talk',
    });
    a.unmount();
    render(<EmbedList embeds={[peertube()]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Watch together' }));
    expect(JSON.parse((await screen.findByTestId('start-party')).textContent ?? '')).toEqual({
      url: 'https://tube.example.org/videos/embed/9c9de5e8-0a1e',
    });
  });
});

describe('addresses the client will not load', () => {
  it('drops an offer whose address is not a privacy-enhanced embed or an https file', () => {
    for (const play of [
      { mode: 'embed', provider: 'youtube', src: 'https://www.youtube.com/embed/dQw4w9WgXcQ' },
      { mode: 'embed', provider: 'x', src: 'https://evil.example/page' },
      { mode: 'embed', provider: 'x', src: 'javascript:alert(1)' },
      { mode: 'direct', provider: 'direct', src: 'http://cdn.example.org/a.mp4' },
      { mode: 'direct', provider: 'direct', src: 'https://user:pw@cdn.example.org/a.mp4' },
      { mode: 'weird', provider: 'x', src: YT },
    ] as unknown as NonNullable<Embed['sr_play']>[]) {
      expect(usablePlay(play)).toBeNull();
    }
    expect(usablePlay(undefined)).toBeNull();
    expect(usablePlay(youtube.sr_play)).not.toBeNull();
  });

  it('a forged embed falls back to the ordinary card with no frame', () => {
    const forged: Embed = {
      ...youtube,
      sr_play: { mode: 'embed', provider: 'youtube', src: 'https://evil.example/embed/x' },
    };
    const { container } = render(<EmbedList embeds={[forged]} />);
    expect(container.querySelector('iframe,video,audio')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Play here' })).toBeNull();
  });
});

describe('"Load embeds" (default ask, then remember)', () => {
  it('ask is the default for a missing or unknown value', () => {
    for (const v of [undefined, 'ask', 'sometimes', true]) {
      setPref(v);
      const { container, unmount } = render(<EmbedList embeds={[youtube]} />);
      expect(container.querySelector('iframe')).toBeNull();
      unmount();
    }
  });

  it('always plays the card without a click', () => {
    setPref('always');
    const { container } = render(<EmbedList embeds={[youtube, mp3]} />);
    expect(container.querySelector('iframe')?.getAttribute('src')).toContain(YT);
    expect(container.querySelector('audio')).not.toBeNull();
  });

  it('ticking "always" with the click saves the setting', async () => {
    const patches: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        patches.push(await request.json());
        return HttpResponse.json({ version: 2, settings: { load_embeds: 'always' } });
      }),
    );
    const { container } = render(<EmbedList embeds={[youtube]} />);
    await userEvent.click(screen.getByRole('checkbox', { name: /without asking/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Play here' }));
    expect(container.querySelector('iframe')).not.toBeNull();
    await waitFor(() => expect(patches).toEqual([{ settings: { load_embeds: 'always' } }]));
  });

  it('the settings card switches it back to ask', async () => {
    setPref('always');
    const patches: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        patches.push(await request.json());
        return HttpResponse.json({ version: 3, settings: { load_embeds: 'ask' } });
      }),
    );
    render(<LoadEmbedsCard />);
    const sw = screen.getByRole('switch', { name: /Play embeds without asking/ });
    expect(sw).toBeChecked();
    await userEvent.click(sw);
    await waitFor(() => expect(patches).toEqual([{ settings: { load_embeds: 'ask' } }]));
  });
});

describe('HLS on the card (D-518)', () => {
  const playlist = (src: string): Embed => ({
    type: 'video',
    url: src,
    provider: { name: 'cdn.example.net' },
    sr_play: { mode: 'direct', provider: 'direct', src, media: 'hls', sync: true },
  });
  afterEach(() => {
    hls.native = false;
    hls.attach.mockClear();
    hls.dispose.mockClear();
  });

  it('a listed host plays through hls.js after the click, never before', async () => {
    const src = 'https://cdn.example.net/hls/a/master.m3u8';
    const { container, unmount } = render(<EmbedList embeds={[playlist(src)]} />);
    expect(container.querySelector('video')).toBeNull();
    expect(hls.attach).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Play here' }));
    const video = container.querySelector('video')!;
    await waitFor(() => expect(hls.attach).toHaveBeenCalledTimes(1));
    expect(hls.attach.mock.calls[0]![0]).toBe(video);
    expect(hls.attach.mock.calls[0]![1]).toBe(src);
    expect(video.hasAttribute('src')).toBe(false);
    unmount();
    expect(hls.dispose).toHaveBeenCalledTimes(1);
  });

  it('an unlisted host keeps the sentence and never attaches hls.js', async () => {
    render(<EmbedList embeds={[playlist('https://live.elsewhere.example/a.m3u8')]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Play here' }));
    expect(screen.getByRole('status')).toHaveTextContent(
      'This browser cannot play this live stream',
    );
    expect(hls.attach).not.toHaveBeenCalled();
  });

  it('a browser with native HLS keeps the element src', async () => {
    hls.native = true;
    const src = 'https://cdn.example.net/hls/a/master.m3u8';
    const { container } = render(<EmbedList embeds={[playlist(src)]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Play here' }));
    expect(container.querySelector('video')!.getAttribute('src')).toBe(src);
    expect(hls.attach).not.toHaveBeenCalled();
  });
});
