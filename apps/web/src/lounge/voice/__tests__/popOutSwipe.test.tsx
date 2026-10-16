import { act, fireEvent, render, renderHook, screen, within } from '@testing-library/react';
import { useRemoteVideo } from '../../../voice/video';
import { useRef } from 'react';
import { afterAll, beforeAll, vi } from 'vitest';
import { resetStore, useStore } from '../../../store';
import { setVoiceController, VoiceController } from '../../../voice/controller';
import type { EngineEvents, VoiceEngine } from '../../../voice/engine';
import { patchSession, resetSession, useVoiceSession } from '../../../voice/session';
import { MockDataProvider } from '../../mockData';
import { copyStyles, isPopOutSupported, usePopOut } from '../PopOut';
import { isSwipeDown, useSwipeDown } from '../swipe';

beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

class StubEngine implements VoiceEngine {
  status: VoiceEngine['status'] = 'idle';
  constructor(readonly events: EngineEvents) {}
  connect() {
    return new Promise<void>(() => undefined);
  }
  disconnect() {}
  setTransmitting() {}
  setDeafened() {}
  setUserVolume() {}
  setUserMuted() {}
  setMasterVolume() {}
  setInputVolume() {}
  async setInputDevice() {}
  async setOutputDevice() {}
  restartIce() {}
}

describe('swipe down to minimise (VOX-UI-016)', () => {
  it('needs a long, mostly vertical swipe', () => {
    expect(isSwipeDown(0, 120)).toBe(true);
    expect(isSwipeDown(30, 100)).toBe(true);
    expect(isSwipeDown(0, 40)).toBe(false);
    expect(isSwipeDown(120, 120)).toBe(false);
    expect(isSwipeDown(0, -200)).toBe(false);
  });

  const touch = (el: Element, type: string, x: number, y: number) =>
    fireEvent(
      el,
      Object.assign(new Event(type, { bubbles: true }), {
        touches: type === 'touchend' ? [] : [{ clientX: x, clientY: y }],
        changedTouches: [{ clientX: x, clientY: y }],
      }),
    );

  function Host({ onSwipe }: { onSwipe?: () => void }) {
    const ref = useRef<HTMLDivElement | null>(null);
    useSwipeDown(ref, onSwipe);
    return <div ref={ref} data-testid="stage" />;
  }

  it('minimises on a swipe down from the top, but not on a short swipe, or while scrolled', () => {
    const onSwipe = vi.fn();
    render(<Host onSwipe={onSwipe} />);
    const stage = screen.getByTestId('stage');
    touch(stage, 'touchstart', 100, 100);
    touch(stage, 'touchend', 100, 150);
    expect(onSwipe).not.toHaveBeenCalled();
    touch(stage, 'touchstart', 100, 100);
    touch(stage, 'touchend', 110, 260);
    expect(onSwipe).toHaveBeenCalledTimes(1);
    stage.scrollTop = 40;
    touch(stage, 'touchstart', 100, 100);
    touch(stage, 'touchend', 100, 300);
    expect(onSwipe).toHaveBeenCalledTimes(1);
  });

  it('does nothing without a handler (desktop)', () => {
    render(<Host />);
    const stage = screen.getByTestId('stage');
    touch(stage, 'touchstart', 100, 100);
    touch(stage, 'touchend', 100, 300);
  });
});

describe('pop-out window (VOX-UI-012)', () => {
  let pip: Window;
  let frame: HTMLIFrameElement;
  let closeSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    resetStore();
    useStore.setState({ currentUserId: '1' });
    resetSession();
    // A real second window: an iframe stands in for the Picture-in-Picture window.
    frame = document.body.appendChild(document.createElement('iframe'));
    pip = frame.contentWindow!;
    closeSpy = vi.fn();
    pip.close = closeSpy as unknown as Window['close'];
    Object.defineProperty(globalThis, 'documentPictureInPicture', {
      configurable: true,
      value: { requestWindow: vi.fn(async () => pip) },
    });
    setVoiceController(
      new VoiceController({
        createEngine: (events) => new StubEngine(events),
        sendGateway: () => true,
        subscribe: () => () => undefined,
        currentUserId: () => '1',
        voiceStatesOf: () => ({}),
        audio: {
          getUserMedia: () => Promise.reject(new Error('none')),
          createAudioContext: () => null,
          createAudioElement: () => document.createElement('audio'),
          enumerateDevices: async () => [],
          onDeviceChange: () => () => undefined,
          queryMicPermission: async () => 'prompt',
        },
        now: () => Date.now(),
      }),
    );
    patchSession({
      status: 'connected',
      guildId: '1001',
      channelId: '2011',
      channelName: 'General',
    });
  });
  afterEach(() => {
    setVoiceController(null);
    Reflect.deleteProperty(globalThis, 'documentPictureInPicture');
    frame.remove();
  });

  function Host() {
    const p = usePopOut('2011');
    return (
      <MockDataProvider>
        <button type="button" onClick={() => void p.open()}>
          open
        </button>
        <button type="button" onClick={p.close}>
          close
        </button>
        <span data-testid="state">{p.isOpen ? 'open' : 'closed'}</span>
        {p.portal}
      </MockDataProvider>
    );
  }

  it('detects support', () => {
    expect(isPopOutSupported()).toBe(true);
    Reflect.deleteProperty(globalThis, 'documentPictureInPicture');
    expect(isPopOutSupported()).toBe(false);
  });

  it('copies stylesheets and theme attributes into the new window', () => {
    document.documentElement.setAttribute('data-theme', 'dark');
    const style = document.createElement('style');
    style.textContent = '.x{color:red}';
    document.head.appendChild(style);
    const target = document.implementation.createHTMLDocument('t');
    copyStyles(document, target);
    expect(target.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(target.head.querySelector('style')?.textContent).toContain('color:red');
    style.remove();
  });

  it('renders the controls into the pop-out, mutes from there, and closes with the call', async () => {
    render(<Host />);
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'open' }));
      await Promise.resolve();
    });
    expect(screen.getByTestId('state')).toHaveTextContent('open');
    const body = within(pip.document.body);
    expect(body.getByTestId('voice-popout')).not.toBeNull();
    expect(body.getByRole('heading', { name: 'General' })).not.toBeNull();
    fireEvent.click(body.getByRole('button', { name: 'Mute' }));
    expect(useVoiceSession.getState().selfMute).toBe(true);
    expect(body.getByRole('button', { name: 'Unmute' }).getAttribute('aria-pressed')).toBe('true');
    // Leaving the call closes the window.
    act(() => patchSession({ status: 'idle' }));
    expect(closeSpy).toHaveBeenCalled();
    expect(screen.getByTestId('state')).toHaveTextContent('closed');
  });

  it('pops out a stream you are watching, with sound', async () => {
    render(<Host />);
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'open' }));
      await Promise.resolve();
    });
    const body = within(pip.document.body);
    expect(body.queryByLabelText('Your camera or screen')).toBeNull();
    act(() =>
      useRemoteVideo.setState({
        watching: { '2': true },
        screen: { '2': { getTracks: () => [] } as unknown as MediaStream },
      }),
    );
    const video = pip.document.body.querySelector('video');
    expect(video).not.toBeNull();
    expect(video!.muted).toBe(false);
    act(() => useRemoteVideo.setState({ watching: {}, screen: {} }));
  });

  it('forgets the window when the person closes it', async () => {
    render(<Host />);
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'open' }));
      await Promise.resolve();
    });
    act(() => {
      pip.dispatchEvent(new Event('pagehide'));
    });
    expect(screen.getByTestId('state')).toHaveTextContent('closed');
  });

  it('hook reports unsupported without the API', () => {
    Reflect.deleteProperty(globalThis, 'documentPictureInPicture');
    const { result } = renderHook(() => usePopOut('2011'));
    expect(result.current.supported).toBe(false);
  });
});
