import { render, screen } from '@testing-library/react';
import { ServerTagChip } from '../components/ServerTagChip';
import { toLoungeMessage, toLoungeUser } from '../liveMap';
import { badgeViews } from '../profile/badgeModel';
import { statsText } from '../profile/useProfile';

const author = { id: '1', username: 'ann' };
const message = (flags: number, a: typeof author = author) => ({
  id: '5',
  channel_id: 'c',
  author: a,
  content: 'hi',
  timestamp: '2026-01-01T00:00:00.000000+00:00',
  flags,
});

// The chip itself is `ServerTagChip` (feat-ui, LNG-SSET-012); these cases cover the G-34 member/author wiring.
describe('server tag on members and authors (G-34)', () => {
  it('maps primary_guild to the chip fields and ignores a disabled identity', () => {
    const u = toLoungeUser({
      ...author,
      primary_guild: { tag: 'sr1', badge: 3, color: 0x00aa33, identity_enabled: true },
    });
    expect(u).toMatchObject({ tag: 'SR1', tagBadge: 3, tagColor: 0x00aa33 });
    expect(
      toLoungeUser({ ...author, primary_guild: { tag: 'sr1', identity_enabled: false } }).tag,
    ).toBeUndefined();
  });

  it('draws the tag with its badge glyph in the tag colour', () => {
    render(
      <ServerTagChip
        user={toLoungeUser({ ...author, primary_guild: { tag: 'sr1', badge: 1, color: 0x00aa33 } })}
      />,
    );
    const glyph = screen.getByText('♥');
    expect(glyph).toHaveStyle({ color: '#00aa33' });
    expect(glyph.parentElement?.textContent).toContain('SR1');
  });

  it('renders nothing without a tag', () => {
    const { container } = render(<ServerTagChip user={{}} />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe('system user messages (G-42)', () => {
  it('marks the author when the SYSTEM_TAG flag is set', () => {
    expect(toLoungeMessage(message(1 << 30)).author.system).toBe(true);
    expect(toLoungeMessage(message(0)).author.system).toBeUndefined();
  });
});

describe('badge catalogue and stats keys (G-53)', () => {
  it('translates known ids and falls back to the server text and icon for unknown ones', () => {
    const [known, unknown] = badgeViews([
      { id: 'bug_hunter', description: 'Bug Hunter (server text)', icon: 'alert' },
      { id: 'brand-new', description: 'Brand new thing', icon: 'star' },
    ]);
    expect(known).toMatchObject({
      kind: 'bug-hunter',
      label: 'Bug Hunter',
      description: 'Bug Hunter',
    });
    expect(unknown).toMatchObject({ kind: 'other', label: 'Brand new thing', icon: 'star' });
  });

  it('writes the five stat keys as translated, pluralised text', () => {
    expect(statsText({ followers: 1, repos: 8, karma: 1200 })).toBe(
      '1 follower · 8 repos · 1,200 karma'.replace('1,200', '1200'),
    );
  });
});
