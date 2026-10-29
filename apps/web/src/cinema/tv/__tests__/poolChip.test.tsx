import { render, screen } from '@testing-library/react';
import { PoolChip } from '../PoolChip';
import { toChannel } from '../api';
import { useStore } from '../../../store';

describe('the shelf chip on a channel (BYOS-TV-001)', () => {
  it('says "Server shelf" for a server channel and names the server in the tooltip', () => {
    useStore.setState((s) => ({
      guilds: { ...s.guilds, '900': { id: '900', name: 'Film Club' } as never },
    }));
    render(<PoolChip pool={{ kind: 'guild', guild_id: '900' }} />);
    const chip = screen.getByText('Server shelf');
    expect(chip).toHaveAttribute(
      'title',
      'This channel airs only what Film Club keeps on its shelves.',
    );
  });

  it('copes with a server the client does not know', () => {
    render(<PoolChip pool={{ kind: 'guild', guild_id: '12345' }} />);
    expect(screen.getByText('Server shelf')).toHaveAttribute(
      'title',
      'This channel airs only what this server keeps on its shelves.',
    );
  });

  it('says "Your shelf" for a personal channel and nothing for the instance catalog', () => {
    const { rerender, container } = render(<PoolChip pool={{ kind: 'personal', user_id: '5' }} />);
    expect(screen.getByText('Your shelf')).toBeInTheDocument();
    rerender(<PoolChip pool={{ kind: 'instance' }} />);
    expect(container).toBeEmptyDOMElement();
    rerender(<PoolChip pool={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('keeps the pool the server sent', () => {
    const c = toChannel({
      id: 'c1',
      number: 1,
      name: 'Club',
      kind: 'rule_based',
      timezone: 'UTC',
      age_restricted: false,
      pool: { kind: 'guild', guild_id: '900' },
    });
    expect(c.pool).toEqual({ kind: 'guild', guild_id: '900' });
  });
});
