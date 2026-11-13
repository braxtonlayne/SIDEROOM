// SEC-AUD-155 (D-640, wave 63): the Block dialog says what a block does in servers and group DMs.
import { render, screen } from '@testing-library/react';
import { ConfirmAction } from '../shell/ConfirmAction';

describe('Block dialog text', () => {
  it('says that they can still write in servers and group DMs and that their messages are hidden', () => {
    render(
      <ConfirmAction
        kind="block"
        name="Bob"
        onCancel={() => undefined}
        onConfirm={() => undefined}
      />,
    );
    const dialog = screen.getByRole('dialog', { name: 'Block Bob?' });
    expect(dialog).toHaveTextContent('direct messages or friend requests');
    expect(dialog).toHaveTextContent('In servers and group DMs they can still write');
    expect(dialog).toHaveTextContent('hidden from you until you choose to show them');
    expect(dialog).toHaveTextContent('unblock them in Settings');
  });
});
