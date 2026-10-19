import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { act } from 'react';
import { WebcamBubbles } from '../WebcamBubbles';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { useStore } from '../../store/store';
import { IDLE_SESSION, useVoiceSession } from '../../voice/session';
import { useRemoteVideo } from '../../voice/video';

const stream = {} as MediaStream;

beforeEach(() => usePrefsStore.setState({ prefs: DEFAULT_PREFS }));
afterEach(() => {
  useVoiceSession.setState(IDLE_SESSION);
  useRemoteVideo.setState({ camera: {}, videoOff: {} });
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
});

describe('WebcamBubbles (D-265)', () => {
  it('renders nothing outside a voice call', () => {
    render(<WebcamBubbles />);
    expect(screen.queryByTestId('wp-bubbles')).toBeNull();
  });

  it('shows the cameras that are on without a click and skips ones the viewer disabled', () => {
    useStore.setState({
      users: { '2': { id: '2', username: 'bob' }, '3': { id: '3', username: 'cy' } },
    } as never);
    useVoiceSession.setState({ status: 'connected' });
    useRemoteVideo.setState({ camera: { '2': stream, '3': stream }, videoOff: { '3': true } });
    render(<WebcamBubbles />);
    expect(screen.getByRole('button', { name: 'Hide cameras' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByTestId('wp-bubble-2')).toBeInTheDocument();
    expect(screen.queryByTestId('wp-bubble-3')).toBeNull();
    expect(screen.getByLabelText('Camera of bob')).toBeInTheDocument();
  });

  it('shows only the toggle when nobody has a camera', () => {
    useVoiceSession.setState({ status: 'connected' });
    render(<WebcamBubbles />);
    expect(screen.getByRole('button', { name: 'Hide cameras' })).toBeInTheDocument();
    expect(screen.queryByRole('list')).toBeNull();
    expect(screen.queryByRole('status')).toBeNull();
    act(() => useRemoteVideo.setState({ camera: { '2': stream } }));
    expect(screen.getByTestId('wp-bubble-2')).toBeInTheDocument();
  });

  it('hides on request, remembers it across a remount, and acknowledges Show with nobody on camera', async () => {
    const user = userEvent.setup();
    useVoiceSession.setState({ status: 'connected' });
    useRemoteVideo.setState({ camera: { '2': stream } });
    const first = render(<WebcamBubbles />);
    await user.click(screen.getByRole('button', { name: 'Hide cameras' }));
    expect(usePrefsStore.getState().prefs.watchPartyCameras).toBe(false);
    expect(screen.queryByTestId('wp-bubble-2')).toBeNull();
    first.unmount();

    render(<WebcamBubbles />);
    expect(screen.queryByTestId('wp-bubble-2')).toBeNull();
    act(() => useRemoteVideo.setState({ camera: {} }));
    await user.click(screen.getByRole('button', { name: 'Show cameras' }));
    expect(usePrefsStore.getState().prefs.watchPartyCameras).toBe(true);
    expect(screen.getByRole('status')).toHaveTextContent('Nobody in the call has a camera on.');
  });
});
