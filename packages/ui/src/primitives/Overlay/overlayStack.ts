import { useCallback, useEffect, useRef, type RefObject } from "react";

/** Q77: one stack for Modal, Drawer and Menu. Only the topmost overlay handles Escape, Tab and scroll lock. */
export const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

interface StackEntry {
  id: symbol;
  lock: boolean;
}

const stack: StackEntry[] = [];
let savedOverflow: string | null = null;

function push(entry: StackEntry): void {
  const wasLocked = stack.some((e) => e.lock);
  stack.push(entry);
  if (entry.lock && !wasLocked) {
    savedOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
  }
}

function pop(id: symbol): void {
  const i = stack.findIndex((e) => e.id === id);
  if (i < 0) return;
  const [removed] = stack.splice(i, 1);
  if (removed?.lock && !stack.some((e) => e.lock)) {
    document.body.style.overflow = savedOverflow ?? "";
    savedOverflow = null;
  }
}

export interface OverlayStackOptions {
  open: boolean;
  /** Lock body scroll while this overlay is open (modals and drawers, not menus). */
  lockScroll?: boolean;
  /** Called on Escape while this overlay is topmost. Omit when the overlay handles Escape itself. */
  onEscape?: () => void;
}

export interface OverlayStackHandle {
  /** True when this overlay is the topmost open overlay. */
  isTopmost: () => boolean;
}

/** Registers an open overlay on the shared stack and routes Escape to the topmost one only. */
export function useOverlayStack({ open, lockScroll = false, onEscape }: OverlayStackOptions): OverlayStackHandle {
  const id = useRef<symbol>(Symbol("overlay"));
  const escRef = useRef(onEscape);
  escRef.current = onEscape;
  const isTopmost = useCallback(() => stack[stack.length - 1]?.id === id.current, []);

  useEffect(() => {
    if (!open) return;
    const entryId = id.current;
    push({ id: entryId, lock: lockScroll });
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !escRef.current || !isTopmost()) return;
      e.stopPropagation();
      escRef.current();
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      pop(entryId);
    };
  }, [open, lockScroll, isTopmost]);

  return { isTopmost };
}

export interface FocusTrapOptions {
  open: boolean;
  containerRef: RefObject<HTMLElement | null>;
  isTopmost: () => boolean;
  /** Element to focus on open; defaults to `[data-autofocus]`, the first focusable, else the container. */
  initialFocusRef?: RefObject<HTMLElement | null>;
}

/** Moves focus into an overlay on open, wraps Tab inside it, and restores focus on close. */
export function useFocusTrap({ open, containerRef, isTopmost, initialFocusRef }: FocusTrapOptions): void {
  useEffect(() => {
    if (!open) return;
    const container = containerRef.current;
    const previouslyFocused = document.activeElement as HTMLElement | null;
    const target =
      initialFocusRef?.current ??
      // One query per priority: a selector list returns the first match in document order, so the header's close
      // button would beat a later `[data-autofocus]` field.
      container?.querySelector<HTMLElement>("[data-autofocus]") ??
      container?.querySelector<HTMLElement>(FOCUSABLE) ??
      container;
    target?.focus();

    const onKey = (e: KeyboardEvent) => {
      const el = containerRef.current;
      if (e.key !== "Tab" || !el || !isTopmost()) return;
      const items = Array.from(el.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (items.length === 0) {
        e.preventDefault();
        el.focus();
        return;
      }
      const first = items[0]!;
      const last = items[items.length - 1]!;
      const active = document.activeElement;
      if (e.shiftKey && (active === first || active === el)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      } else if (!el.contains(active)) {
        e.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      previouslyFocused?.focus?.();
    };
    // initialFocusRef is read once on open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
}
