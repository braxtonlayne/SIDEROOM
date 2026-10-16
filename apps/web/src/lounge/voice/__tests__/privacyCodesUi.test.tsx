import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { verifiedStore } from '../../../voice/e2ee/verified';
import { PrivacyCodes, type PrivacyParticipant } from '../PrivacyCodes';

const people: PrivacyParticipant[] = [
  {
    userId: '1',
    name: 'Me',
    code: '11111 22222 33333 44444 55555 66666',
    keyFingerprint: 'a',
    self: true,
  },
  { userId: '2', name: 'Ana', code: '01234 56789 01234 56789 01234 56789', keyFingerprint: 'b' },
  { userId: '3', name: 'Bo', code: '99999 88888 77777 66666 55555 44444', keyFingerprint: 'c' },
];

describe('privacy codes popover (VOX-E2E-007)', () => {
  it('lists every code and marks participants verified', async () => {
    localStorage.clear();
    const store = verifiedStore(localStorage);
    const user = userEvent.setup();
    render(<PrivacyCodes participants={people} store={store} />);
    await user.click(screen.getByRole('button', { name: 'End-to-end encryption' }));
    expect(screen.getByText('Me (you)')).toBeInTheDocument();
    expect(screen.getByLabelText('Privacy code of Ana')).toHaveTextContent(
      '01234 56789 01234 56789 01234 56789',
    );
    // No verify button for yourself.
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
});
