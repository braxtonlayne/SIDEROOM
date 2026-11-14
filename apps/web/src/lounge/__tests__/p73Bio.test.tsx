// SEC-AUD-291 (wave 73, D-775): About Me starts the parser itself. Rendered alone (no Lounge shell), a bio asks
// for the parser on mount, draws nothing of the typed text while it loads (no `**`, no spoiler in clear), draws it
// as markdown when it is ready, and draws the typed text only if the load failed.
import { act, render, screen } from '@testing-library/react';
import { setMarkdownEngine, testFallbackEngine } from '../markdown';
import { ProfileBio } from '../profile/profileMarkdown';

const loadMarkdown = vi.hoisted(() => vi.fn());
vi.mock('@sideroom/wasm', () => ({ loadMarkdown }));

const RAW = '**bold** ||secret|| <@123>';
const api = {
  parse: (t: string) => testFallbackEngine.parse(t),
  isJumbo: () => false,
  expandShortcodes: (t: string) => t,
};

const flush = () =>
  act(async () => {
    for (let i = 0; i < 6; i++) await Promise.resolve();
  });

beforeEach(() => {
  loadMarkdown.mockReset();
  loadMarkdown.mockRejectedValue(new Error('unset'));
  setMarkdownEngine(null);
});
afterEach(() => setMarkdownEngine(null));

describe('ProfileBio starts the parser (D-775)', () => {
  it('draws the typed text when the load fails, and throws nothing', async () => {
    loadMarkdown.mockRejectedValue(new Error('offline'));
    const { container } = render(<ProfileBio text={RAW} />);
    await flush();
    await vi.waitFor(() => expect(loadMarkdown).toHaveBeenCalledTimes(1));
    expect(container.textContent).toContain('**bold**');
    expect(container.textContent).toContain('||secret||');
  });

  it('draws the bio at the first render when the parser is ready, without a load', () => {
    setMarkdownEngine(testFallbackEngine);
    const { container } = render(<ProfileBio text={RAW} />);
    expect(container.querySelector('strong')?.textContent).toBe('bold');
    expect(container.textContent).not.toContain('**');
    expect(loadMarkdown).not.toHaveBeenCalled();
  });

  it('returns nothing for an empty bio', () => {
    setMarkdownEngine(testFallbackEngine);
    const { container } = render(<ProfileBio text="" />);
    expect(container.firstChild).toBeNull();
  });

  it('loads once on mount, shows nothing of the text until ready, then draws markdown', async () => {
    let resolve!: (v: typeof api) => void;
    loadMarkdown.mockReturnValue(new Promise((r) => (resolve = r)));
    const { container } = render(<ProfileBio text={RAW} />);
    await flush();
    await vi.waitFor(() => expect(loadMarkdown).toHaveBeenCalledTimes(1));
    expect(container.textContent).toBe('');
    expect(screen.queryByText(/secret/)).toBeNull();
    await act(async () => {
      resolve(api);
      await Promise.resolve();
    });
    expect(container.querySelector('strong')?.textContent).toBe('bold');
    expect(container.textContent).not.toContain('**');
    expect(container.textContent).not.toContain('||');
    expect(container.textContent).not.toContain('<@123>');
    const spoiler = container.querySelector('.sr-md-spoiler');
    expect(spoiler?.getAttribute('data-revealed')).toBe('false');
    expect(spoiler?.getAttribute('role')).toBe('button');
    await vi.waitFor(() => expect(loadMarkdown).toHaveBeenCalledTimes(1));
  });
});
