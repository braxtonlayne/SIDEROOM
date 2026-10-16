import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { resetE2ee, setE2ee } from '../../../voice/e2ee/store';
import { verifiedStore } from '../../../voice/e2ee/verified';
import { MockDataProvider } from '../../mockData';
import { E2eeShield, PrivacyCodes, type PrivacyParticipant } from '../PrivacyCodes';

beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

const people: PrivacyParticipant[] = [
  { userId: '2', name: 'Ana', code: '01234 56789 01234 56789 01234 56789', keyFingerprint: 'b' },
  { userId: '3', name: 'Bo', code: '99999 88888 77777 66666 55555 44444', keyFingerprint: 'c' },
];

describe('privacy codes popover (VOX-E2E-007, D-250)', () => {
  it('shows the call code and pairwise codes and marks participants verified', async () => {
    localStorage.clear();
    const store = verifiedStore(localStorage);
    const user = userEvent.setup();
    render(
      <PrivacyCodes
        callCode="12345 12345 12345 12345 12345 12345"
        participants={people}
        store={store}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'End-to-end encryption' }));
    expect(screen.getByLabelText('Call code')).toHaveTextContent('12345 12345');
    expect(screen.getByLabelText('Privacy code of Ana')).toHaveTextContent(
      '01234 56789 01234 56789 01234 56789',
    );
    const verify = screen.getAllByRole('button', { name: 'Mark as verified' });
    expect(verify).toHaveLength(2);
    await user.click(verify[0]!);
    expect(screen.getByRole('button', { name: 'Verified' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(store.isVerified('2', 'b')).toBe(true);
    await user.click(screen.getByRole('button', { name: 'Mark as verified' }));
    expect(
      screen.getByRole('button', { name: 'End-to-end encrypted, everyone verified' }),
    ).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('says the call is being secured before the first epoch', async () => {
    const user = userEvent.setup();
    render(<PrivacyCodes callCode={null} participants={[]} store={verifiedStore(localStorage)} />);
    await user.click(screen.getByRole('button', { name: 'Securing the call…' }));
    expect(screen.queryByLabelText('Call code')).not.toBeInTheDocument();
  });

  it('the shield follows the call state and hides when E2EE is off', () => {
    resetE2ee();
    const { container } = render(
      <MockDataProvider>
        <E2eeShield />
      </MockDataProvider>,
    );
    expect(container).toBeEmptyDOMElement();
    act(() =>
      setE2ee({
        active: true,
        selfKey: 'aa',
        pairwise: { '2': '11111 11111 11111 11111 11111 11111' },
        call: {
          status: 'secure',
          epoch: 3,
          callCode: '22222 22222 22222 22222 22222 22222',
          members: [
            { userId: '1', identityKey: 'aa' },
            { userId: '2', identityKey: 'bb' },
          ],
          error: null,
        },
      }),
    );
    expect(screen.getByRole('button', { name: 'End-to-end encryption' })).toBeInTheDocument();
    act(() => resetE2ee());
    expect(screen.queryByRole('button', { name: 'End-to-end encryption' })).toBeNull();
  });
});
