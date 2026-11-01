// ARC-SBX-001: the build badge and the vouch control. The badge flip itself is decided by the server (Rust tests
// in crates/sr-activities); here the web side turns each state into the words the owner specified.
import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { badgeModel, badgeModels, badgeTag, vouchState } from '../trust/badge';
import { BuildBadges, BuildTrust } from '../trust/BuildBadge';
import { useBuildTrust } from '../trust/useBuildTrust';
import type { TrustApi } from '../trust/api';
import type { TrustView } from '../trust/types';
import { ThemeProvider } from '@sideroom/ui';

const H1 = '1a2b3c4d'.padEnd(64, '0');
const H2 = '9f8e7d6c'.padEnd(64, '0');

const base: TrustView = {
  application_id: '7',
  code_sha256: H1,
  headline: 'unsigned',
  badges: ['unsigned'],
  signature: null,
  featured: false,
  vouches: 0,
  vouched_by_me: false,
  can_vouch: true,
  previous: null,
};
const signed: TrustView = {
  ...base,
  headline: 'swarm_safety_signed',
  badges: ['swarm_safety_signed'],
};
const update: TrustView = {
  ...base,
  code_sha256: H2,
  headline: 'unverified_update',
  badges: ['unverified_update'],
  previous: { code_sha256: H1, short: '1a2b3c4d', vouches: 12, signed: true },
};

function wrap(ui: React.ReactElement) {
  return render(<ThemeProvider section="lounge">{ui}</ThemeProvider>);
}

describe('badge models', () => {
  it('tags only the signed tiers', () => {
    expect(badgeTag('swarm_safety_signed')).toBe('[SWARM-SAFETY-SIGNED]');
    expect(badgeTag('operator_featured')).toBe('[OPERATOR-FEATURED]');
    expect(badgeTag('unverified_update')).toBeNull();
    expect(badgeTag('unsigned')).toBeNull();
  });

  it('flips from signed to an unverified update when the hash changes', () => {
    expect(badgeModel(signed.headline, signed)).toMatchObject({ tone: 'signed' });
    const m = badgeModel(update.headline, update);
    expect(m.tone).toBe('warning');
    expect(m.labelValues).toEqual({ n: 12, previous: '1a2b3c4d', current: 0 });
  });

  it('shows both tags for a featured, signed build', () => {
    const v: TrustView = {
      ...signed,
      featured: true,
      headline: 'operator_featured',
      badges: ['operator_featured', 'swarm_safety_signed'],
    };
    expect(badgeModels(v).map((m) => m.kind)).toEqual(['operator_featured', 'swarm_safety_signed']);
  });

  it('decides the vouch control from the viewer and the build', () => {
    expect(vouchState(base)).toBe('available');
    expect(vouchState({ ...base, vouched_by_me: true })).toBe('vouched');
    expect(vouchState({ ...base, can_vouch: false })).toBe('not-eligible');
    expect(vouchState({ ...base, code_sha256: null })).toBe('no-build');
  });
});

describe('BuildBadges', () => {
  it('renders the owner-specified texts', () => {
    const { rerender } = wrap(<BuildBadges trust={signed} />);
    expect(screen.getByText('SWARM-SAFETY-SIGNED')).toBeInTheDocument();
    rerender(
      <ThemeProvider section="lounge">
        <BuildBadges trust={update} />
      </ThemeProvider>,
    );
    expect(
      screen.getByText(
        '⚠️ UNVERIFIED UPDATE (12 users vouched for previous build 1a2b3c4d; 0 vouches on current hash)',
      ),
    ).toBeInTheDocument();
  });
});

function fakeApi(first: TrustView): TrustApi & { vouch: ReturnType<typeof vi.fn> } {
  return {
    get: vi.fn(async () => first),
    vouch: vi.fn(async (_id: string, hash: string) => ({
      ...first,
      vouches: first.vouches + 1,
      vouched_by_me: true,
      code_sha256: hash,
    })),
    unvouch: vi.fn(async () => ({ ...first, vouched_by_me: false })),
  } as TrustApi & { vouch: ReturnType<typeof vi.fn> };
}

function Harness({ api }: { api: TrustApi }) {
  const state = useBuildTrust('7', api);
  return <BuildTrust state={state} />;
}

describe('Vouch for Build', () => {
  it('vouches pinned to the hash it displayed', async () => {
    const api = fakeApi(update);
    wrap(<Harness api={api} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Vouch for Build' }));
    expect(api.vouch).toHaveBeenCalledWith('7', H2);
    expect(await screen.findByRole('button', { name: 'Withdraw vouch' })).toBeInTheDocument();
  });

  it('shows no button to a member who cannot vouch', async () => {
    wrap(<Harness api={fakeApi({ ...base, can_vouch: false })} />);
    expect(
      await screen.findByText('Only members in good standing can vouch for a build.'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Vouch for Build' })).toBeNull();
  });

  it('reloads and says so when the vouch is refused (the game was updated)', async () => {
    const api = fakeApi(base);
    api.vouch.mockRejectedValueOnce(new Error('409'));
    wrap(<Harness api={api} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Vouch for Build' }));
    await waitFor(() => expect(api.get).toHaveBeenCalledTimes(2));
  });
});
