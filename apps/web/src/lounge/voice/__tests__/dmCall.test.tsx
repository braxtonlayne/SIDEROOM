import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import { applyCallEvent, resetCalls, useCalls } from '../../../voice/calls';
import { setVoiceController, VoiceController } from '../../../voice/controller';
import { patchSession, resetSession } from '../../../voice/session';
import { useStore } from '../../../store/store';
import { MockDataProvider } from '../../mockData';
import { DmCallStage, STAGE_DEFAULT, STAGE_HEIGHT_KEY, STAGE_MIN } from '../DmCallStage';

beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

const rung: string[] = [];

beforeEach(() => {
  localStorage.clear();
  resetCalls();
  resetSession();
  rung.length = 0;
  useStore.setState({ currentUserId: '1' });
  const c = new VoiceController();
  vi.spyOn(c, 'playRing').mockImplementation(() => void rung.push('ring'));
  setVoiceController(c);
  patchSession({ status: 'connected', guildId: '@me', channelId: '9003' });
  server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })));
});

describe('call ringing state', () => {
  it('tracks who is still ringing in a call the viewer is in, and clears it when the call ends', () => {
    applyCallEvent('CALL_CREATE', {
      channel_id: '9003',
      ringing: ['2', '3'],
      initiator_id: '1',
      group: true,
    });
    expect(useCalls.getState().ringing['9003']).toEqual(['2', '3']);
    // The viewer started it, so there is nothing incoming.
    expect(useCalls.getState().incoming).toEqual([]);
    applyCallEvent('CALL_UPDATE', { channel_id: '9003', ringing: ['3'], initiator_id: '1' });
    expect(useCalls.getState().ringing['9003']).toEqual(['3']);
    applyCallEvent('CALL_DELETE', { channel_id: '9003' });
    expect(useCalls.getState().ringing['9003']).toBeUndefined();
  });

  it('does not list the viewer as someone being called', () => {
    applyCallEvent('CALL_CREATE', { channel_id: '9004', ringing: ['1', '2'], initiator_id: '2' });
    expect(useCalls.getState().ringing['9004']).toEqual(['2']);
    expect(useCalls.getState().incoming).toHaveLength(1);
  });
});

describe('DM call stage (VOX-UI-011, VOX-UI-010)', () => {
  function show() {
    const user = userEvent.setup();
    render(
      <MockDataProvider>
        <DmCallStage channelId="9003" />
      </MockDataProvider>,
    );
    return user;
  }

  it('shows you, a Calling tile for everyone still ringing, and rings back while you wait alone', async () => {
    act(() => useCalls.setState({ ringing: { '9003': ['2'] } }));
    show();
    expect(await screen.findByTestId('voice-tile-1')).toBeInTheDocument();
    const tile = screen.getByTestId('ringing-2');
    expect(tile).toHaveAccessibleName('Calling Mara');
    expect(within(tile).getByText('Calling...')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Cancel call' })).toBeInTheDocument();
    // A ringback right away and again on the interval.
    expect(rung.length).toBeGreaterThan(0);
  });

  it('stops ringing everyone who has not answered when you cancel', async () => {
    let called = '';
    server.use(
      http.post('*/api/v1/channels/9003/call/stop-ringing', ({ request }) => {
        called = new URL(request.url).pathname;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    act(() => useCalls.setState({ ringing: { '9003': ['2'] } }));
    const user = show();
    await user.click(await screen.findByRole('button', { name: 'Cancel call' }));
    await waitFor(() => expect(called).toBe('/api/v1/channels/9003/call/stop-ringing'));
  });

  it('drops the ringing tile and the ringback once the other person is in the call', async () => {
    act(() => useCalls.setState({ ringing: { '9003': [] } }));
    show();
    await screen.findByTestId('voice-tile-1');
    expect(screen.queryByTestId('ringing-2')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Cancel call' })).toBeNull();
    expect(rung).toHaveLength(0);
  });

  it('resizes with the keyboard, within limits, and remembers the size', async () => {
    show();
    const handle = await screen.findByRole('separator', { name: 'Resize the call' });
    expect(handle).toHaveAttribute('aria-valuenow', String(STAGE_DEFAULT));
    handle.focus();
    fireEvent.keyDown(handle, { key: 'ArrowDown' });
    expect(handle).toHaveAttribute('aria-valuenow', String(STAGE_DEFAULT + 16));
    expect(localStorage.getItem(STAGE_HEIGHT_KEY)).toBe(String(STAGE_DEFAULT + 16));
    fireEvent.keyDown(handle, { key: 'Home' });
    expect(handle).toHaveAttribute('aria-valuenow', String(STAGE_MIN));
    fireEvent.keyDown(handle, { key: 'ArrowUp' });
    expect(handle).toHaveAttribute('aria-valuenow', String(STAGE_MIN));
    expect(screen.getByTestId('dm-call-stage').firstElementChild).toHaveStyle({
      blockSize: `${STAGE_MIN}px`,
    });
  });

  it('resizes by dragging the handle', async () => {
    show();
    const handle = await screen.findByRole('separator', { name: 'Resize the call' });
    // jsdom has no PointerEvent: a MouseEvent with the pointer event name carries clientY just as well.
    const fire = (type: string, clientY: number) =>
      act(() => {
        handle.dispatchEvent(new MouseEvent(type, { clientY, bubbles: true }));
      });
    fire('pointerdown', 100);
    fire('pointermove', 160);
    expect(handle).toHaveAttribute('aria-valuenow', String(STAGE_DEFAULT + 60));
    fire('pointerup', 160);
    fire('pointermove', 400);
    expect(handle).toHaveAttribute('aria-valuenow', String(STAGE_DEFAULT + 60));
  });

  it('has no text chat button (the conversation is right below) and no a11y violations', async () => {
    show();
    const stage = await screen.findByTestId('dm-call-stage');
    expect(within(stage).queryByRole('button', { name: 'Text chat' })).toBeNull();
    expect(await axeViolations(stage)).toEqual([]);
  });
});
