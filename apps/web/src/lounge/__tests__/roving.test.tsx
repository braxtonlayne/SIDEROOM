import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { warmLoungeRoute } from '../../test/warmRoutes';
import { at, baseHandlers, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

warmLoungeRoute();

/**
 * RDY-A11Y-002: the message list and the member list are one Tab stop each with arrow keys inside, and the
 * Lounge has a skip link to its main region.
 */
describe('Lounge keyboard lists', () => {
  beforeEach(() => {
    baseHandlers();
    seedLounge({
      messages: [
        msg('100', { content: 'first message' }),
        msg('101', { content: 'second message' }),
        msg('102', { content: 'third message' }),
      ],
    });
  });

  it('the message list is one Tab stop; Up, Down, Home and End move between messages', async () => {
    const user = userEvent.setup();
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    await waitFor(() => expect(within(log).getAllByRole('article')).toHaveLength(3));
    const rows = () => within(log).getAllByRole('article');
    await waitFor(() => expect(rows().map((r) => r.tabIndex)).toEqual([0, -1, -1]));
    rows()[0]!.focus();
    await user.keyboard('{ArrowDown}');
    expect(rows()[1]).toHaveFocus();
    await user.keyboard('{End}');
    expect(rows()[2]).toHaveFocus();
    expect(rows().map((r) => r.tabIndex)).toEqual([-1, -1, 0]);
    await user.keyboard('{ArrowDown}');
    expect(rows()[2]).toHaveFocus();
    await user.keyboard('{ArrowUp}{Home}');
    expect(rows()[0]).toHaveFocus();
    // the log region itself is no longer a second stop
    expect(log).not.toHaveAttribute('tabindex');
  });

  it('the member list is one Tab stop with arrow keys between members', async () => {
    const user = userEvent.setup();
    at('/channels/g1/c1');
    const list = await screen.findByRole('list', { name: 'Members' });
    const members = () => Array.from(list.querySelectorAll<HTMLElement>('.lng-member'));
    await waitFor(() => expect(members().length).toBeGreaterThanOrEqual(2));
    expect(members().map((m) => m.tabIndex)).toEqual([0, ...members().slice(1).map(() => -1)]);
    members()[0]!.focus();
    await user.keyboard('{ArrowDown}');
    expect(members()[1]).toHaveFocus();
    expect(members()[0]!.tabIndex).toBe(-1);
    expect(members()[1]!.tabIndex).toBe(0);
    await user.keyboard('{ArrowUp}');
    expect(members()[0]).toHaveFocus();
    await user.keyboard('{End}');
    expect(members().at(-1)).toHaveFocus();
  });

  it('has a skip link that moves focus to the main region', async () => {
    const user = userEvent.setup();
    at('/channels/g1/c1');
    await screen.findByRole('log');
    const skip = screen.getByRole('link', { name: 'Skip to main content' });
    await user.click(skip);
    expect(screen.getByRole('main')).toHaveFocus();
  });
});
