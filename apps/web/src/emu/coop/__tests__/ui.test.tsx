// Play together UI (ACT-EMU-002): the dialog as host and as guest against a relay, and the status chip.
import { ThemeProvider } from '@sideroom/ui';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { patchSession, resetSession } from '../../../voice/session';
import { CoopDialog } from '../CoopDialog';
import { CoopHost } from '../host';
import { PAD, bit } from '../input';
import { coopSession } from '../session';
import { CoopStatus } from '../CoopStatus';
import { offerCoopGame, withdrawCoopGame } from '../source';
import { Hub, RecordingCore } from './hub';

vi.mock('../../../lounge/data', () => ({
  useCurrentUser: () => ({ id: '2', username: 'me', displayName: 'Me', avatarUrl: null }),
  useUsers: () => ({
    '1': { id: '1', username: 'hal', displayName: 'Hal', avatarUrl: null },
    '2': { id: '2', username: 'me', displayName: 'Me', avatarUrl: null },
  }),
}));

const wrap = (ui: React.ReactElement) =>
  render(<ThemeProvider section="lounge">{ui}</ThemeProvider>);

let hub: Hub;
beforeEach(() => {
  hub = new Hub();
  coopSession.stop();
  coopSession.setTransportFactory(() => hub.transport('2'));
  patchSession({ quality: { level: 'good', pingMs: 23, lossPct: 0 } });
});
afterEach(() => {
  coopSession.stop();
  withdrawCoopGame();
  resetSession();
});

describe('Play together dialog', () => {
  it('says there is no rollback and, with no game running, how to get one', async () => {
    wrap(<CoopDialog open onClose={() => undefined} onShare={() => undefined} />);
    expect(await screen.findByTestId('coop-no-rollback')).toHaveTextContent(/no netplay rollback/i);
    expect(screen.getByTestId('coop-no-game')).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('hosts the running game: seats appear and can be removed', async () => {
    const user = userEvent.setup();
    const core = new RecordingCore();
    offerCoopGame(Object.assign(core, { title: 'Test ROM' }));
    const onShare = vi.fn();
    wrap(<CoopDialog open onClose={() => undefined} onShare={onShare} />);
    await user.click(await screen.findByRole('button', { name: 'Host Test ROM' }));
    expect(screen.getByText('You are hosting Test ROM.')).toBeInTheDocument();
    expect(screen.getByText('Your connection to the call: 23 ms.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Share screen' }));
    expect(onShare).toHaveBeenCalled();

    // somebody joins through the relay
    const other = hub.transport('7');
    other.join();
    const { encode } = await import('../protocol');
    await act(async () => {
      await other.send(encode({ type: 'join', host: '2' }));
    });
    expect(await screen.findByText('Player 2: 7')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Remove 7' }));
    expect(screen.getByText('Nobody has taken a seat yet.')).toBeInTheDocument();
  });

  it('lists a host, takes a seat, shows the round trip and ends cleanly', async () => {
    const user = userEvent.setup();
    const core = new RecordingCore();
    const host = new CoopHost({
      transport: hub.transport('1'),
      game: Object.assign(core, { title: 'Test ROM' }),
      selfId: '1',
    });
    host.start();
    wrap(
      <>
        <CoopDialog open onClose={() => undefined} onShare={() => undefined} />
        <CoopStatus />
      </>,
    );
    expect(await screen.findByTestId('coop-host-row')).toHaveTextContent(
      'Hal is hosting Test ROM. 3 seats free.',
    );
    await user.click(screen.getByRole('button', { name: 'Join as a player' }));
    await waitFor(() =>
      expect(screen.getByTestId('coop-status-text')).toHaveTextContent('Player 2 in Test ROM'),
    );
    expect(screen.getByTestId('coop-latency')).toHaveTextContent('Measuring the round trip');
    expect(screen.getByRole('button', { name: 'Controls on' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );

    // the keyboard is the pad now, and reaches port 1 of the host's core
    await user.keyboard('{ArrowRight>}');
    await waitFor(() => expect(core.down.get(1)?.has(PAD.RIGHT)).toBe(true));
    await user.keyboard('{/ArrowRight}');
    await waitFor(() => expect(core.down.get(1)?.has(PAD.RIGHT)).toBe(false));

    await user.click(screen.getByRole('button', { name: 'Controls on' }));
    expect(screen.getByRole('button', { name: 'Controls off' })).toBeInTheDocument();
    await user.keyboard('x');
    expect(core.calls.some(([, b]) => b === PAD.A)).toBe(false);

    await user.click(screen.getByRole('button', { name: 'Leave the game' }));
    expect(screen.queryByTestId('coop-status')).toBeNull();
    expect(host.snapshot().seats).toEqual([]);
    host.stop();
    expect(bit(PAD.A)).toBe(256);
  });

  it('tells a guest when the game is full, and drops the host when it ends', async () => {
    const core = new RecordingCore();
    const host = new CoopHost({
      transport: hub.transport('1'),
      game: Object.assign(core, { title: 'Test ROM' }),
      selfId: '1',
    });
    host.start();
    // three other guests fill the seats
    const { encode } = await import('../protocol');
    for (const id of ['5', '6', '8']) {
      const t = hub.transport(id);
      t.join();
      await t.send(encode({ type: 'join', host: '1' }));
    }
    wrap(<CoopDialog open onClose={() => undefined} onShare={() => undefined} />);
    const row = await screen.findByTestId('coop-host-row');
    expect(row).toHaveTextContent('No seat free');
    expect(screen.getByRole('button', { name: 'Join as a player' })).toBeDisabled();
    await act(async () => host.stop());
    await waitFor(() => expect(screen.queryByTestId('coop-host-row')).toBeNull());
  });
});
