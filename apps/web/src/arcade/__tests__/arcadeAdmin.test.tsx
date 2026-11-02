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
    candidates: vi.fn().mockResolvedValue([]),
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
    expect(admin.removeComingSoon).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByText('OpenMW is released. 2 of 3 watchers were told.')).toBeTruthy(),
    );
  });

  it('keeps Remove as the silent removal', async () => {
    const { admin, api } = rig();
    render(<ArcadeAdminSection admin={admin} api={api} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Remove OpenMW' }));
    expect(admin.removeComingSoon).toHaveBeenCalledWith('7');
    expect(admin.releaseComingSoon).not.toHaveBeenCalled();
  });
});
