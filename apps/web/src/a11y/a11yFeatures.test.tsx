import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useRef } from 'react';
import { useRovingTabindex, nextIndex } from './useRovingTabindex';
import { voiceAnnouncements } from './voiceAnnounce';

function List({ orientation }: { orientation?: 'vertical' | 'horizontal' }) {
  const ref = useRef<HTMLElement>(null);
  useRovingTabindex(ref, { selector: 'button', orientation: orientation ?? 'vertical' });
  return (
    <nav ref={ref} aria-label="list">
      <button>One</button>
      <button>Two</button>
      <button>Three</button>
    </nav>
  );
}

describe('XC-A11Y-001 roving tabindex', () => {
  it('computes the next index', () => {
    expect(nextIndex('ArrowDown', 0, 3, 'vertical')).toBe(1);
    expect(nextIndex('ArrowDown', 2, 3, 'vertical')).toBe(2);
    expect(nextIndex('ArrowRight', 0, 3, 'vertical')).toBeNull();
    expect(nextIndex('ArrowRight', 0, 3, 'horizontal')).toBe(1);
    expect(nextIndex('End', 0, 3, 'both')).toBe(2);
    expect(nextIndex('Home', 2, 3, 'both')).toBe(0);
  });

  it('makes the list one Tab stop and moves with the arrow keys', async () => {
    const user = userEvent.setup();
    render(
      <>
        <button>Before</button>
        <List />
        <button>After</button>
      </>,
    );
    const tabs = () => screen.getAllByRole('button').map((b) => b.tabIndex);
    expect(tabs()).toEqual([0, 0, -1, -1, 0]);
    await user.tab();
    await user.tab();
    expect(screen.getByRole('button', { name: 'One' })).toHaveFocus();
    await user.keyboard('{ArrowDown}');
    expect(screen.getByRole('button', { name: 'Two' })).toHaveFocus();
    expect(tabs()).toEqual([0, -1, 0, -1, 0]);
    await user.keyboard('{End}');
    expect(screen.getByRole('button', { name: 'Three' })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole('button', { name: 'After' })).toHaveFocus();
    await user.tab({ shift: true });
    expect(screen.getByRole('button', { name: 'Three' })).toHaveFocus();
  });
});

describe('XC-A11Y-002 call announcements', () => {
  const base = {
    status: 'idle' as const,
    channelName: 'Lounge' as string | null,
    selfMute: false,
    selfDeaf: false,
    error: null,
  };
  it('announces join, mute, reconnect and drop', () => {
    expect(voiceAnnouncements(base, { ...base, status: 'connected' })).toEqual([
      { text: 'Joined voice channel Lounge', politeness: 'polite' },
    ]);
    const on = { ...base, status: 'connected' as const };
    expect(voiceAnnouncements(on, { ...on, selfMute: true })[0]?.text).toBe('Microphone muted');
    expect(voiceAnnouncements(on, { ...on, status: 'reconnecting' })).toEqual([
      { text: 'Voice connection lost. Reconnecting…', politeness: 'assertive' },
    ]);
    expect(voiceAnnouncements(on, { ...on, status: 'error' })[0]?.politeness).toBe('assertive');
    expect(voiceAnnouncements(on, { ...on, status: 'idle' })[0]?.text).toBe(
      'Left the voice channel',
    );
    expect(voiceAnnouncements(base, base)).toEqual([]);
  });
});
