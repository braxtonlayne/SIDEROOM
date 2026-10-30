/**
 * UTL-SHL-001: the gadget and pin store. A thin zustand wrapper over the pure functions of `layout.ts`; the
 * account sync (`sync.ts`) watches `layout` and `hydrate`s it from the account copy.
 */
import { create } from 'zustand';
import { getTool, DEFAULT_TOOL_SIZE } from '../registry';
import {
  EMPTY_LAYOUT,
  closeGadget,
  openGadget,
  pinTool,
  raiseGadget,
  setGadgetRect,
  unpinTool,
  type GadgetRect,
  type Layout,
  type Viewport,
} from './layout';

export const viewport = (): Viewport =>
  typeof window === 'undefined'
    ? { w: 1280, h: 800 }
    : { w: window.innerWidth, h: window.innerHeight };

interface ToolsState {
  layout: Layout;
  /** Set when a pop-out was refused at the limit; the drawer shows a note and clears it. */
  limitHit: boolean;
  hydrate: (layout: Layout) => void;
  /** Pops a tool out. Returns false when the limit of open gadgets refused it. */
  popOut: (tool: string) => boolean;
  close: (tool: string) => void;
  raise: (tool: string) => void;
  setRect: (tool: string, rect: GadgetRect) => void;
  pin: (tool: string) => void;
  unpin: (tool: string) => void;
  clearLimit: () => void;
}

export const useToolsStore = create<ToolsState>()((set, get) => ({
  layout: EMPTY_LAYOUT,
  limitHit: false,
  hydrate: (layout) => set({ layout }),
  popOut: (tool) => {
    const entry = getTool(tool);
    const res = openGadget(get().layout, tool, entry?.size ?? DEFAULT_TOOL_SIZE, viewport());
    set({ layout: res.layout, limitHit: !res.ok });
    return res.ok;
  },
  close: (tool) => set({ layout: closeGadget(get().layout, tool) }),
  raise: (tool) => set({ layout: raiseGadget(get().layout, tool) }),
  setRect: (tool, rect) =>
    set({
      layout: setGadgetRect(
        get().layout,
        tool,
        rect,
        viewport(),
        getTool(tool)?.size ?? DEFAULT_TOOL_SIZE,
      ),
    }),
  pin: (tool) => set({ layout: pinTool(get().layout, tool) }),
  unpin: (tool) => set({ layout: unpinTool(get().layout, tool) }),
  clearLimit: () => set({ limitHit: false }),
}));
