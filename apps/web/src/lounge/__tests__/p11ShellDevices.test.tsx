import { fireEvent, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../../voice/settings';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const devices = [
  { deviceId: 'default', kind: 'audioinput', label: 'Default' },
  { deviceId: 'mic1', kind: 'audioinput', label: 'Desk microphone' },
  { deviceId: 'mic2', kind: 'audioinput', label: 'Headset microphone' },
  { deviceId: 'spk1', kind: 'audiooutput', label: 'Speakers' },
];

describe('LNG-HOME-012/013 mic and headphone menus', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [] });
    useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: {
        enumerateDevices: () => Promise.resolve(devices),
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
      },
    });
  });
  afterEach(() => {
    Reflect.deleteProperty(navigator, 'mediaDevices');
  });

  it('picks an input device, sets its volume and opens Voice settings', async () => {
    const { router } = at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Input device and volume' }));
    const pop = await screen.findByRole('dialog', { name: 'Input device and volume' });
    const radio = await within(pop).findByRole('radio', { name: 'Headset microphone' });
    await userEvent.click(radio);
    expect(useVoiceSettings.getState().settings.inputDeviceId).toBe('mic2');
    expect(within(pop).queryByRole('radio', { name: 'Speakers' })).toBeNull();
    fireEvent.change(within(pop).getByRole('slider', { name: 'Input volume' }), {
      target: { value: '150' },
    });
    expect(useVoiceSettings.getState().settings.inputVolume).toBe(150);
    await userEvent.click(within(pop).getByRole('button', { name: 'Voice settings' }));
    await vi.waitFor(() => expect(router.state.location.pathname).toBe('/settings/user/voice'));
  });

  it('picks an output device where the browser can switch it', async () => {
    Object.defineProperty(HTMLMediaElement.prototype, 'setSinkId', {
      configurable: true,
      value: () => Promise.resolve(),
    });
    try {
      at('/channels/g1/c1');
      await userEvent.click(
        await screen.findByRole('button', { name: 'Output device and volume' }),
      );
      const pop = await screen.findByRole('dialog', { name: 'Output device and volume' });
      await userEvent.click(await within(pop).findByRole('radio', { name: 'Speakers' }));
      expect(useVoiceSettings.getState().settings.outputDeviceId).toBe('spk1');
      fireEvent.change(within(pop).getByRole('slider', { name: 'Output volume' }), {
        target: { value: '40' },
      });
      expect(useVoiceSettings.getState().settings.outputVolume).toBe(40);
    } finally {
      Reflect.deleteProperty(HTMLMediaElement.prototype, 'setSinkId');
    }
  });

  it('says so when the browser cannot switch the output device', async () => {
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Output device and volume' }));
    const pop = await screen.findByRole('dialog', { name: 'Output device and volume' });
    expect(
      within(pop).getByText('This browser cannot switch the output device.'),
    ).toBeInTheDocument();
  });
});
