import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../test/server';
import { useAgeAnswers } from '../AgeGate';
import { at, baseHandlers, calls, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const channels = {
  cat: { id: 'cat', type: 4, guild_id: 'g1', name: 'Text Channels', position: 0 },
  c1: {
    id: 'c1',
    type: 0,
    guild_id: 'g1',
    name: 'spicy',
    parent_id: 'cat',
    position: 1,
    nsfw: true,
  },
  c2: { id: 'c2', type: 0, guild_id: 'g1', name: 'random', parent_id: 'cat', position: 2 },
};

describe('age-restricted channel interstitial (LNG-CHN-022)', () => {
  beforeEach(() => {
    baseHandlers();
    useAgeAnswers.setState({ answers: {} });
    seedLounge({ messages: [msg('100', { content: 'restricted words' })], channels });
    server.use(
      http.post('*/api/v1/sr/age-gate', async (ctx) => {
        await record('POST')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
    );
  });

  it('asks first, loads nothing, and opens the channel after a yes', async () => {
    const { container } = at('/channels/g1/c1');
    expect(await screen.findByRole('heading', { name: '#spicy is age-restricted' })).toBeVisible();
    expect(screen.queryByRole('log')).not.toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: /Message/ })).not.toBeInTheDocument();
    expect(screen.queryByText('restricted words')).not.toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole('button', { name: 'Yes, I am 18 or older' }));
    expect(await screen.findByRole('log', { name: 'Messages in spicy' })).toBeInTheDocument();
    expect(await screen.findByText('restricted words')).toBeInTheDocument();
    expect(calls.find((c) => c.key === 'POST /sr/age-gate')?.body).toEqual({ over_18: true });
    expect(localStorage.getItem('sr.age.answer:1')).toBe('yes');
  });

  it('does not ask again once answered, and does not gate other channels', async () => {
    localStorage.setItem('sr.age.answer:1', 'yes');
    at('/channels/g1/c1');
    expect(await screen.findByRole('log', { name: 'Messages in spicy' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Yes, I am 18/ })).not.toBeInTheDocument();
  });

  it('records a no, leaves the channel and keeps it blocked', async () => {
    const { router } = at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'No, take me back' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'POST /sr/age-gate')?.body).toEqual({ over_18: false }),
    );
    await waitFor(() => expect(router.state.location.pathname).not.toBe('/channels/g1/c1'));
    expect(localStorage.getItem('sr.age.answer:1')).toBe('no');
    await router.navigate({
      to: '/channels/$guildId/$channelId',
      params: { guildId: 'g1', channelId: 'c1' },
    });
    expect(await screen.findByText('Age-restricted channel')).toBeInTheDocument();
    expect(screen.queryByRole('log', { name: 'Messages in spicy' })).not.toBeInTheDocument();
  });

  it('shows an error and stays on the prompt when saving fails', async () => {
    server.use(
      http.post('*/api/v1/sr/age-gate', () => HttpResponse.json({ message: 'x' }, { status: 500 })),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Yes, I am 18 or older' }));
    expect(await screen.findByText('Could not save your answer. Try again.')).toBeInTheDocument();
    expect(screen.queryByRole('log')).not.toBeInTheDocument();
    expect(localStorage.getItem('sr.age.answer:1')).toBeNull();
  });

  it('does not gate channels that are not restricted', async () => {
    at('/channels/g1/c2');
    expect(await screen.findByRole('log', { name: 'Messages in random' })).toBeInTheDocument();
  });
});
