import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { ApiError } from '../../api/client';
import type { VouchApi, VouchSummary } from '../vouch/api';
import { VouchPanel } from '../vouch/VouchPanel';

const base: VouchSummary = {
  count: 3,
  tier: 'citizen',
  multiplier: 1,
  pillar_min: 50,
  elder_min: 500,
  pillar_multiplier: 1.5,
  elder_multiplier: 3,
  viewer_vouched: false,
  can_vouch: true,
  is_self: false,
};

function fake(first: Partial<VouchSummary> = {}, after: Partial<VouchSummary> = {}) {
  let state: VouchSummary = { ...base, ...first };
  const api: VouchApi & { calls: string[] } = {
    calls: [],
    get: vi.fn(async () => state),
    vouch: vi.fn(async () => {
      api.calls.push('vouch');
      state = {
        ...state,
        count: state.count + 1,
        viewer_vouched: true,
        can_vouch: false,
        ...after,
      };
      return state;
    }),
    revoke: vi.fn(async () => {
      api.calls.push('revoke');
      state = {
        ...state,
        count: state.count - 1,
        viewer_vouched: false,
        can_vouch: true,
        ...after,
      };
    }),
  };
  return api;
}

describe('profile vouch panel (MOD-VCH-001)', () => {
  it('shows the count and tier, and vouches with one click', async () => {
    const api = fake();
    render(<VouchPanel userId="2" api={api} />);
    expect(await screen.findByText('3 members vouch for this person')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Vouch tier: Citizen' })).toHaveAttribute(
      'data-tier',
      'citizen',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Vouch' }));
    expect(api.calls).toEqual(['vouch']);
    expect(await screen.findByText('4 members vouch for this person')).toBeInTheDocument();
    // Now it is mine to withdraw, and the Vouch button is gone.
    expect(screen.getByRole('button', { name: 'Withdraw vouch' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Vouch' })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Withdraw vouch' }));
    expect(api.calls).toEqual(['vouch', 'revoke']);
    expect(await screen.findByRole('button', { name: 'Vouch' })).toBeInTheDocument();
  });

  it('documents the multiplier on the tooltip and names the current tier', async () => {
    const api = fake({ count: 500, tier: 'elder', multiplier: 3 });
    render(<VouchPanel userId="2" api={api} />);
    const badge = await screen.findByRole('button', { name: 'Vouch tier: Elder' });
    act(() => badge.focus());
    const tip = await screen.findByRole('tooltip');
    expect(tip).toHaveTextContent(
      'Citizen: under 50 vouches. Pillar: 50 to 499. Elder: 500 or more.',
    );
    expect(tip).toHaveTextContent('count 1.5x');
    expect(tip).toHaveTextContent('For an Elder they count 3x.');
    expect(tip).toHaveTextContent('This person is a Elder (500): automatic penalties count 3x.');
    expect(tip).toHaveTextContent('Nothing happens to your account.');
    expect(badge).toHaveAttribute('aria-describedby', tip.id);
  });

  it('says a Citizen is counted normally', async () => {
    render(<VouchPanel userId="2" api={fake({ count: 49 })} />);
    const badge = await screen.findByRole('button', { name: 'Vouch tier: Citizen' });
    fireEvent.focus(badge);
    expect(await screen.findByRole('tooltip')).toHaveTextContent('count normally (1x)');
  });

  it('has no vouch button on your own profile', async () => {
    render(<VouchPanel userId="1" api={fake({ is_self: true, can_vouch: false })} />);
    expect(await screen.findByText('3 members vouch for this person')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Vouch' })).toBeNull();
    expect(screen.queryByText('You cannot vouch for this person right now.')).toBeNull();
  });

  it('explains when you cannot vouch', async () => {
    render(<VouchPanel userId="2" api={fake({ can_vouch: false })} />);
    expect(
      await screen.findByText('You cannot vouch for this person right now.'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Vouch' })).toBeNull();
  });

  it('shows the daily limit and other failures', async () => {
    const api = fake();
    (api.vouch as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new ApiError(409, 'slow down'));
    render(<VouchPanel userId="2" api={api} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Vouch' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'vouched for a lot of members today',
    );
    (api.vouch as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('boom'));
    await userEvent.click(screen.getByRole('button', { name: 'Vouch' }));
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Your vouch could not be saved.'),
    );
  });

  it('says so when the vouches cannot be loaded', async () => {
    const api = fake();
    (api.get as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('404'));
    render(<VouchPanel userId="2" api={api} />);
    expect(await screen.findByText('Vouches could not be loaded.')).toBeInTheDocument();
  });

  it('has no accessibility violations', async () => {
    const { container } = render(
      <VouchPanel userId="2" api={fake({ count: 60, tier: 'pillar', multiplier: 1.5 })} />,
    );
    await screen.findByRole('button', { name: 'Vouch tier: Pillar' });
    expect(await axeViolations(container)).toEqual([]);
  });
});
