// "Play here" on link cards (PRF-UNF-001): nothing third-party loads on render, the click creates the
// privacy-enhanced frame or the native player, and the remembered "Load embeds" setting is the standing click.
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Embed } from '../data';
import { EmbedList } from '../msg/Embeds';
import { LoadEmbedsCard } from '../msg/EmbedsSettings';
import { usablePlay } from '../msg/EmbedsPlay';
import { resetStore, useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';

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
    const peertube: Embed = {
      ...youtube,
      sr_play: {
        mode: 'embed',
        provider: 'peertube',
        src: 'https://tube.example.org/videos/embed/9c9de5e8-0a1e',
        sync: false,
      },
    };
    const { unmount } = render(<EmbedList embeds={[youtube]} />);
    expect(screen.getByRole('button', { name: 'Watch together' })).toBeInTheDocument();
    unmount();
    render(<EmbedList embeds={[peertube]} />);
    expect(screen.getByRole('button', { name: 'Play here' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Watch together' })).toBeNull();
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
