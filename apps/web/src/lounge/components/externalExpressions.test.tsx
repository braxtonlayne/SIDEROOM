// PAR-FU-003 (D-375, D-376): the pickers show other servers' expressions in their own sections, grey them
// out where the channel refuses them, omit servers whose roles forbid it, and refresh on gateway events.
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { emitDispatch } from '../../store/events';
import { http, HttpResponse, server } from '../../test/server';
import { overwriteApplies } from '../../settings/server/sections/Channels';
import { PERMISSIONS } from '../../settings/server/permissions';
import { EmojiPicker, resetGuildEmojis } from './EmojiPicker';
import { resetGuildStickers, StickerPicker } from './ExpressionPicker';
import { invalidateMyExpressions, resetMyExpressions } from './myExpressions';

const EMOJI = (id: string, name: string, extra: object = {}) => ({
  id,
  name,
  animated: false,
  available: true,
  roles: [],
  ...extra,
});

const MINE = [
  {
    guild: { id: 'g1', name: 'Home', icon: null },
    usable_externally: true,
    emojis: [EMOJI('1', 'homey')],
    stickers: [],
    sounds: [],
  },
  {
    guild: { id: 'g2', name: 'Friends', icon: null },
    usable_externally: true,
    emojis: [EMOJI('20', 'pog'), EMOJI('21', 'gone', { available: false })],
    stickers: [{ id: '70', name: 'wave', tags: 'hi', format_type: 1, available: true }],
    sounds: [],
  },
  {
    guild: { id: 'g3', name: 'Locked', icon: null },
    usable_externally: false,
    emojis: [EMOJI('30', 'secret')],
    stickers: [{ id: '71', name: 'hidden', tags: 'x', format_type: 1, available: true }],
    sounds: [],
  },
];

function handlers(list: object = MINE) {
  server.use(
    http.get('*/api/v1/users/@me/expressions', () => HttpResponse.json(list)),
    http.get('*/api/v1/guilds/g1/emojis', () => HttpResponse.json([EMOJI('1', 'homey')])),
    http.get('*/api/v1/guilds/:g/emojis/:id/url', () =>
      HttpResponse.json({ url: '/cdn/e.png', expires_at: 1 }),
    ),
    http.get('*/api/v1/guilds/g1/stickers', () => HttpResponse.json([])),
    http.get('*/api/v1/guilds/g1/default-sticker-packs', () => HttpResponse.json({ hidden: true })),
    http.get('*/api/v1/guilds/:g/stickers/:id/url', () =>
      HttpResponse.json({ url: '/cdn/s.png', expires_at: 1 }),
    ),
  );
}

beforeEach(() => {
  localStorage.clear();
  resetGuildEmojis();
  resetGuildStickers();
  resetMyExpressions();
});

describe('role editor and overwrites', () => {
  it('lists bit 60 in the SIDE ROOM group of the role editor and keeps it out of overwrites', () => {
    const info = PERMISSIONS.find((p) => p.id === 'USE_EXPRESSIONS_EXTERNALLY');
    expect(info).toMatchObject({ bit: 60, group: 'sideroom', scope: 'guild' });
    expect(info?.label).toBe('Use Expressions Externally');
    const text = { id: 'c', type: 0 } as Parameters<typeof overwriteApplies>[1];
    const voice = { id: 'v', type: 2 } as Parameters<typeof overwriteApplies>[1];
    expect(overwriteApplies('USE_EXPRESSIONS_EXTERNALLY', text)).toBe(false);
    expect(overwriteApplies('USE_EXPRESSIONS_EXTERNALLY', voice)).toBe(false);
  });
});

describe('EmojiPicker: other servers', () => {
  it('adds one section per usable server and omits servers whose roles forbid it', async () => {
    handlers();
    const onCustom = vi.fn();
    const user = userEvent.setup();
    render(<EmojiPicker onPick={vi.fn()} guildId="g1" onPickCustom={onCustom} />);
    const friends = await screen.findByRole('region', { name: 'Friends' });
    expect(within(friends).getByRole('button', { name: 'pog' })).toBeInTheDocument();
    // Unavailable emoji and the server that forbids external use are not offered.
    expect(within(friends).queryByRole('button', { name: 'gone' })).toBeNull();
    expect(screen.queryByRole('region', { name: 'Locked' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'secret' })).toBeNull();
    // The current server stays its own section, and is not repeated as an external one.
    expect(screen.getAllByRole('button', { name: 'homey' })).toHaveLength(1);
    // The category bar gets a button for the section.
    const bar = screen.getByRole('toolbar', { name: 'Emoji categories' });
    expect(within(bar).getByRole('button', { name: 'Friends' })).toBeInTheDocument();
    await user.click(within(friends).getByRole('button', { name: 'pog' }));
    expect(onCustom).toHaveBeenCalledWith(expect.objectContaining({ id: '20', name: 'pog' }));
  });

  it('greys the sections out with the reason when the channel refuses other servers', async () => {
    handlers();
    const onCustom = vi.fn();
    const user = userEvent.setup();
    render(
      <EmojiPicker onPick={vi.fn()} guildId="g1" onPickCustom={onCustom} externalAllowed={false} />,
    );
    const friends = await screen.findByRole('region', { name: 'Friends' });
    const tile = within(friends).getByRole('button', { name: 'pog' });
    expect(tile).toHaveAttribute('aria-disabled', 'true');
    expect(friends.querySelector('[title]')).toHaveAttribute(
      'title',
      'This channel does not allow emoji from other servers',
    );
    await user.click(tile);
    expect(onCustom).not.toHaveBeenCalled();
    // This server's own emoji are still usable.
    await user.click(await screen.findByRole('button', { name: 'homey' }));
    expect(onCustom).toHaveBeenCalledWith(expect.objectContaining({ id: '1' }));
  });

  it('offers other servers emoji in a DM (no current server)', async () => {
    handlers();
    render(<EmojiPicker onPick={vi.fn()} onPickCustom={vi.fn()} />);
    expect(await screen.findByRole('region', { name: 'Home' })).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Friends' })).toBeInTheDocument();
  });

  it('shows no custom sections without a custom handler (reaction picker)', async () => {
    handlers();
    render(<EmojiPicker onPick={vi.fn()} />);
    await waitFor(() => expect(screen.getByLabelText('Search emoji')).toBeInTheDocument());
    expect(screen.queryByRole('region', { name: 'Friends' })).toBeNull();
  });

  it('fetches once per session and again after a gateway event changes the list', async () => {
    let calls = 0;
    server.use(
      http.get('*/api/v1/users/@me/expressions', () => {
        calls += 1;
        return HttpResponse.json(calls === 1 ? [] : MINE);
      }),
      http.get('*/api/v1/guilds/g1/emojis', () => HttpResponse.json([])),
      http.get('*/api/v1/guilds/:g/emojis/:id/url', () =>
        HttpResponse.json({ url: '/cdn/e.png', expires_at: 1 }),
      ),
    );
    const { unmount } = render(
      <EmojiPicker onPick={vi.fn()} guildId="g1" onPickCustom={vi.fn()} />,
    );
    await waitFor(() => expect(calls).toBe(1));
    unmount();
    render(<EmojiPicker onPick={vi.fn()} guildId="g1" onPickCustom={vi.fn()} />);
    await waitFor(() => expect(screen.getByLabelText('Search emoji')).toBeInTheDocument());
    expect(calls).toBe(1);
    await act(async () => emitDispatch('GUILD_ROLE_UPDATE', { guild_id: 'g2' }));
    expect(await screen.findByRole('region', { name: 'Friends' })).toBeInTheDocument();
    expect(calls).toBe(2);
    invalidateMyExpressions();
    await waitFor(() => expect(calls).toBe(3));
  });

  it('has no axe violations with the external sections', async () => {
    handlers();
    const { container } = render(
      <EmojiPicker onPick={vi.fn()} guildId="g1" onPickCustom={vi.fn()} externalAllowed={false} />,
    );
    await screen.findByRole('region', { name: 'Friends' });
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe('StickerPicker: other servers', () => {
  it('shows usable servers stickers, greyed with the reason when the channel refuses them', async () => {
    handlers();
    const onPick = vi.fn();
    const user = userEvent.setup();
    const { rerender } = render(<StickerPicker guildId="g1" onPick={onPick} />);
    const friends = await screen.findByRole('region', { name: 'Friends' });
    expect(screen.queryByRole('region', { name: 'Locked' })).toBeNull();
    await user.click(within(friends).getByRole('button', { name: 'Send sticker wave' }));
    expect(onPick).toHaveBeenCalledWith(expect.objectContaining({ id: '70' }));
    onPick.mockClear();
    rerender(<StickerPicker guildId="g1" onPick={onPick} externalAllowed={false} />);
    const greyed = await screen.findByRole('region', { name: 'Friends' });
    const tile = within(greyed).getByRole('button', { name: 'Send sticker wave' });
    expect(tile).toHaveAttribute('aria-disabled', 'true');
    expect(greyed.querySelector('[title]')).toHaveAttribute(
      'title',
      'This channel does not allow stickers from other servers',
    );
    await user.click(tile);
    expect(onPick).not.toHaveBeenCalled();
  });

  it('works in a DM when another server offers stickers', async () => {
    handlers();
    render(<StickerPicker guildId={undefined} onPick={vi.fn()} />);
    expect(await screen.findByRole('region', { name: 'Friends' })).toBeInTheDocument();
  });
});
