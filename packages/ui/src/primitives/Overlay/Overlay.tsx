import {
  cloneElement,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type FocusEvent,
  type MouseEvent,
  type ReactElement,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { usePortalTarget } from "../../theme/ThemeProvider";
import type { FloatingPlacement } from "../../util/position";
import { cx } from "../../util/cx";
import styles from "./Overlay.module.css";
import { useFloating } from "./useFloating";

/** Preferred placement; flips and shifts automatically at viewport edges. */
export type Placement = FloatingPlacement;

type TriggerProps = {
  onMouseEnter?: (e: MouseEvent) => void;
  onMouseLeave?: (e: MouseEvent) => void;
  onFocus?: (e: FocusEvent) => void;
  onBlur?: (e: FocusEvent) => void;
  onClick?: (e: MouseEvent) => void;
  "aria-describedby"?: string;
  "aria-expanded"?: boolean;
  "aria-haspopup"?: "dialog";
  "aria-controls"?: string;
};

export interface TooltipProps {
  content: ReactNode;
  /** A single focusable element. The tooltip describes it (aria-describedby). */
  children: ReactElement<TriggerProps>;
  placement?: Placement;
  /** Delay before showing on hover, in ms (focus shows immediately). */
  delay?: number;
  className?: string;
}

export function Tooltip({ content, children, placement = "top", delay = 400, className }: TooltipProps) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<HTMLSpanElement | null>(null);
  const portalTarget = usePortalTarget();
  const floating = useFloating({ open, anchor, placement });
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const clear = useCallback(() => clearTimeout(timer.current), []);
  useEffect(() => clear, [clear]);

  const show = (immediate: boolean) => {
    clear();
    if (immediate || delay <= 0) setOpen(true);
    else timer.current = setTimeout(() => setOpen(true), delay);
  };
  const hide = useCallback(() => {
    clear();
    setOpen(false);
  }, [clear]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") hide();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, hide]);

  const child = cloneElement(children, {
    "aria-describedby": open ? id : children.props["aria-describedby"],
    onMouseEnter: (e: MouseEvent) => {
      children.props.onMouseEnter?.(e);
      show(false);
    },
    onMouseLeave: (e: MouseEvent) => {
      children.props.onMouseLeave?.(e);
      hide();
    },
    onFocus: (e: FocusEvent) => {
      children.props.onFocus?.(e);
      show(true);
    },
    onBlur: (e: FocusEvent) => {
      children.props.onBlur?.(e);
      hide();
    },
  });

  return (
    <span ref={setAnchor} className={cx(styles.anchor, className)}>
      {child}
      {open &&
        portalTarget &&
        createPortal(
          <span
            role="tooltip"
            id={id}
            ref={floating.ref}
            style={floating.style}
            data-side={floating.position?.side}
            className={styles.tooltip}
          >
            {content}
          </span>,
          portalTarget,
        )}
    </span>
  );
}

export interface PopoverProps {
  /** The element that toggles the popover. */
  trigger: ReactElement<TriggerProps>;
  /** Accessible name for the popover dialog. */
  label: string;
  children: ReactNode;
  placement?: Placement;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  className?: string;
}

export function Popover({
  trigger,
  label,
  children,
  placement = "bottom",
  open: controlled,
  defaultOpen = false,
  onOpenChange,
  className,
}: PopoverProps) {
  const id = useId();
  const [internal, setInternal] = useState(defaultOpen);
  const open = controlled ?? internal;
  const root = useRef<HTMLSpanElement | null>(null);
  const panel = useRef<HTMLDivElement | null>(null);
  const [anchor, setAnchor] = useState<HTMLSpanElement | null>(null);
  const portalTarget = usePortalTarget();
  const floating = useFloating({ open, anchor, placement });
  const setPanel = useCallback(
    (el: HTMLDivElement | null) => {
      panel.current = el;
      floating.ref(el);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [floating.ref],
  );

  const setOpen = useCallback(
    (next: boolean) => {
      if (controlled === undefined) setInternal(next);
      onOpenChange?.(next);
    },
    [controlled, onOpenChange],
  );

  useEffect(() => {
    if (!open) return;
    const returnFocusTo = document.activeElement as HTMLElement | null;
    panel.current?.focus();
    const onDown = (e: globalThis.MouseEvent) => {
      const t = e.target as Node;
      if (root.current && !root.current.contains(t) && !panel.current?.contains(t)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
        (root.current?.querySelector("[aria-haspopup]") as HTMLElement | null)?.focus();
      }
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
      if (returnFocusTo && document.activeElement === document.body) returnFocusTo.focus?.();
    };
  }, [open, setOpen]);

  const t = cloneElement(trigger, {
    "aria-haspopup": "dialog",
    "aria-expanded": open,
    "aria-controls": open ? id : undefined,
    onClick: (e: MouseEvent) => {
      trigger.props.onClick?.(e);
      setOpen(!open);
    },
  });

  return (
    <span
      ref={(el) => {
        root.current = el;
        setAnchor(el);
      }}
      className={cx(styles.anchor, className)}
    >
      {t}
      {open &&
        portalTarget &&
        createPortal(
          <div
            ref={setPanel}
            id={id}
            role="dialog"
            aria-label={label}
            tabIndex={-1}
            style={floating.style}
            data-side={floating.position?.side}
            className={styles.popover}
          >
            {children}
          </div>,
          portalTarget,
        )}
    </span>
  );
}
