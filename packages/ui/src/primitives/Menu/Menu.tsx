import {
  cloneElement,
  Fragment,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactElement,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { usePortalTarget } from "../../theme/ThemeProvider";
import type { FloatingPlacement } from "../../util/position";
import { cx } from "../../util/cx";
import { useOverlayStack } from "../Overlay/overlayStack";
import { useFloating, type FloatingAnchor } from "../Overlay/useFloating";
import styles from "./Menu.module.css";

/** Q60: a submenu opens after the pointer rests on its item this long (no safe triangle). */
export const SUBMENU_HOVER_INTENT_MS = 150;
/** Q60: touch long-press opens a ContextMenu after this long. */
export const LONG_PRESS_MS = 500;
/** A touch that moves further than this before the timer fires is a scroll, not a press. */
const LONG_PRESS_SLOP_PX = 10;

/* ------------------------------------------------------------------ model */

interface ItemBase {
  label: ReactNode;
  icon?: ReactNode;
  /** Right-aligned hint, e.g. "Ctrl+K". Purely visual; wire the real shortcut yourself. */
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  /** Keep the menu open after selecting. Default: close. */
  keepOpen?: boolean;
}
export interface MenuActionItem extends ItemBase {
  type?: "item";
  onSelect?: () => void;
}
export interface MenuCheckboxItem extends ItemBase {
  type: "checkbox";
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}
export interface MenuRadioGroup {
  type: "radio-group";
  /** Optional visible + accessible group heading. */
  label?: string;
  value: string;
  onValueChange: (value: string) => void;
  options: Array<{ value: string; label: ReactNode; icon?: ReactNode; disabled?: boolean; keepOpen?: boolean }>;
}
export interface MenuSeparator {
  type: "separator";
}
export interface MenuLabel {
  type: "label";
  label: string;
}
export interface MenuSubmenu extends Omit<ItemBase, "keepOpen" | "shortcut"> {
  type: "submenu";
  items: MenuEntry[];
}
export type MenuEntry =
  | MenuActionItem
  | MenuCheckboxItem
  | MenuRadioGroup
  | MenuSeparator
  | MenuLabel
  | MenuSubmenu;

/* ------------------------------------------------------------------ panel */

const ITEM_SELECTOR = '[role="menuitem"],[role="menuitemcheckbox"],[role="menuitemradio"]';

function enabledItems(panel: HTMLElement): HTMLElement[] {
  return Array.from(panel.querySelectorAll<HTMLElement>(ITEM_SELECTOR)).filter(
    (el) => el.getAttribute("aria-disabled") !== "true" && el.closest('[role="menu"]') === panel,
  );
}

interface PanelProps {
  entries: MenuEntry[];
  label: string;
  rootId: string;
  anchor: FloatingAnchor;
  placement: FloatingPlacement;
  /** Move focus to the first item on mount (keyboard / click opened). */
  focusFirst: boolean;
  /** Close the whole menu tree. */
  onCloseAll: () => void;
  /** Present on submenus: close just this level and refocus the parent item. */
  onCloseSelf?: () => void;
  id?: string;
}

function MenuPanel({ entries, label, rootId, anchor, placement, focusFirst, onCloseAll, onCloseSelf, id }: PanelProps) {
  const portalTarget = usePortalTarget();
  const floating = useFloating({ open: true, anchor, placement, offset: onCloseSelf ? 2 : 4 });
  const panelRef = useRef<HTMLDivElement | null>(null);
  const [openSub, setOpenSub] = useState<{ index: number; anchor: HTMLElement; focus: boolean } | null>(null);
  const typed = useRef({ buffer: "", timer: undefined as ReturnType<typeof setTimeout> | undefined });
  const intent = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const groupId = useId();

  const setPanel = useCallback(
    (el: HTMLDivElement | null) => {
      panelRef.current = el;
      floating.ref(el);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [floating.ref],
  );

  useLayoutEffect(() => {
    const p = panelRef.current;
    if (!p) return;
    if (focusFirst) (enabledItems(p)[0] ?? p).focus({ preventScroll: true });
    else p.focus({ preventScroll: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [portalTarget]);

  useEffect(() => {
    const t = typed.current;
    return () => {
      clearTimeout(t.timer);
      clearTimeout(intent.current);
    };
  }, []);

  if (!portalTarget) return null;

  const move = (to: "next" | "prev" | "first" | "last") => {
    const p = panelRef.current;
    if (!p) return;
    const items = enabledItems(p);
    if (!items.length) return;
    const cur = items.indexOf(document.activeElement as HTMLElement);
    let i: number;
    if (to === "first") i = 0;
    else if (to === "last") i = items.length - 1;
    else if (to === "next") i = cur < 0 ? 0 : (cur + 1) % items.length;
    else i = cur < 0 ? items.length - 1 : (cur - 1 + items.length) % items.length;
    items[i]?.focus();
  };

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    // Events from a submenu (a React-tree child living in another DOM subtree) are handled there.
    if (!e.currentTarget.contains(e.target as Node)) return;
    const active = document.activeElement as HTMLElement | null;
    const handled = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    switch (e.key) {
      case "ArrowDown":
        handled();
        move("next");
        return;
      case "ArrowUp":
        handled();
        move("prev");
        return;
      case "Home":
        handled();
        move("first");
        return;
      case "End":
        handled();
        move("last");
        return;
      case "ArrowRight":
        if (active?.getAttribute("aria-haspopup") === "menu" && active.getAttribute("aria-disabled") !== "true") {
          handled();
          setOpenSub({ index: Number(active.dataset.index), anchor: active, focus: true });
        }
        return;
      case "ArrowLeft":
        if (onCloseSelf) {
          handled();
          onCloseSelf();
        }
        return;
      case "Escape":
        handled();
        if (onCloseSelf) onCloseSelf();
        else onCloseAll();
        return;
      case "Tab":
        handled();
        onCloseAll();
        return;
      case "Enter":
      case " ":
        if (active && active.matches(ITEM_SELECTOR)) {
          handled();
          active.click();
        }
        return;
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
          const t = typed.current;
          clearTimeout(t.timer);
          t.buffer += e.key.toLowerCase();
          t.timer = setTimeout(() => (t.buffer = ""), 600);
          const items = enabledItems(panelRef.current as HTMLElement);
          const start = Math.max(0, items.indexOf(active as HTMLElement));
          const order = [...items.slice(start + (t.buffer.length === 1 ? 1 : 0)), ...items.slice(0, start + 1)];
          const hit = order.find((el) => (el.textContent ?? "").trim().toLowerCase().startsWith(t.buffer));
          if (hit) {
            handled();
            hit.focus();
          }
        }
    }
  };

  const hoverFocus = (e: ReactMouseEvent<HTMLElement>, index: number, sub: boolean) => {
    const el = e.currentTarget;
    if (el.getAttribute("aria-disabled") === "true") return;
    if (document.activeElement !== el) el.focus({ preventScroll: true });
    clearTimeout(intent.current);
    if (sub) {
      // Hover intent: the submenu opens only if the pointer rests on this item.
      intent.current = setTimeout(
        () => setOpenSub((cur) => (cur?.index === index ? cur : { index, anchor: el, focus: false })),
        SUBMENU_HOVER_INTENT_MS,
      );
    } else {
      setOpenSub(null);
    }
  };

  const finish = (keepOpen?: boolean) => {
    if (!keepOpen) onCloseAll();
  };

  const renderItem = (
    key: string,
    index: number,
    role: "menuitem" | "menuitemcheckbox" | "menuitemradio",
    item: { label: ReactNode; icon?: ReactNode; shortcut?: string; disabled?: boolean; danger?: boolean },
    extra: { checked?: boolean; onActivate: () => void; sub?: boolean },
  ) => (
    // eslint-disable-next-line jsx-a11y/click-events-have-key-events -- keys are handled by the menu container
    <div
      key={key}
      role={role}
      tabIndex={-1}
      data-index={index}
      aria-disabled={item.disabled || undefined}
      aria-checked={role === "menuitem" ? undefined : !!extra.checked}
      aria-haspopup={extra.sub ? "menu" : undefined}
      aria-expanded={extra.sub ? openSub?.index === index : undefined}
      data-danger={item.danger || undefined}
      className={styles.item}
      onMouseDown={(e) => e.preventDefault()}
      onMouseEnter={(e) => hoverFocus(e, index, !!extra.sub)}
      onClick={(e) => {
        if (item.disabled) return;
        if (extra.sub) {
          setOpenSub({ index, anchor: e.currentTarget, focus: true });
          return;
        }
        extra.onActivate();
      }}
    >
      <span className={styles.indicator} aria-hidden="true">
        {role !== "menuitem" ? (extra.checked ? (role === "menuitemradio" ? "●" : "✓") : "") : item.icon}
      </span>
      {role !== "menuitem" && item.icon && <span className={styles.icon}>{item.icon}</span>}
      <span className={styles.text}>{item.label}</span>
      {item.shortcut && <kbd className={styles.shortcut}>{item.shortcut}</kbd>}
      {extra.sub && (
        <span className={styles.chevron} aria-hidden="true">
          ›
        </span>
      )}
    </div>
  );

  let nextIndex = 0;
  const body = entries.map((entry, i) => {
    const type = entry.type ?? "item";
    switch (type) {
      case "separator":
        return <div key={i} role="separator" className={styles.separator} />;
      case "label":
        return (
          <div key={i} role="presentation" className={styles.label}>
            {(entry as MenuLabel).label}
          </div>
        );
      case "radio-group": {
        const g = entry as MenuRadioGroup;
        const gid = `${groupId}-${i}`;
        return (
          <div key={i} role="group" aria-labelledby={g.label ? gid : undefined}>
            {g.label && (
              <div id={gid} role="presentation" className={styles.label}>
                {g.label}
              </div>
            )}
            {g.options.map((o) => {
              const idx = nextIndex++;
              return renderItem(`${i}-${o.value}`, idx, "menuitemradio", o, {
                checked: g.value === o.value,
                onActivate: () => {
                  g.onValueChange(o.value);
                  finish(o.keepOpen);
                },
              });
            })}
          </div>
        );
      }
      case "checkbox": {
        const c = entry as MenuCheckboxItem;
        return renderItem(String(i), nextIndex++, "menuitemcheckbox", c, {
          checked: c.checked,
          onActivate: () => {
            c.onCheckedChange(!c.checked);
            finish(c.keepOpen);
          },
        });
      }
      case "submenu": {
        const s = entry as MenuSubmenu;
        const idx = nextIndex++;
        return (
          <Fragment key={i}>
            {renderItem(String(i), idx, "menuitem", s, { onActivate: () => {}, sub: true })}
            {openSub?.index === idx && (
              <MenuPanel
                entries={s.items}
                label={typeof s.label === "string" ? s.label : "Submenu"}
                rootId={rootId}
                anchor={openSub.anchor}
                placement="right-start"
                focusFirst={openSub.focus}
                onCloseAll={onCloseAll}
                onCloseSelf={() => {
                  const back = openSub.anchor;
                  setOpenSub(null);
                  back.focus();
                }}
              />
            )}
          </Fragment>
        );
      }
      default: {
        const it = entry as MenuActionItem;
        return renderItem(String(i), nextIndex++, "menuitem", it, {
          onActivate: () => {
            it.onSelect?.();
            finish(it.keepOpen);
          },
        });
      }
    }
  });

  return createPortal(
    <div
      ref={setPanel}
      id={id}
      role="menu"
      aria-label={label}
      aria-orientation="vertical"
      tabIndex={-1}
      data-sr-menu={rootId}
      style={floating.style}
      className={styles.menu}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
    >
      {body}
    </div>,
    portalTarget,
  );
}

/* --------------------------------------------------------------- surface */

interface SurfaceProps {
  entries: MenuEntry[];
  label: string;
  anchor: FloatingAnchor;
  placement: FloatingPlacement;
  /** Elements (the trigger) whose mousedown must not count as "outside". */
  ignore?: Array<HTMLElement | null>;
  onClose: (restoreFocus: boolean) => void;
  id?: string;
}

/** An open menu tree: root panel plus outside-click dismissal. Mount it only while open. */
export function MenuSurface({ entries, label, anchor, placement, ignore, onClose, id }: SurfaceProps) {
  const rootId = useId();
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  // Q77: menus join the overlay stack so a Modal or Drawer above them owns Escape. Menus keep
  // their own Escape handling (MenuPanel) and do not lock scroll.
  useOverlayStack({ open: true });
  const ignoreRef = useRef(ignore);
  ignoreRef.current = ignore;
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      const t = e.target as Element | null;
      if (!t) return;
      if (t.closest(`[data-sr-menu="${rootId.replace(/"/g, "")}"]`)) return;
      if (ignoreRef.current?.some((el) => el?.contains(t))) return;
      closeRef.current(false);
    };
    document.addEventListener("mousedown", onDown, true);
    return () => document.removeEventListener("mousedown", onDown, true);
  }, [rootId]);
  return (
    <MenuPanel
      entries={entries}
      label={label}
      rootId={rootId}
      anchor={anchor}
      placement={placement}
      focusFirst
      id={id}
      onCloseAll={() => onClose(true)}
    />
  );
}

/* -------------------------------------------------------------- dropdown */

export interface DropdownMenuProps {
  /** The element that opens the menu (a button). */
  trigger: ReactElement<{
    onClick?: (e: ReactMouseEvent) => void;
    onKeyDown?: (e: ReactKeyboardEvent) => void;
    "aria-haspopup"?: "menu";
    "aria-expanded"?: boolean;
    "aria-controls"?: string;
  }>;
  items: MenuEntry[];
  /** Accessible name for the menu. */
  label: string;
  placement?: FloatingPlacement;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  className?: string;
}

export function DropdownMenu({
  trigger,
  items,
  label,
  placement = "bottom-start",
  open: controlled,
  defaultOpen = false,
  onOpenChange,
  className,
}: DropdownMenuProps) {
  const id = useId();
  const [internal, setInternal] = useState(defaultOpen);
  const open = controlled ?? internal;
  const [anchor, setAnchor] = useState<HTMLSpanElement | null>(null);

  const setOpen = useCallback(
    (next: boolean) => {
      if (controlled === undefined) setInternal(next);
      onOpenChange?.(next);
    },
    [controlled, onOpenChange],
  );

  const t = cloneElement(trigger, {
    "aria-haspopup": "menu",
    "aria-expanded": open,
    "aria-controls": open ? id : undefined,
    onClick: (e: ReactMouseEvent) => {
      trigger.props.onClick?.(e);
      setOpen(!open);
    },
    onKeyDown: (e: ReactKeyboardEvent) => {
      trigger.props.onKeyDown?.(e);
      if (e.key === "ArrowDown" && !open) {
        e.preventDefault();
        setOpen(true);
      }
    },
  });

  return (
    <span ref={setAnchor} className={cx(styles.anchor, className)}>
      {t}
      {open && (
        <MenuSurface
          id={id}
          entries={items}
          label={label}
          anchor={anchor}
          placement={placement}
          ignore={[anchor]}
          onClose={(restore) => {
            setOpen(false);
            if (restore) (anchor?.querySelector("[aria-haspopup]") as HTMLElement | null)?.focus();
          }}
        />
      )}
    </span>
  );
}

/* --------------------------------------------------------------- context */

export interface ContextMenuProps {
  /** The region that opens the menu on right-click / the Menu key / Shift+F10. */
  children: ReactNode;
  items: MenuEntry[];
  label: string;
  className?: string;
  disabled?: boolean;
  onOpenChange?: (open: boolean) => void;
}

export function ContextMenu({ children, items, label, className, disabled, onOpenChange }: ContextMenuProps) {
  const [point, setPoint] = useState<{ x: number; y: number } | null>(null);
  const returnTo = useRef<HTMLElement | null>(null);

  const press = useRef<{ x: number; y: number; timer: ReturnType<typeof setTimeout> } | null>(null);

  const close = useCallback(
    (restore: boolean) => {
      setPoint(null);
      onOpenChange?.(false);
      if (restore) returnTo.current?.focus?.();
    },
    [onOpenChange],
  );

  const openAt = useCallback(
    (x: number, y: number) => {
      returnTo.current = document.activeElement as HTMLElement | null;
      setPoint({ x, y });
      onOpenChange?.(true);
    },
    [onOpenChange],
  );

  const cancelPress = useCallback(() => {
    if (press.current) clearTimeout(press.current.timer);
    press.current = null;
  }, []);
  useEffect(() => cancelPress, [cancelPress]);

  return (
    <div
      className={cx(styles.contextRegion, className)}
      onContextMenu={(e) => {
        if (disabled || e.defaultPrevented) return;
        e.preventDefault();
        cancelPress();
        openAt(e.clientX, e.clientY);
      }}
      // Q60: touch long-press (500 ms) opens the menu where the finger rests; a moving touch is a scroll.
      onTouchStart={(e) => {
        const t = e.touches[0];
        if (disabled || !t) return;
        cancelPress();
        const { clientX: x, clientY: y } = t;
        press.current = { x, y, timer: setTimeout(() => openAt(x, y), LONG_PRESS_MS) };
      }}
      onTouchMove={(e) => {
        const t = e.touches[0];
        const p = press.current;
        if (p && t && Math.hypot(t.clientX - p.x, t.clientY - p.y) > LONG_PRESS_SLOP_PX) cancelPress();
      }}
      onTouchEnd={cancelPress}
      onTouchCancel={cancelPress}
    >
      {children}
      {point && <MenuSurface entries={items} label={label} anchor={point} placement="bottom-start" onClose={close} />}
    </div>
  );
}
