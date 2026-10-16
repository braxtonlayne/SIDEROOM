import { render, screen } from '@testing-library/react';
import { FeatureSlot, hasFeatureSlot, registerFeatureSlot } from '../sections/featureSlots';
import type { GuildFeature } from '../featuresApi';

const feature = (id: string): GuildFeature =>
  ({
    id,
    name: id,
    description: '',
    section: 'x',
    phase: 1,
    enabled: true,
    platform_disabled: false,
    settings: {},
    schema: {},
  }) as unknown as GuildFeature;

describe('feature custom-UI slots (FND-FEAT-007)', () => {
  it('renders the lazy component registered for a feature id, with guild and feature', async () => {
    registerFeatureSlot('test.panel', async () => ({
      default: ({ guildId, feature: f }) => (
        <p>
          panel of {f.id} in {guildId}
        </p>
      ),
    }));
    expect(hasFeatureSlot('test.panel')).toBe(true);
    render(<FeatureSlot guildId="g1" feature={feature('test.panel')} />);
    expect(await screen.findByText('panel of test.panel in g1')).toBeInTheDocument();
  });

  it('renders nothing for a feature without a panel', () => {
    expect(hasFeatureSlot('test.none')).toBe(false);
    const { container } = render(<FeatureSlot guildId="g1" feature={feature('test.none')} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('ships the panels of the built-in action modules', () => {
    for (const id of ['moderation.temp_roles', 'community.rss', 'lounge.slowmode_plus']) {
      expect(hasFeatureSlot(id), id).toBe(true);
    }
  });

  it('a later registration replaces the earlier one', async () => {
    registerFeatureSlot('test.swap', async () => ({ default: () => <p>first</p> }));
    registerFeatureSlot('test.swap', async () => ({ default: () => <p>second</p> }));
    render(<FeatureSlot guildId="g1" feature={feature('test.swap')} />);
    expect(await screen.findByText('second')).toBeInTheDocument();
    expect(screen.queryByText('first')).not.toBeInTheDocument();
  });
});
