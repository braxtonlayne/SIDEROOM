import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { browserAudioDeps } from '../../../voice/deps';
import { usePrefsStore } from '../prefs';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../../../voice/settings';
import { FakeAnalyser, FakeAudioContext, FakeStream } from '../../../voice/__tests__/fakes';
import { openSettings, resetPrefs, signedIn } from './helpers';

const device = (kind: MediaDeviceKind, deviceId: string, label: string) =>
  ({ kind, deviceId, label, groupId: 'g', toJSON: () => ({}) }) as MediaDeviceInfo;

const state = () => useVoiceSettings.getState().settings;

beforeEach(() => {
  resetPrefs();
  signedIn();
  useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
  vi.spyOn(browserAudioDeps, 'enumerateDevices').mockResolvedValue([
    device('audioinput', 'default', 'Default'),
    device('audioinput', 'usb-1', 'USB Podcast Mic'),
    device('audiooutput', 'default', 'Default'),
    device('audiooutput', 'hp-1', 'Headphones'),
  ]);
  vi.spyOn(browserAudioDeps, 'queryMicPermission').mockResolvedValue('granted');
  vi.spyOn(browserAudioDeps, 'onDeviceChange').mockReturnValue(() => undefined);
});
afterEach(() => vi.restoreAllMocks());

describe('Settings > Voice & Video', () => {
  it('appears in the settings navigation', async () => {
    openSettings('voice');
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Voice & Video' }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: 'Voice & Video' }).length).toBeGreaterThan(0);
  });

  it('has no accessibility violations', async () => {
    openSettings('voice');
    await screen.findByRole('heading', { level: 1, name: 'Voice & Video' });
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('lists input and output devices by name and stores the choice', async () => {
    const user = userEvent.setup();
    openSettings('voice');
    const input = await screen.findByRole('combobox', { name: 'Input device' });
    await user.click(input);
    await user.click(await screen.findByRole('option', { name: 'USB Podcast Mic' }));
    expect(state().inputDeviceId).toBe('usb-1');
  });

  it('sets input and output volume up to 200%', async () => {
    openSettings('voice');
    const input = await screen.findByRole('slider', { name: 'Input volume' });
    fireEvent.change(input, { target: { value: '170' } });
    fireEvent.change(screen.getByRole('slider', { name: 'Output volume' }), {
      target: { value: '35' },
    });
    expect(state().inputVolume).toBe(170);
    expect(state().outputVolume).toBe(35);
  });

  it('turns the watch party camera bubbles off and on (D-265)', async () => {
    const user = userEvent.setup();
    openSettings('voice');
    const sw = await screen.findByRole('switch', { name: /Show cameras over watch parties/ });
    expect(sw).toBeChecked();
    await user.click(sw);
    expect(usePrefsStore.getState().prefs.watchPartyCameras).toBe(false);
    await user.click(sw);
    expect(usePrefsStore.getState().prefs.watchPartyCameras).toBe(true);
  });

  it('explains a blocked microphone and offers to check again', async () => {
    const user = userEvent.setup();
    vi.spyOn(browserAudioDeps, 'getUserMedia').mockRejectedValue(
      new DOMException('denied', 'NotAllowedError'),
    );
    openSettings('voice');
    await user.click(await screen.findByRole('button', { name: "Let's check" }));
    expect(await screen.findByText('Microphone blocked')).toBeInTheDocument();
    expect(screen.getByText(/lock icon in your browser address bar/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Check again' })).toBeInTheDocument();
  });

  it('shows a live level while testing and releases the microphone when stopped', async () => {
    const user = userEvent.setup();
    const ctx = new FakeAudioContext();
    const stream = new FakeStream();
    vi.spyOn(browserAudioDeps, 'getUserMedia').mockResolvedValue(stream as unknown as MediaStream);
    vi.spyOn(browserAudioDeps, 'createAudioContext').mockReturnValue(
      ctx as unknown as AudioContext,
    );
    openSettings('voice');
    await user.click(await screen.findByRole('button', { name: "Let's check" }));
    (ctx.analysers[0] as FakeAnalyser).amplitude = 0.1;
    const meter = screen.getAllByRole('meter', { name: 'Microphone level' })[0]!;
    await waitFor(() => expect(Number(meter.getAttribute('aria-valuenow'))).toBeGreaterThan(40));
    await user.click(screen.getByRole('button', { name: 'Stop testing' }));
    expect((stream.getAudioTracks()[0] as unknown as { stopped: boolean }).stopped).toBe(true);
    expect(meter).toHaveAttribute('aria-valuenow', '0');
  });

  it('requests the microphone without noise suppression and with the chosen processing', async () => {
    const user = userEvent.setup();
    const gum = vi
      .spyOn(browserAudioDeps, 'getUserMedia')
      .mockResolvedValue(new FakeStream() as unknown as MediaStream);
    vi.spyOn(browserAudioDeps, 'createAudioContext').mockReturnValue(null);
    openSettings('voice');
    await user.click(await screen.findByRole('switch', { name: /Automatic gain control/ }));
    expect(state().autoGainControl).toBe(false);
    await user.click(screen.getByRole('button', { name: "Let's check" }));
    const audio = (gum.mock.calls[0]![0] as MediaStreamConstraints).audio as MediaTrackConstraints;
    expect(audio).toMatchObject({
      noiseSuppression: false,
      echoCancellation: true,
      autoGainControl: false,
    });
    // D-045: no noise suppression control exists.
    expect(screen.queryByRole('switch', { name: /noise/i })).toBeNull();
    expect(screen.getByText(/no noise suppression/i)).toBeInTheDocument();
  });

  describe('input mode', () => {
    it('Voice Activity: automatic sensitivity or a manual threshold', async () => {
      const user = userEvent.setup();
      openSettings('voice');
      const auto = await screen.findByRole('switch', {
        name: /Automatically determine input sensitivity/,
      });
      expect(auto).toBeChecked();
      const slider = screen.getByRole('slider', { name: 'Input sensitivity' });
      expect(slider).toBeDisabled();
      await user.click(auto);
      expect(state().autoSensitivity).toBe(false);
      expect(slider).toBeEnabled();
      fireEvent.change(slider, { target: { value: '-35' } });
      expect(state().thresholdDb).toBe(-35);
      // The meter marks the manual threshold.
      const meter = screen.getByRole('meter', { name: 'Input sensitivity meter' });
      expect(meter.querySelector('.us-meter-mark')).not.toBeNull();
    });

    it('Push to Talk: records a key, sets the release delay, warns while unbound', async () => {
      const user = userEvent.setup();
      openSettings('voice');
      await user.click(await screen.findByRole('radio', { name: 'Push to Talk' }));
      expect(state().inputMode).toBe('ptt');
      expect(screen.queryByRole('slider', { name: 'Input sensitivity' })).toBeNull();
      expect(screen.getByText(/Pick a push-to-talk key/)).toBeInTheDocument();

      await user.click(screen.getByRole('button', { name: 'Record Push to Talk keybind' }));
      expect(screen.getByText(/Press a key or combination/)).toBeInTheDocument();
      act(() => {
        window.dispatchEvent(
          new KeyboardEvent('keydown', { code: 'KeyV', key: 'v', bubbles: true, cancelable: true }),
        );
      });
      expect(state().keybinds.ptt).toEqual({ code: 'KeyV' });
      expect(screen.queryByText(/Pick a push-to-talk key/)).toBeNull();
      expect(screen.getByRole('img', { name: 'V' })).toBeInTheDocument();

      const delay = screen.getByRole('slider', { name: 'Push to talk release delay' });
      expect(delay).toHaveAttribute('min', '0');
      expect(delay).toHaveAttribute('max', '2000');
      fireEvent.change(delay, { target: { value: '750' } });
      expect(state().pttReleaseMs).toBe(750);

      await user.click(screen.getByRole('button', { name: 'Clear Push to Talk keybind' }));
      expect(state().keybinds.ptt).toBeNull();
    });

    it('records a lone modifier on release and cancels on Escape', async () => {
      const user = userEvent.setup();
      openSettings('voice');
      await user.click(await screen.findByRole('radio', { name: 'Push to Talk' }));
      await user.click(screen.getByRole('button', { name: 'Record Push to Talk keybind' }));
      act(() => {
        window.dispatchEvent(
          new KeyboardEvent('keydown', { code: 'ShiftLeft', bubbles: true, cancelable: true }),
        );
        window.dispatchEvent(
          new KeyboardEvent('keyup', { code: 'ShiftLeft', bubbles: true, cancelable: true }),
        );
      });
      expect(state().keybinds.ptt).toEqual({ code: 'ShiftLeft' });
      await user.click(screen.getByRole('button', { name: 'Change Push to Talk keybind' }));
      act(() => {
        window.dispatchEvent(
          new KeyboardEvent('keydown', { code: 'Escape', bubbles: true, cancelable: true }),
        );
      });
      expect(state().keybinds.ptt).toEqual({ code: 'ShiftLeft' });
      expect(screen.queryByText(/Press a key/)).toBeNull();
    });
  });

  it('toggles echo cancellation and the sounds', async () => {
    const user = userEvent.setup();
    openSettings('voice');
    await user.click(await screen.findByRole('switch', { name: /Echo cancellation/ }));
    expect(state().echoCancellation).toBe(false);
    await user.click(screen.getByRole('switch', { name: 'Join and leave sounds' }));
    await user.click(screen.getByRole('switch', { name: 'Mute and deafen sounds' }));
    await user.click(screen.getByRole('switch', { name: 'Push to talk sounds' }));
    expect(state()).toMatchObject({
      soundJoinLeave: false,
      soundMuteDeafen: false,
      soundPtt: false,
    });
    fireEvent.change(screen.getByRole('slider', { name: 'Sound volume' }), {
      target: { value: '25' },
    });
    expect(state().soundVolume).toBe(25);
  });

  it('persists to localStorage on this device', async () => {
    openSettings('voice');
    fireEvent.change(await screen.findByRole('slider', { name: 'Output volume' }), {
      target: { value: '80' },
    });
    const saved = JSON.parse(localStorage.getItem('sr.voice.settings')!) as {
      state: { settings: { outputVolume: number } };
    };
    expect(saved.state.settings.outputVolume).toBe(80);
  });
});

describe('Settings > Keybinds', () => {
  it('lists the voice keybinds and records them', async () => {
    const user = userEvent.setup();
    openSettings('keybinds');
    const card = (await screen.findByRole('heading', { name: 'Voice' })).closest('section')!;
    expect(within(card).getByText('Push to Talk')).toBeInTheDocument();
    await user.click(within(card).getByRole('button', { name: 'Record Toggle Mute keybind' }));
    act(() => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', {
          code: 'KeyM',
          ctrlKey: true,
          shiftKey: true,
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    expect(state().keybinds.toggleMute).toEqual({ code: 'KeyM', ctrl: true, shift: true });
    expect(within(card).getByRole('img', { name: 'Ctrl plus Shift plus M' })).toBeInTheDocument();
  });

  it('refuses a key another voice action already uses', async () => {
    const user = userEvent.setup();
    useVoiceSettings.getState().setKeybind('toggleMute', { code: 'KeyM' });
    openSettings('keybinds');
    await user.click(await screen.findByRole('button', { name: 'Record Toggle Deafen keybind' }));
    act(() => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', { code: 'KeyM', bubbles: true, cancelable: true }),
      );
    });
    expect(await screen.findByRole('alert')).toHaveTextContent('Toggle Mute');
    expect(state().keybinds.toggleDeafen).toBeNull();
  });

  it('keeps the read-only defaults and has no accessibility violations', async () => {
    openSettings('keybinds');
    expect(await screen.findByRole('heading', { name: 'Navigation' })).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
