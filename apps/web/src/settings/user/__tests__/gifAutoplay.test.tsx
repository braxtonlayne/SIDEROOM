import { act, render, renderHook, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { AttachmentList } from '../../../lounge/msg/Attachments';
import { isGif, useGifAutoplay } from '../gifAutoplay';
import { DEFAULT_PREFS, sanitizePrefs, usePrefsStore } from '../prefs';
import { openSettings, resetPrefs, signedIn } from './helpers';

const setPrefs = (p: Partial<typeof DEFAULT_PREFS>) =>
  act(() => usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, ...p } }));

let focused = true;
beforeEach(() => {
  resetPrefs();
  focused = true;
  vi.spyOn(document, 'hasFocus').mockImplementation(() => focused);
});
afterEach(() => vi.restoreAllMocks());

describe('GIF autoplay (RDY-PAR-006)', () => {
  it('defaults on, validates stored values and detects GIFs', () => {
    expect(DEFAULT_PREFS.gifAutoplay).toBe(true);
    expect(sanitizePrefs({ gifAutoplay: 'no' }).gifAutoplay).toBe(true);
    expect(sanitizePrefs({ gifAutoplay: false }).gifAutoplay).toBe(false);
    expect(isGif('image/gif')).toBe(true);
    expect(isGif('application/octet-stream', 'a/b.GIF?x=1')).toBe(true);
    expect(isGif('image/png', 'a.png')).toBe(false);
  });

  it('plays only while the preference is on, the client is focused and motion is not reduced', () => {
    const { result } = renderHook(() => useGifAutoplay());
    expect(result.current).toBe(true);
    setPrefs({ gifAutoplay: false });
    expect(result.current).toBe(false);
    setPrefs({ gifAutoplay: true, reducedMotion: 'on' });
    expect(result.current).toBe(false);
    setPrefs({ gifAutoplay: true, reducedMotion: 'off' });
    expect(result.current).toBe(true);
    focused = false;
    act(() => {
      window.dispatchEvent(new Event('blur'));
    });
    expect(result.current).toBe(false);
    focused = true;
    act(() => {
      window.dispatchEvent(new Event('focus'));
    });
    expect(result.current).toBe(true);
  });

  it('follows the operating system when reduced motion is left on system', () => {
    const mm = vi.spyOn(window, 'matchMedia').mockImplementation(
      (q: string) =>
        ({
          matches: q.includes('prefers-reduced-motion'),
          media: q,
          addEventListener: () => undefined,
          removeEventListener: () => undefined,
          addListener: () => undefined,
          removeListener: () => undefined,
          dispatchEvent: () => false,
          onchange: null,
        }) as MediaQueryList,
    );
    const { result } = renderHook(() => useGifAutoplay());
    expect(result.current).toBe(false);
    mm.mockRestore();
  });

  it('turns the setting on and off in Accessibility', async () => {
    signedIn();
    const user = userEvent.setup();
    openSettings('accessibility');
    const sw = await screen.findByRole('switch', { name: /Play GIFs when the app is focused/ });
    expect(sw).toBeChecked();
    await user.click(sw);
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(usePrefsStore.getState().prefs.gifAutoplay).toBe(false);
  });

  it('keeps a GIF attachment animated with autoplay and still renders it without', () => {
    const gif = {
      id: '1',
      filename: 'a.gif',
      contentType: 'image/gif',
      url: '/cdn/a.gif',
      size: 10,
      width: 10,
      height: 10,
    } as never;
    const { container } = render(<AttachmentList attachments={[gif]} />);
    expect(container.querySelector('img')).toHaveAttribute('src', '/cdn/a.gif');
    setPrefs({ gifAutoplay: false });
    // jsdom cannot decode a frame, so the animated image is the fallback (as for stickers).
    expect(container.querySelector('img')).toHaveAttribute('src', '/cdn/a.gif');
  });
});
