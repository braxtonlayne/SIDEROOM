import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { matchCustomShortcut, matchShortcut } from '../../lounge/shortcuts';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { PlayerSkinCard } from '../PlayerSkinCard';
import { LibrarySkin } from '../skins/LibrarySkin';
import { PillSkin } from '../skins/PillSkin';
import { RetroSkin } from '../skins/RetroSkin';
import {
  __resetPlayerStore,
  engineReport,
  play,
  registerLibraryPanel,
  usePlayerStore,
} from '../store';
import type { PlayerTrack } from '../types';

const tracks: PlayerTrack[] = [
  { id: 'a', title: 'Alpha', artist: 'The Band', src: '/a.mp3', durationSec: 200 },
  { id: 'b', title: 'Bravo', src: '/b.mp3', durationSec: 100 },
  { id: 'c', title: 'Charlie', src: '/c.mp3', durationSec: 100 },
];
const state = () => usePlayerStore.getState();

beforeEach(() => {
  __resetPlayerStore();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  play(tracks);
  engineReport.status('playing');
  engineReport.time(10, 200);
});
afterEach(() => {
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
});

const skins = [
  ['retro', RetroSkin],
  ['pill', PillSkin],
  ['library', LibrarySkin],
] as const;

describe.each(skins)('%s skin controls', (_name, Skin) => {
  it('shows the track and has no axe violations', async () => {
    const { container } = render(<Skin />);
    expect(screen.getByRole('region', { name: 'Music player' })).toBeTruthy();
    expect(screen.getByText('Alpha — The Band')).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it('pauses and plays', async () => {
    const user = userEvent.setup();
    render(<Skin />);
    await user.click(screen.getByRole('button', { name: 'Pause' }));
    expect(state().status).toBe('paused');
    await user.click(screen.getByRole('button', { name: 'Play' }));
    expect(state().status).toBe('playing');
  });

  it('goes to the next and previous track', async () => {
    const user = userEvent.setup();
    render(<Skin />);
    await user.click(screen.getByRole('button', { name: 'Next track' }));
    expect(state().index).toBe(1);
    engineReport.time(0, 100);
    await user.click(screen.getByRole('button', { name: 'Previous track' }));
    expect(state().index).toBe(0);
  });

  it('seeks, sets the volume and mutes', async () => {
    const user = userEvent.setup();
    render(<Skin />);
    const seek = screen.getByRole('slider', { name: 'Seek' });
    expect((seek as HTMLInputElement).value).toBe('10');
    expect((seek as HTMLInputElement).max).toBe('200');
    fireEvent.change(seek, { target: { value: '11' } });
    expect(state().seekRequest).toBe(11);
    const vol = screen.getByRole('slider', { name: 'Volume' });
    fireEvent.change(vol, { target: { value: '30' } });
    expect(state().volume).toBe(0.3);
    await user.click(screen.getByRole('button', { name: 'Mute' }));
    expect(state().muted).toBe(true);
    expect(screen.getByRole('button', { name: 'Unmute' })).toBeTruthy();
  });

  it('moves the dock through the four corners and closes', async () => {
    const user = userEvent.setup();
    render(<Skin />);
    await user.click(screen.getByRole('button', { name: 'Move player to bottom left' }));
    expect(state().dock).toBe('bottom-start');
    await user.click(screen.getByRole('button', { name: 'Move player to top right' }));
    expect(state().dock).toBe('top-end');
    await user.click(screen.getByRole('button', { name: 'Close player' }));
    expect(state().queue).toHaveLength(0);
  });

  it('has a skin button instead of a dropdown, and cycles the skin', async () => {
    const user = userEvent.setup();
    render(<Skin />);
    expect(screen.queryByRole('combobox')).toBeNull();
    const here = state().skin;
    const order = ['retro', 'pill', 'library'] as const;
    const want = order[(order.indexOf(here) + 1) % 3]!;
    const names = {
      retro: 'Retro MP3 Hardware',
      pill: 'Compact Pill / Dock',
      library: 'Full Library Manager',
    };
    await user.click(screen.getByRole('button', { name: `Switch to the ${names[want]} skin` }));
    expect(state().skin).toBe(want);
  });
});

describe('retro skin', () => {
  it('shows the LCD state, the counter and a level bar', () => {
    render(<RetroSkin />);
    expect(screen.getByTestId('player-state').textContent).toBe('Playing');
    expect(screen.getByText('1/3')).toBeTruthy();
    expect(screen.getByTestId('player-vu')).toBeTruthy();
    expect(screen.getByText('0:10 / 3:20')).toBeTruthy();
  });

  it('stop pauses and rewinds; repeat cycles', async () => {
    const user = userEvent.setup();
    render(<RetroSkin />);
    await user.click(screen.getByRole('button', { name: 'Stop' }));
    expect(state().status).toBe('paused');
    expect(state().seekRequest).toBe(0);
    await user.click(screen.getByRole('button', { name: 'Repeat: off' }));
    expect(state().repeat).toBe('all');
    await user.click(screen.getByRole('button', { name: /^Repeat:/ }));
    expect(state().repeat).toBe('one');
  });

  it('scrolls the ticker while playing, and stops under reduced motion', () => {
    const { container, rerender } = render(<RetroSkin />);
    expect(container.querySelector('.sr-ply-ticker')?.getAttribute('data-motion')).toBe('on');
    act(() => usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, reducedMotion: 'on' } }));
    rerender(<RetroSkin />);
    expect(container.querySelector('.sr-ply-ticker')?.getAttribute('data-motion')).toBe('off');
  });

  it('keeps the ticker still while paused', () => {
    engineReport.status('paused');
    const { container } = render(<RetroSkin />);
    expect(container.querySelector('.sr-ply-ticker')?.getAttribute('data-motion')).toBe('off');
  });
});

describe('library skin', () => {
  it('tells the member the library is missing until music-lib registers its panel', () => {
    render(<LibrarySkin />);
    expect(screen.getByText('The library is not available yet.')).toBeTruthy();
  });

  it('renders the registered panel in its slot', () => {
    const off = registerLibraryPanel(() => <p>My library panel</p>);
    render(<LibrarySkin />);
    expect(screen.getByText('My library panel')).toBeTruthy();
    act(() => off());
    expect(screen.queryByText('My library panel')).toBeNull();
  });

  it('lists the queue, jumps to a track and removes one', async () => {
    const user = userEvent.setup();
    const { container } = render(<LibrarySkin />);
    await user.click(screen.getByRole('tab', { name: 'Queue' }));
    const list = screen.getByRole('list', { name: 'Play queue' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(3);
    await user.click(within(list).getByRole('button', { name: /^3.*Charlie/ }));
    expect(state().index).toBe(2);
    await user.click(screen.getByRole('button', { name: 'Remove Alpha from the queue' }));
    expect(state().queue.map((t) => t.id)).toEqual(['b', 'c']);
    expect(await axeViolations(container)).toEqual([]);
  });

  it('moves between tabs with the arrow keys', async () => {
    const user = userEvent.setup();
    render(<LibrarySkin />);
    const lib = screen.getByRole('tab', { name: 'Library' });
    act(() => lib.focus());
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Queue' }).getAttribute('aria-selected')).toBe('true');
  });
});

describe('settings skin picker', () => {
  it('shows the current skin and changes it', async () => {
    const user = userEvent.setup();
    const { container } = render(<PlayerSkinCard />);
    expect(screen.getByText(/An LCD with a scrolling ticker/)).toBeTruthy();
    await user.click(screen.getByRole('combobox', { name: 'Player skin' }));
    await user.click(await screen.findByRole('option', { name: 'Compact Pill / Dock' }));
    expect(state().skin).toBe('pill');
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe('player shortcuts', () => {
  const ev = (key: string, extra: Partial<KeyboardEvent> = {}) => ({
    key,
    ctrlKey: false,
    metaKey: false,
    altKey: true,
    shiftKey: true,
    ...extra,
  });
  it('map Alt+Shift keys to the player', () => {
    expect(matchShortcut(ev('P'))).toBe('playerToggle');
    expect(matchShortcut(ev('M'))).toBe('playerMute');
    expect(matchShortcut(ev('ArrowLeft'))).toBe('playerPrev');
    expect(matchShortcut(ev('ArrowRight'))).toBe('playerNext');
  });
  it('leave the channel and unread keys alone', () => {
    expect(matchShortcut(ev('ArrowUp'))).toBe('prevUnread');
    expect(matchShortcut(ev('ArrowDown', { shiftKey: false }))).toBe('nextChannel');
  });
  it('can be rebound or unbound like the other shortcuts', () => {
    expect(matchCustomShortcut(ev('P'), {})).toBe('playerToggle');
    expect(matchCustomShortcut(ev('P'), { playerToggle: '' })).toBeNull();
    expect(matchCustomShortcut(ev('ArrowRight'), { playerNext: 'Mod+Alt+ArrowRight' })).toBeNull();
    expect(
      matchCustomShortcut(ev('ArrowRight', { ctrlKey: true, shiftKey: false }), {
        playerNext: 'Mod+Alt+ArrowRight',
      }),
    ).toBe('playerNext');
  });
});
