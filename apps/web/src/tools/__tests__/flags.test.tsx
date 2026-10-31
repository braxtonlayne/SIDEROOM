// OPS-FLG-001: the operator's switches hide the tools (platform.tools and one flag per tool) and the emulator.
import { act, render, renderHook, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { EmulatorSwitchedOff, useEmulatorOn } from '../../emu/SwitchedOff';
import { setClientFlags } from '../../moderation/reports';
import { toolAllowed, toolFlagId, useAllowedTools, useToolAllowed } from '../flags';
import { TOOLS } from '../registry';

afterEach(() => act(() => setClientFlags({})));

describe('tool flags', () => {
  it('names the sub-flag of a tool with underscores for hyphens', () => {
    expect(toolFlagId('json')).toBe('platform.tools.json');
    expect(toolFlagId('image-studio')).toBe('platform.tools.image_studio');
    expect(toolFlagId('md-table')).toBe('platform.tools.md_table');
  });

  it('shows everything until a flag says false (flags that have not loaded hide nothing)', () => {
    expect(TOOLS.every((t) => toolAllowed({}, t.id))).toBe(true);
    const { result } = renderHook(() => useAllowedTools());
    expect(result.current).toHaveLength(TOOLS.length);
  });

  it('hides one tool with its own flag and all of them with platform.tools', () => {
    const { result } = renderHook(() => ({
      all: useAllowedTools(),
      dev: useToolAllowed('md-table'),
      guide: useToolAllowed('guide'),
    }));
    act(() => setClientFlags({ 'platform.tools.md_table': false }));
    expect(result.current.dev).toBe(false);
    expect(result.current.all.map((t) => t.id)).not.toContain('md-table');
    expect(result.current.all).toHaveLength(TOOLS.length - 1);
    expect(result.current.guide).toBe(true);
    act(() => setClientFlags({ 'platform.tools': false }));
    expect(result.current.all).toHaveLength(0);
    expect(result.current.guide).toBe(false);
    // The sub-flags being on does not override the parent.
    act(() => setClientFlags({ 'platform.tools': false, 'platform.tools.json': true }));
    expect(toolAllowed({ 'platform.tools': false, 'platform.tools.json': true }, 'json')).toBe(
      false,
    );
  });
});

describe('emulator flag', () => {
  it('is on until platform.emulator is false, and the off page explains it', () => {
    const { result } = renderHook(() => useEmulatorOn());
    expect(result.current).toBe(true);
    act(() => setClientFlags({ 'platform.emulator': false }));
    expect(result.current).toBe(false);
    render(<EmulatorSwitchedOff />);
    expect(screen.getByText('The emulator is switched off')).toBeInTheDocument();
  });
});
