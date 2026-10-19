import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { act } from 'react';
import { WebcamBubbles } from '../WebcamBubbles';
import { useStore } from '../../store/store';
import { IDLE_SESSION, useVoiceSession } from '../../voice/session';
import { useRemoteVideo } from '../../voice/video';

const stream = {} as MediaStream;

afterEach(() => {
  useVoiceSession.setState(IDLE_SESSION);
  useRemoteVideo.setState({ camera: {}, videoOff: {} });
});

describe('WebcamBubbles', () => {
  it('renders nothing outside a voice call', () => {
    render(<WebcamBubbles />);
    expect(screen.queryByTestId('wp-bubbles')).toBeNull();
  });

  it('is off until asked, then shows the cameras that are on and skips ones the viewer disabled', async () => {
    useStore.setState({
      users: { '2': { id: '2', username: 'bob' }, '3': { id: '3', username: 'cy' } },
    } as never);
    useVoiceSession.setState({ status: 'connected' });
    useRemoteVideo.setState({ camera: { '2': stream, '3': stream }, videoOff: { '3': true } });
    render(<WebcamBubbles />);
    expect(screen.queryByTestId('wp-bubble-2')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Show cameras' }));
    expect(screen.getByTestId('wp-bubble-2')).toBeInTheDocument();
    expect(screen.queryByTestId('wp-bubble-3')).toBeNull();
    expect(screen.getByLabelText('Camera of bob')).toBeInTheDocument();
    act(() => useRemoteVideo.setState({ camera: {} }));
    expect(screen.getByRole('status')).toHaveTextContent('Nobody in the call has a camera on.');
  });
});
