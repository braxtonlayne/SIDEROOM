import { act, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { browserAudioDeps } from '../../../voice/deps';
import { vadHost } from '../../../voice/mic';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../../../voice/settings';
import { FakeAudioContext, FakeStream } from '../../../voice/__tests__/fakes';
import { openSettings, resetPrefs, signedIn } from './helpers';

const state = () => useVoiceSettings.getState().settings;
const group = () => screen.findByRole('radiogroup', { name: 'Voice detection' });

beforeEach(() => {
  resetPrefs();
  signedIn();
  useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
  vi.spyOn(browserAudioDeps, 'enumerateDevices').mockResolvedValue([]);
  vi.spyOn(browserAudioDeps, 'queryMicPermission').mockResolvedValue('granted');
  vi.spyOn(browserAudioDeps, 'onDeviceChange').mockReturnValue(() => undefined);
});
afterEach(() => {
  vi.restoreAllMocks();
  useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
  vadHost.refresh();
});

describe('Settings > Voice & Video > Voice detection (D-1048)', () => {
  it('offers Standard and Silero (on-device model) in voice activity mode, Standard chosen', async () => {
    openSettings('voice');
    const g = await group();
    expect(within(g).getAllByRole('radio')).toHaveLength(2);
    expect(within(g).getByRole('radio', { name: 'Silero (on-device model)' })).toBeInTheDocument();
    expect(within(g).getByRole('radio', { name: 'Standard' })).toBeChecked();
    expect(screen.queryByTestId('vad-engine-status')).toBeNull();
    expect(
      screen.getByRole('switch', { name: /Automatically determine input sensitivity/ }),
    ).toBeEnabled();
  });

  it('is not shown in Push to Talk mode', async () => {
    const user = userEvent.setup();
    openSettings('voice');
    await user.click(await screen.findByRole('radio', { name: 'Push to Talk' }));
    expect(screen.queryByRole('radiogroup', { name: 'Voice detection' })).toBeNull();
  });

  it('choosing Silero updates the setting, explains it, and disables the energy controls', async () => {
    const user = userEvent.setup();
    openSettings('voice');
    await user.click(within(await group()).getByRole('radio', { name: /Silero/ }));
    expect(state().vadEngine).toBe('silero');
    expect(
      screen.getByText(
        'Runs on this device. Nothing is sent anywhere. Downloads about 18 MB the first time.',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('switch', { name: /Automatically determine input sensitivity/ }),
    ).toBeDisabled();
    expect(screen.getByRole('slider', { name: 'Input sensitivity' })).toBeDisabled();
    expect(screen.getByText(/The model decides when you are speaking/)).toBeInTheDocument();
    // the level meter stays
    expect(screen.getByRole('meter', { name: 'Input sensitivity meter' })).toBeInTheDocument();
    // choosing alone loads nothing
    expect(screen.getByTestId('vad-engine-status')).toHaveAttribute('data-state', 'idle');
    await user.click(within(await group()).getByRole('radio', { name: 'Standard' }));
    expect(state().vadEngine).toBe('energy');
    expect(screen.queryByTestId('vad-engine-status')).toBeNull();
    expect(
      screen.getByRole('switch', { name: /Automatically determine input sensitivity/ }),
    ).toBeEnabled();
  });

  it('the status line shows each of the four states', async () => {
    const user = userEvent.setup();
    openSettings('voice');
    await user.click(within(await group()).getByRole('radio', { name: /Silero/ }));
    const expected = {
      idle: /Not loaded yet/,
      loading: /Loading the model/,
      ready: /The model is listening/,
      fallback: /standard voice detection is in use/,
    } as const;
    for (const [s, text] of Object.entries(expected)) {
      vi.spyOn(vadHost, 'state', 'get').mockReturnValue(s as 'idle');
      act(() => (vadHost as unknown as { emit(): void }).emit());
      const line = screen.getByTestId('vad-engine-status');
      expect(line).toHaveAttribute('data-state', s);
      expect(line.textContent).toMatch(text);
    }
  });

  it('the microphone test loads the detector; where it cannot start, the line says fallback and the test goes on', async () => {
    const user = userEvent.setup();
    const ctx = new FakeAudioContext();
    vi.spyOn(browserAudioDeps, 'getUserMedia').mockResolvedValue(
      new FakeStream() as unknown as MediaStream,
    );
    vi.spyOn(browserAudioDeps, 'createAudioContext').mockReturnValue(
      ctx as unknown as AudioContext,
    );
    openSettings('voice');
    await user.click(within(await group()).getByRole('radio', { name: /Silero/ }));
    await user.click(await screen.findByRole('button', { name: "Let's check" }));
    // jsdom has no Worker: the client reports a fallback instead of throwing
    await vi.waitFor(() =>
      expect(screen.getByTestId('vad-engine-status')).toHaveAttribute('data-state', 'fallback'),
    );
    expect(vadHost.snapshot().fallbackReason).toBe('worker');
    expect(screen.getByRole('button', { name: 'Stop testing' })).toBeInTheDocument();
  });
});
