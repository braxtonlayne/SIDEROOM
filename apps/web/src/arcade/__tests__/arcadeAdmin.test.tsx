// FU-3 (FU-ARC-4): the Operator's Release button tells the watchers; Remove stays silent.
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ArcadeAdminApi } from '../adminApi';
import type { ArcadeApi } from '../api';
import { ArcadeAdminSection } from '../ArcadeAdmin';
import type { ComingSoonCard } from '../types';

const entry: ComingSoonCard = {
  id: '7',
  title: 'OpenMW',
  summary: 'A clean-room engine.',
  tier: 'TIER_2_REIMPLEMENTATION',
  repo_url: 'https://github.com/OpenMW/openmw',
  notify: false,
  created_at_ms: 1,
};

function rig() {
  const admin = {
    candidates: vi.fn().mockResolvedValue([
      {
        application_id: '42',
        name: 'OpenMW Web',
        tier: 'TIER_2_REIMPLEMENTATION',
        bundle_sha256: 'ab',
        featured: false,
      },
    ]),
    feature: vi.fn(),
    unfeature: vi.fn(),
    addComingSoon: vi.fn(),
    removeComingSoon: vi.fn().mockResolvedValue(undefined),
    releaseComingSoon: vi.fn().mockResolvedValue({ watchers: 3, notified: 2 }),
  } satisfies ArcadeAdminApi;
  const api = {
    front: vi.fn().mockResolvedValue({ featured: [], coming_soon: [entry], flags: [] }),
  } as unknown as Pick<ArcadeApi, 'front'>;
  return { admin, api };
}

describe('Arcade admin: Coming Soon release', () => {
  it('releases an entry and says how many watchers were told', async () => {
    const { admin, api } = rig();
    render(<ArcadeAdminSection admin={admin} api={api} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Release OpenMW' }));
    expect(admin.releaseComingSoon).toHaveBeenCalledWith('7');
    expect(vi.mocked(admin.releaseComingSoon).mock.calls[0]).toHaveLength(1);
    expect(admin.removeComingSoon).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByText('OpenMW is released. 2 of 3 watchers were told.')).toBeTruthy(),
    );
  });

  it('releases as the chosen Activity so the message names it', async () => {
    const { admin, api } = rig();
    render(<ArcadeAdminSection admin={admin} api={api} />);
    const pick = await screen.findByRole('combobox', { name: 'Released as' });
    await userEvent.click(pick);
    await userEvent.click(await screen.findByRole('option', { name: /OpenMW Web/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Release OpenMW' }));
    expect(admin.releaseComingSoon).toHaveBeenCalledWith('7', { application_id: '42' });
  });

  it('shows the tier of a Coming Soon entry beside its title', async () => {
    const { admin, api } = rig();
    render(<ArcadeAdminSection admin={admin} api={api} />);
    const item = (await screen.findByText('OpenMW')).closest('li');
    expect(item?.textContent).toContain('Clean-room reimplementation');
  });

  it('keeps Remove as the silent removal', async () => {
    const { admin, api } = rig();
    render(<ArcadeAdminSection admin={admin} api={api} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Remove OpenMW' }));
    expect(admin.removeComingSoon).toHaveBeenCalledWith('7');
    expect(admin.releaseComingSoon).not.toHaveBeenCalled();
  });

  it('finds each select by its visible label', async () => {
    const { admin, api } = rig();
    render(<ArcadeAdminSection admin={admin} api={api} />);
    expect(await screen.findByRole('combobox', { name: 'Released as' })).toBeTruthy();
    expect(screen.getByRole('combobox', { name: 'Content tier' }).textContent).toContain(
      'Original',
    );
    expect(screen.getByRole('heading', { level: 1, name: 'Arcade' })).toBeTruthy();
  });
});
