import { act, fireEvent, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { at, baseHandlers, seedLounge } from '../../lounge/__tests__/gapsHelpers';
import { closeDrawer } from '../drawer/openState';
import { MAX_GADGETS, EMPTY_LAYOUT } from '../gadgets/layout';
import { useToolsStore } from '../gadgets/store';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

beforeEach(() => {
  baseHandlers();
  seedLounge({ messages: [] });
  closeDrawer();
  useToolsStore.setState({ layout: EMPTY_LAYOUT, limitHit: false });
});

const openWithKeys = async () => {
  await screen.findByRole('navigation', { name: 'Servers' }, { timeout: 30_000 });
  fireEvent.keyDown(document, { key: 'k', ctrlKey: true });
  return screen.findByRole('dialog', { name: 'Drawer' });
};

describe('the utility drawer (UTL-SHL-001)', { timeout: 60_000 }, () => {
  it('opens on Go to with Ctrl+K and still navigates the way the switcher did', async () => {
    const { router } = at('/channels/g1/c1');
    const dialog = await openWithKeys();
    expect(within(dialog).getByRole('tab', { name: 'Go to', selected: true })).toBeTruthy();
    await userEvent.type(within(dialog).getByRole('combobox'), '#rand');
    expect(await screen.findByRole('option', { name: /random/ })).toBeInTheDocument();
    await userEvent.keyboard('{Enter}');
    await vi.waitFor(() => expect(router.state.location.pathname).toBe('/channels/g1/c2'));
    expect(screen.queryByRole('dialog', { name: 'Drawer' })).not.toBeInTheDocument();
  });

  it('switches to the tools when the line starts with > and opens a tool in the drawer', async () => {
    at('/channels/g1/c1');
    const dialog = await openWithKeys();
    await userEvent.type(within(dialog).getByRole('combobox'), '>guide');
    expect(within(dialog).getByRole('tab', { name: 'Tools', selected: true })).toBeTruthy();
    expect(await within(dialog).findByRole('option', { name: /Drawer guide/ })).toBeTruthy();
    await userEvent.keyboard('{Enter}');
    expect(await within(dialog).findByRole('button', { name: 'Pop out' })).toBeTruthy();
    expect(await within(dialog).findByText(/Press Ctrl\+K/)).toBeTruthy();
  });

  it('pops a tool out; Escape then closes the drawer but not the gadget, and it survives navigation', async () => {
    const { router } = at('/channels/g1/c1');
    const dialog = await openWithKeys();
    await userEvent.type(within(dialog).getByRole('combobox'), '>guide');
    await userEvent.keyboard('{Enter}');
    await userEvent.click(await within(dialog).findByRole('button', { name: 'Pop out' }));
    const gadget = await screen.findByRole('dialog', { name: 'Drawer guide' });
    expect(screen.queryByRole('dialog', { name: 'Drawer' })).not.toBeInTheDocument();
    expect(useToolsStore.getState().layout.gadgets.map((g) => g.tool)).toEqual(['guide']);

    // Reopen the drawer and press Escape: only the drawer goes.
    fireEvent.keyDown(document, { key: 'k', ctrlKey: true });
    await screen.findByRole('dialog', { name: 'Drawer' });
    await userEvent.keyboard('{Escape}');
    await vi.waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'Drawer' })).not.toBeInTheDocument(),
    );
    expect(screen.getByRole('dialog', { name: 'Drawer guide' })).toBe(gadget);

    // Navigation keeps it, and the same element stays mounted.
    await act(async () => {
      await router.navigate({ to: '/channels/$guildId/$channelId', params: { guildId: 'g1', channelId: 'c2' } });
    });
    expect(screen.getByRole('dialog', { name: 'Drawer guide' })).toBe(gadget);

    await userEvent.click(within(gadget).getByRole('button', { name: 'Close Drawer guide' }));
    expect(screen.queryByRole('dialog', { name: 'Drawer guide' })).not.toBeInTheDocument();
  });

  it('pins a tool to the rail and removes the pin', async () => {
    at('/channels/g1/c1');
    const dialog = await openWithKeys();
    await userEvent.type(within(dialog).getByRole('combobox'), '>guide');
    await userEvent.keyboard('{Enter}');
    await userEvent.click(await within(dialog).findByRole('button', { name: 'Pin to the rail' }));
    expect(useToolsStore.getState().layout.pins).toEqual(['guide']);
    await userEvent.keyboard('{Escape}');
    const rail = await screen.findByRole('toolbar', { name: 'Pinned tools' });
    await userEvent.click(within(rail).getByRole('button', { name: 'Drawer guide' }));
    const panel = await screen.findByRole('region', { name: 'Drawer guide' });
    await userEvent.click(within(panel).getByRole('button', { name: 'Unpin from the rail' }));
    expect(useToolsStore.getState().layout.pins).toEqual([]);
    expect(screen.queryByRole('toolbar', { name: 'Pinned tools' })).not.toBeInTheDocument();
  });

  it('moves a gadget with the arrow keys and resizes it with Shift', async () => {
    at('/channels/g1/c1');
    await openWithKeys(); // mounts the host
    await userEvent.keyboard('{Escape}');
    act(() => {
      useToolsStore.getState().popOut('guide');
    });
    const gadget = await screen.findByRole('dialog', { name: 'Drawer guide' });
    const before = useToolsStore.getState().layout.gadgets[0]!;
    const handle = within(gadget).getByRole('toolbar');
    handle.focus();
    await userEvent.keyboard('{ArrowDown}');
    expect(useToolsStore.getState().layout.gadgets[0]?.y).toBe(before.y + 16);
    await userEvent.keyboard('{Shift>}{ArrowRight}{/Shift}');
    expect(useToolsStore.getState().layout.gadgets[0]?.w).toBe(before.w + 16);
  });

  it('has no axe violations with the drawer open and with a gadget and the rail showing', async () => {
    at('/channels/g1/c1');
    const dialog = await openWithKeys();
    await userEvent.type(within(dialog).getByRole('combobox'), '>');
    await within(dialog).findByRole('option', { name: /Drawer guide/ });
    expect(await axeViolations(dialog)).toEqual([]);
    await userEvent.keyboard('{Escape}');
    act(() => {
      useToolsStore.getState().popOut('guide');
      useToolsStore.getState().pin('guide');
    });
    await screen.findByRole('toolbar', { name: 'Pinned tools' });
    for (const sel of ['.sr-tl-gadgets', '.sr-tl-rail-wrap'])
      expect(await axeViolations(document.querySelector<HTMLElement>(sel)!)).toEqual([]);
  });
});

describe('the gadget limit in the store', () => {
  it('refuses the seventh pop-out and says so', () => {
    for (let i = 0; i < MAX_GADGETS; i++)
      expect(useToolsStore.getState().popOut(`tool-${i}`)).toBe(true);
    expect(useToolsStore.getState().popOut('tool-extra')).toBe(false);
    expect(useToolsStore.getState().limitHit).toBe(true);
    expect(useToolsStore.getState().layout.gadgets).toHaveLength(MAX_GADGETS);
    // An already open tool is raised, not refused.
    expect(useToolsStore.getState().popOut('tool-0')).toBe(true);
    expect(useToolsStore.getState().limitHit).toBe(false);
  });
});
