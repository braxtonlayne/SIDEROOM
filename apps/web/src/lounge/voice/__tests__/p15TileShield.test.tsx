import { act, render, screen, within } from '@testing-library/react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetE2ee, setE2ee } from '../../../voice/e2ee/store';
import { MockDataProvider } from '../../mockData';
import { ParticipantTile } from '../Tiles';

beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());
beforeEach(() => resetE2ee());

const call = (status: 'secure' | 'waiting') => ({
  status,
  epoch: 2,
  callCode: '11111 22222',
  members: [{ userId: '1', identityKey: 'ab' }],
  error: null,
});

function tile() {
  return render(
    <MockDataProvider>
      <ul>
        <ParticipantTile
          guildId="1001"
          channelId="2011"
          participant={{ userId: '1' }}
          onError={() => {}}
        />
      </ul>
    </MockDataProvider>,
  );
}

describe('participant tile E2EE shield (VOX-UI-003)', () => {
  it('shows no shield in a call that is not end-to-end encrypted', () => {
    tile();
    expect(
      within(screen.getByTestId('voice-tile-1')).queryByLabelText('End-to-end encrypted'),
    ).toBeNull();
  });

  it('shows the shield once the person is secured in the current epoch', () => {
    tile();
    act(() => setE2ee({ active: true, call: call('secure') }));
    expect(
      within(screen.getByTestId('voice-tile-1')).getByLabelText('End-to-end encrypted'),
    ).toBeInTheDocument();
    act(() => setE2ee({ call: call('waiting') }));
    expect(
      within(screen.getByTestId('voice-tile-1')).queryByLabelText('End-to-end encrypted'),
    ).toBeNull();
  });
});
