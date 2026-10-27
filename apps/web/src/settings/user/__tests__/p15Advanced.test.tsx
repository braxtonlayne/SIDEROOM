import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { PrivacyCodes } from '../../../lounge/voice/PrivacyCodes';
import { verifiedStore } from '../../../voice/e2ee/verified';
import {
  DEFAULT_VOICE_SETTINGS,
  sanitizeVoiceSettings,
  useVoiceSettings,
} from '../../../voice/settings';
import { useSoundboard } from '../../../voice/soundboard';
import { AdvancedVoiceCards, detectHardwareAcceleration } from '../sections/AdvancedVoice';
import { VoiceVideoSection } from '../sections/VoiceVideo';

const canvas = (renderer: string | null | 'nogl') =>
  (() => ({
    getContext: () =>
      renderer === 'nogl'
        ? null
        : {
            getExtension: () => (renderer === null ? null : { UNMASKED_RENDERER_WEBGL: 1 }),
            getParameter: () => renderer,
          },
  })) as unknown as () => HTMLCanvasElement;

describe('Advanced voice settings (LNG-USET-050)', () => {
  beforeEach(() => useVoiceSettings.getState().reset());

  it('reads the renderer name for the hardware acceleration hint', () => {
    expect(detectHardwareAcceleration(canvas('NVIDIA GeForce RTX'))).toBe(true);
    expect(detectHardwareAcceleration(canvas('Google SwiftShader'))).toBe(false);
    expect(detectHardwareAcceleration(canvas('llvmpipe (LLVM 15)'))).toBe(false);
    expect(detectHardwareAcceleration(canvas('nogl'))).toBe(false);
    expect(detectHardwareAcceleration(canvas(null))).toBeNull();
  });

  it('keeps QoS off and privacy codes on by default and validates them', () => {
    expect(DEFAULT_VOICE_SETTINGS.qosHighPriority).toBe(false);
    expect(DEFAULT_VOICE_SETTINGS.showPrivacyCodes).toBe(true);
    expect(sanitizeVoiceSettings({ qosHighPriority: 'yes' }).qosHighPriority).toBe(false);
    expect(sanitizeVoiceSettings({ showPrivacyCodes: false }).showPrivacyCodes).toBe(false);
  });

  it('switches QoS and privacy codes and explains the encryption state', async () => {
    const user = userEvent.setup();
    render(<AdvancedVoiceCards />);
    // Always on (OPS-IMM-001): nothing can turn it off, so there is no other state to show.
    expect(screen.getByText('Calls are end-to-end encrypted')).toBeInTheDocument();
    expect(screen.queryByText('Calls are encrypted in transit')).not.toBeInTheDocument();
    await user.click(screen.getByRole('switch', { name: /High packet priority/ }));
    expect(useVoiceSettings.getState().settings.qosHighPriority).toBe(true);
    await user.click(screen.getByRole('switch', { name: /Show privacy codes/ }));
    expect(useVoiceSettings.getState().settings.showPrivacyCodes).toBe(false);
  });

  it('hides the codes in the call shield when switched off', async () => {
    const user = userEvent.setup();
    const props = {
      callCode: '11111 22222',
      participants: [{ userId: '1', name: 'Ada', code: '33333 44444', keyFingerprint: 'ab' }],
      store: verifiedStore(),
    };
    const { unmount } = render(<PrivacyCodes {...props} />);
    await user.click(screen.getByRole('button'));
    expect(await screen.findByText('11111 22222')).toBeInTheDocument();
    unmount();
    useVoiceSettings.getState().update({ showPrivacyCodes: false });
    render(<PrivacyCodes {...props} />);
    await user.click(screen.getByRole('button'));
    expect(await screen.findByText(/Privacy codes are hidden/)).toBeInTheDocument();
    expect(screen.queryByText('11111 22222')).toBeNull();
  });

  it('sets the soundboard volume from the Voice settings', async () => {
    render(<VoiceVideoSection />);
    const slider = screen.getByRole('slider', { name: 'Soundboard volume' });
    fireEvent.change(slider, { target: { value: '40' } });
    expect(useSoundboard.getState().volume).toBe(40);
  });
});
