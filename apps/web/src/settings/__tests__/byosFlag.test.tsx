import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { setClientFlags } from '../../moderation/reports';
import type { SectionProps } from '../server/ServerSettings';
import { ServerStorageSection } from '../server/storage/StorageSection';
import { StorageSection } from '../user/storage/StorageSection';

vi.mock('../user/storage/MountsPanel', () => ({ MountsPanel: () => <p>mounts panel</p> }));

const props = { guildId: '1' };

// BYOS-FLG-001: the platform kill switch hides the storage screens; only an explicit `false` does.
describe('platform.byos on the settings screens', () => {
  afterEach(() => setClientFlags({}));

  it('shows the panels before the flags load and while the feature is on', () => {
    const { unmount } = render(<StorageSection />);
    expect(screen.getByText('mounts panel')).toBeInTheDocument();
    unmount();
    setClientFlags({ 'platform.byos': true });
    render(<ServerStorageSection {...(props as SectionProps)} />);
    expect(screen.getByText('mounts panel')).toBeInTheDocument();
  });

  it('replaces them with a notice when the operator switched it off', () => {
    setClientFlags({ 'platform.byos': false });
    const { unmount } = render(<StorageSection />);
    expect(screen.queryByText('mounts panel')).not.toBeInTheDocument();
    expect(screen.getByText('Bring your own storage is switched off')).toBeInTheDocument();
    unmount();
    render(<ServerStorageSection {...(props as SectionProps)} />);
    expect(screen.queryByText('mounts panel')).not.toBeInTheDocument();
    expect(screen.getByText('Bring your own storage is switched off')).toBeInTheDocument();
  });
});
