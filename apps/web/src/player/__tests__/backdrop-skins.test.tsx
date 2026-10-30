/** MUS-SKN-001: the backdrop skins. Each renders, its live controls work, its artwork is hidden and still. */
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { EQ_PRESETS } from '../../music/eq/presets';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { PlayerSkinCard } from '../PlayerSkinCard';
import { isPlayerLibraryOpen, closePlayerLibrary } from '../presence';
import { CircuitSkin } from '../skins/backdrop/CircuitSkin';
import {
  __resetSkinExtras,
  createPomodoro,
  FOCUS_MS,
  registerSkinExtras,
} from '../skins/backdrop/extras';
import { readBands, SPECTRUM_BANDS } from '../skins/backdrop/frame';
import { __resetStackWindows, StackSkin } from '../skins/backdrop/StackSkin';
import { StudySkin } from '../skins/backdrop/StudySkin';
import { TidepoolSkin } from '../skins/backdrop/TidepoolSkin';
import { ValveSkin } from '../skins/backdrop/ValveSkin';
import {
  __resetPlayerStore,
  enqueue,
  engineReport,
  play,
  removeAt,
  setShuffle,
  usePlayerStore,
} from '../store';
import type { PlayerTrack } from '../types';

const tracks: PlayerTrack[] = [
  { id: 'a', title: 'Alpha', artist: 'The Band', src: '/a.mp3', durationSec: 200, format: 'mp3' },
  { id: 'b', title: 'Bravo', src: '/b.mp3', durationSec: 100 },
  { id: 'c', title: 'Charlie', src: '/c.mp3', durationSec: 100 },
];
const state = () => usePlayerStore.getState();

beforeEach(() => {
  __resetPlayerStore();
  __resetSkinExtras();
  __resetStackWindows();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  play(tracks);
  engineReport.status('playing');
  engineReport.time(10, 200);
});
afterEach(() => {
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  closePlayerLibrary();
});

const skins = [
  ['valve', ValveSkin],
  ['tidepool', TidepoolSkin],
  ['circuit', CircuitSkin],
  ['study', StudySkin],
  ['stack', StackSkin],
] as const;

describe.each(skins)('%s skin', (name, Skin) => {
  it('renders the track in a labelled region with no axe violations', async () => {
    const { container } = render(<Skin />);
    const region = screen.getByRole('region', { name: 'Music player' });
    expect(region.getAttribute('data-skin')).toBe(name);
    expect(region.getAttribute('data-kind')).toBe('backdrop');
    expect(screen.getAllByText('Alpha — The Band').length).toBeGreaterThan(0);
    expect(await axeViolations(container)).toEqual([]);
  });

  it('pauses, plays, skips and goes back', async () => {
    const user = userEvent.setup();
    render(<Skin />);
    await user.click(screen.getByRole('button', { name: 'Pause' }));
    expect(state().status).toBe('paused');
    await user.click(screen.getByRole('button', { name: 'Play' }));
    expect(state().status).toBe('playing');
    await user.click(screen.getByRole('button', { name: 'Next track' }));
    expect(state().index).toBe(1);
    engineReport.time(0, 100);
    await user.click(screen.getByRole('button', { name: 'Previous track' }));
    expect(state().index).toBe(0);
  });

  it('stops, seeks, sets the volume and mutes', async () => {
    const user = userEvent.setup();
    render(<Skin />);
    fireEvent.change(screen.getByRole('slider', { name: 'Seek' }), { target: { value: '42' } });
    expect(state().seekRequest).toBe(42);
    await user.click(screen.getByRole('button', { name: 'Stop' }));
    expect(state().status).toBe('paused');
    expect(state().seekRequest).toBe(0);
    fireEvent.change(screen.getByRole('slider', { name: 'Volume' }), { target: { value: '25' } });
    expect(state().volume).toBe(0.25);
    await user.click(screen.getByRole('button', { name: 'Mute' }));
    expect(state().muted).toBe(true);
  });

  it('cycles repeat and shuffle', async () => {
    const user = userEvent.setup();
    render(<Skin />);
    await user.click(screen.getByRole('button', { name: 'Repeat: off' }));
    expect(state().repeat).toBe('all');
    const shuffle = screen.getByRole('button', { name: 'Shuffle' });
    await user.click(shuffle);
    expect(state().shuffle).toBe(true);
    expect(shuffle.getAttribute('aria-pressed')).toBe('true');
  });

  it('has the skin, dock and close buttons', async () => {
    const user = userEvent.setup();
    render(<Skin />);
    await user.click(screen.getByRole('button', { name: /^Switch to the .* skin$/ }));
    expect(state().skin).toBe('pill');
    await user.click(screen.getByRole('button', { name: 'Move player to bottom left' }));
    expect(state().dock).toBe('bottom-start');
    await user.click(screen.getByRole('button', { name: 'Close player' }));
    expect(state().queue).toHaveLength(0);
  });

  it('hides its artwork from assistive tech and keeps it out of the tab order', () => {
    const { container } = render(<Skin />);
    const art = container.querySelectorAll('.sr-ply-bd-art');
    expect(art.length).toBeGreaterThan(0);
    for (const el of Array.from(art)) {
      expect(el.getAttribute('aria-hidden')).toBe('true');
      expect(el.querySelector('button, input, select, textarea, a[href], [tabindex]')).toBeNull();
    }
  });

  it('animates only while playing, and never under reduced motion', () => {
    const { rerender } = render(<Skin />);
    const region = () => screen.getByRole('region', { name: 'Music player' });
    expect(region().getAttribute('data-motion')).toBe('on');
    act(() => engineReport.status('paused'));
    expect(region().getAttribute('data-motion')).toBe('off');
    act(() => engineReport.status('playing'));
    act(() => usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, reducedMotion: 'on' } }));
    rerender(<Skin />);
    expect(region().getAttribute('data-motion')).toBe('off');
  });
});

describe('valve skin', () => {
  it('has two VU meters as artwork and a tone switch that turns the equalizer off and on', async () => {
    const user = userEvent.setup();
    const { container } = render(<ValveSkin />);
    expect(container.querySelectorAll('.v-vu[aria-hidden="true"]')).toHaveLength(2);
    const tone = screen.getByRole('button', { name: 'Tone: equalizer on' });
    expect(tone.getAttribute('aria-pressed')).toBe('true');
    await user.click(tone);
    expect(state().eqEnabled).toBe(false);
  });

  it('turns equalizer bands and the master gain with its brass knobs', () => {
    render(<ValveSkin />);
    const knobs = screen.getByRole('group', { name: 'Tone controls' });
    const band = within(knobs).getByRole('slider', { name: '1KHz band' });
    fireEvent.change(band, { target: { value: '5' } });
    expect(state().eq[5]).toBe(5);
    expect(state().eq.filter((g) => g !== 0)).toHaveLength(1);
    expect(band.getAttribute('aria-valuetext')).toBe('+5 dB');
    fireEvent.change(within(knobs).getByRole('slider', { name: 'Volume' }), {
      target: { value: '40' },
    });
    expect(state().volume).toBe(0.4);
  });

  it('marks A, then B, loops back to A, and stops looping', async () => {
    const user = userEvent.setup();
    render(<ValveSkin />);
    act(() => engineReport.time(20, 200));
    await user.click(screen.getByRole('button', { name: 'A-B loop: mark A here' }));
    act(() => engineReport.time(30, 200));
    const loop = screen.getByRole('button', { name: 'A-B loop: mark B here and loop' });
    await user.click(loop);
    expect(loop.getAttribute('aria-pressed')).toBe('true');
    act(() => engineReport.time(31, 200));
    expect(state().seekRequest).toBe(20);
    await user.click(screen.getByRole('button', { name: 'A-B loop: stop looping' }));
    expect(screen.getByRole('button', { name: 'A-B loop: mark A here' })).toBeTruthy();
  });
});

describe('tidepool skin', () => {
  it('plays a track from the queue wing and opens the library', async () => {
    const user = userEvent.setup();
    render(<TidepoolSkin />);
    const list = screen.getByRole('list', { name: 'Play queue' });
    await user.click(within(list).getByRole('button', { name: /Charlie/ }));
    expect(state().index).toBe(2);
    await user.click(screen.getByRole('button', { name: 'Open the music library' }));
    expect(isPlayerLibraryOpen()).toBe(true);
  });
});

describe('circuit skin', () => {
  it('shows a spectrum and a level meter, and its cue pads jump through the track', async () => {
    const user = userEvent.setup();
    render(<CircuitSkin />);
    expect(screen.getByTestId('skin-spectrum').getAttribute('aria-hidden')).toBe('true');
    expect(screen.getByTestId('skin-meter').getAttribute('aria-hidden')).toBe('true');
    await user.click(screen.getByRole('button', { name: 'Cue 3: jump to 50% of the track' }));
    expect(state().seekRequest).toBe(100);
    await user.click(screen.getByRole('button', { name: 'Cue 1: jump to 0% of the track' }));
    expect(state().seekRequest).toBe(0);
  });

  it('seeks on the waveform, and its fader, knobs and flags drive volume, EQ, crossfade, repeat and shuffle', async () => {
    const user = userEvent.setup();
    render(<CircuitSkin />);
    fireEvent.change(screen.getByRole('slider', { name: 'Seek' }), { target: { value: '50' } });
    expect(state().seekRequest).toBe(50);
    fireEvent.change(screen.getByRole('slider', { name: 'Volume' }), { target: { value: '40' } });
    expect(state().volume).toBeCloseTo(0.4);
    usePlayerStore.setState({ eqEnabled: false });
    fireEvent.change(screen.getByRole('slider', { name: /^Sub bass/ }), { target: { value: '6' } });
    expect(state().eq.slice(0, 3)).toEqual([6, 6, 6]);
    expect(state().eq[3]).toBe(0);
    expect(state().eqEnabled).toBe(true);
    fireEvent.change(screen.getByRole('slider', { name: /^Air treble/ }), {
      target: { value: '-4' },
    });
    expect(state().eq.slice(8)).toEqual([-4, -4]);
    fireEvent.change(screen.getByRole('slider', { name: 'Crossfade' }), { target: { value: '5' } });
    expect(state().crossfadeSec).toBe(5);
    await user.click(screen.getByRole('button', { name: 'EQ: equalizer on' }));
    expect(state().eqEnabled).toBe(false);
    await user.click(screen.getByRole('button', { name: 'Repeat: off' }));
    expect(state().repeat).toBe('all');
    await user.click(screen.getByRole('button', { name: 'Shuffle' }));
    expect(state().shuffle).toBe(true);
    await user.click(screen.getByRole('button', { name: 'Mute' }));
    expect(state().muted).toBe(true);
  });
});

describe('study skin', () => {
  it('runs the default focus timer: start, pause, break and reset', async () => {
    const user = userEvent.setup();
    render(<StudySkin />);
    const timer = screen.getByRole('timer');
    expect(timer.textContent).toBe('25:00');
    await user.click(screen.getByRole('button', { name: 'Start' }));
    expect(screen.getByRole('button', { name: 'Pause timer' })).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Pause timer' }));
    await user.click(screen.getByRole('button', { name: 'Take a break' }));
    expect(screen.getByText('Break time')).toBeTruthy();
    expect(screen.getByRole('timer').textContent).toBe('05:00');
    await user.click(screen.getByRole('button', { name: 'Reset timer' }));
    expect(screen.getByRole('timer').textContent).toBe('25:00');
  });

  it('keeps a note for the tab and says it is not saved', async () => {
    const user = userEvent.setup();
    const { unmount } = render(<StudySkin />);
    const pad = screen.getByRole('textbox', { name: 'Notes' });
    await user.type(pad, 'Finish chapter 4');
    expect(screen.getByText('Kept in this tab only, not saved.')).toBeTruthy();
    unmount();
    render(<StudySkin />);
    expect((screen.getByRole('textbox', { name: 'Notes' }) as HTMLTextAreaElement).value).toBe(
      'Finish chapter 4',
    );
  });

  it('uses the tools the integrator binds (wave 32) instead of the defaults', async () => {
    const user = userEvent.setup();
    let toggled = 0;
    let saved = '';
    render(<StudySkin />);
    act(() => {
      registerSkinExtras({
        useFocusTimer: () => ({
          clock: '12:34',
          running: true,
          progress: 0.5,
          segment: 'focus',
          toggle: () => toggled++,
          reset: () => undefined,
          skip: () => undefined,
        }),
        useNotes: () => ({
          status: 'ready',
          text: 'Shared note',
          setText: (s) => (saved = s),
          persistent: true,
        }),
      });
    });
    expect(screen.getByRole('timer').textContent).toBe('12:34');
    await user.click(screen.getByRole('button', { name: 'Pause timer' }));
    expect(toggled).toBe(1);
    expect(screen.getByText('Saved to your encrypted notes.')).toBeTruthy();
    fireEvent.change(screen.getByRole('textbox', { name: 'Notes' }), { target: { value: 'x' } });
    expect(saved).toBe('x');
  });

  it('hides the notes pad when the bound notes are unavailable', () => {
    registerSkinExtras({
      useNotes: () => ({
        status: 'unavailable',
        text: '',
        setText: () => undefined,
        persistent: true,
      }),
    });
    render(<StudySkin />);
    expect(screen.queryByRole('textbox', { name: 'Notes' })).toBeNull();
  });
});

describe('default pomodoro', () => {
  it('ends a focus segment into a paused break', () => {
    let now = 1_000;
    const p = createPomodoro(() => now);
    p.toggle();
    now += FOCUS_MS + 1;
    p.settle();
    expect(p.getState()).toEqual({ segment: 'short', accumulated: 0, startedAt: null });
  });
});

describe('stack skin', () => {
  it('drives the 10-band equalizer: a band, a preset and the ON switch', async () => {
    const user = userEvent.setup();
    render(<StackSkin />);
    const bands = within(screen.getByRole('group', { name: 'Equalizer bands' })).getAllByRole(
      'slider',
    );
    expect(bands).toHaveLength(10);
    fireEvent.change(bands[0]!, { target: { value: '6' } });
    expect(state().eq[0]).toBe(6);
    await user.selectOptions(screen.getByRole('combobox', { name: 'Preset' }), 'rock');
    expect(state().eq).toEqual([...EQ_PRESETS.rock]);
    await user.click(screen.getByRole('button', { name: 'Equalizer on' }));
    expect(state().eqEnabled).toBe(false);
    expect((bands[0] as HTMLInputElement).disabled).toBe(true);
  });

  it('shows and hides the equalizer and playlist windows', async () => {
    const user = userEvent.setup();
    render(<StackSkin />);
    await user.click(screen.getByRole('button', { name: 'EQ: equalizer window' }));
    expect(screen.queryByRole('group', { name: 'Equalizer bands' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Hide the playlist window' }));
    expect(screen.queryByRole('listbox')).toBeNull();
    expect(
      screen.getByRole('button', { name: 'PL: playlist window' }).getAttribute('aria-pressed'),
    ).toBe('false');
    await user.click(screen.getByRole('button', { name: 'PL: playlist window' }));
    expect(screen.getByRole('listbox', { name: 'Play queue' })).toBeTruthy();
  });

  it('plays from the playlist with Enter or a double-click, and removes with Delete or REM', async () => {
    const user = userEvent.setup();
    render(<StackSkin />);
    const list = screen.getByRole('listbox', { name: 'Play queue' });
    const opts = within(list).getAllByRole('option');
    expect(opts).toHaveLength(3);
    expect(opts[0]!.getAttribute('aria-current')).toBe('true');
    act(() => list.focus());
    await user.keyboard('{ArrowDown}{Enter}');
    expect(state().index).toBe(1);
    await user.dblClick(within(list).getByText('Charlie'));
    expect(state().index).toBe(2);
    await user.click(within(list).getByText('Alpha — The Band'));
    await user.keyboard('{Delete}');
    expect(state().queue.map((t) => t.id)).toEqual(['b', 'c']);
    await user.click(
      screen.getByRole('button', { name: 'Rem: remove the selected track from the queue' }),
    );
    expect(state().queue).toHaveLength(1);
  });

  it('shows the format the library knows and leaves unknown readouts dark', () => {
    const { container } = render(<StackSkin />);
    expect(container.querySelector('.ss-fmt')?.textContent).toContain('MP3');
    expect(container.querySelector('.ss-fmt')?.getAttribute('data-lit')).toBe('true');
    const dark = container.querySelectorAll('.ss-readout[data-lit="false"]');
    // bitrate, sample rate and mono/stereo: nothing in the player knows them.
    expect(dark.length).toBe(3);
    act(() => play(1));
    expect(container.querySelector('.ss-fmt')?.getAttribute('data-lit')).toBe('false');
  });

  it('shows the position against the queue length', () => {
    render(<StackSkin />);
    expect(screen.getByText('0:10/6:40')).toBeTruthy();
  });
});

describe('shuffle', () => {
  it('shuffles what is to come, keeps the current track, and restores the order', () => {
    play(tracks, 0);
    const seq = [0.1, 0.9, 0.5];
    let k = 0;
    setShuffle(true, () => seq[k++ % seq.length]!);
    expect(state().queue[0]!.id).toBe('a');
    expect(
      state()
        .queue.map((t) => t.id)
        .sort(),
    ).toEqual(['a', 'b', 'c']);
    play(state().queue.findIndex((t) => t.id === 'c'));
    enqueue({ id: 'd', title: 'Delta', src: '/d.mp3' });
    removeAt(state().queue.findIndex((t) => t.id === 'b'));
    setShuffle(false);
    expect(state().queue.map((t) => t.id)).toEqual(['a', 'c', 'd']);
    expect(state().queue[state().index]!.id).toBe('c');
  });

  it('turns off for a new queue', () => {
    setShuffle(true);
    play(tracks);
    expect(state().shuffle).toBe(false);
    expect(state().unshuffled).toBeNull();
  });
});

describe('levels', () => {
  it('reads a level, a low and a high band and the spectrum from the analyser bins', () => {
    const silent = readBands(new Uint8Array(32));
    expect(silent.level).toBe(0);
    expect(silent.bands).toHaveLength(SPECTRUM_BANDS);
    const bass = new Uint8Array(32);
    bass[0] = 255;
    bass[1] = 255;
    const v = readBands(bass);
    expect(v.low).toBeGreaterThan(0.5);
    expect(v.high).toBe(0);
    expect(v.bands[0]).toBe(1);
    expect(v.bands[SPECTRUM_BANDS - 1]).toBe(0);
  });
});

describe('settings skin picker', () => {
  it('offers the backdrop skins', async () => {
    const user = userEvent.setup();
    render(<PlayerSkinCard />);
    await user.click(screen.getByRole('combobox', { name: 'Player skin' }));
    await user.click(await screen.findByRole('option', { name: 'Steel Stack' }));
    expect(state().skin).toBe('stack');
    expect(screen.getByText(/Three stacked steel windows/)).toBeTruthy();
  });
});
