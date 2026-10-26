import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { LikelySpamGate } from '../msg/LikelySpamNotice';
import { toLoungeMessage } from '../liveMap';
import type * as S from '../../store/types';

describe('likely spam notice (PAR-FU-002)', () => {
  it('shows an unflagged message as it is', () => {
    render(<LikelySpamGate flagged={false}>hello there</LikelySpamGate>);
    expect(screen.getByText('hello there')).toBeInTheDocument();
    expect(screen.queryByTestId('likely-spam-notice')).toBeNull();
  });

  it('keeps a flagged message behind the notice until the recipient reveals it, and hides it again', async () => {
    render(<LikelySpamGate flagged>win big https://a.example</LikelySpamGate>);
    expect(screen.getByTestId('likely-spam-notice')).toHaveTextContent(
      'This message looks like spam.',
    );
    expect(screen.queryByText('win big https://a.example')).toBeNull();
    const toggle = screen.getByRole('button', { name: 'Show message' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(toggle);
    expect(screen.getByText('win big https://a.example')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Hide message' }));
    expect(screen.queryByText('win big https://a.example')).toBeNull();
  });

  it('reads the message flag from the wire (1 << 29)', () => {
    const raw = {
      id: '1',
      channel_id: '2',
      content: 'x',
      timestamp: '2026-10-26T00:00:00Z',
      author: { id: '3', username: 'u' },
      flags: 1 << 29,
    } as unknown as S.Message;
    expect(toLoungeMessage(raw).likelySpam).toBe(true);
    expect(toLoungeMessage({ ...raw, flags: 0 }).likelySpam).toBeUndefined();
  });
});
