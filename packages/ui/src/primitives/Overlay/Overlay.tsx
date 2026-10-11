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
import { cx } from "../../util/cx";
import styles from "./Overlay.module.css";

export type Placement = "top" | "bottom" | "left" | "right";

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
    <span className={cx(styles.anchor, className)}>
      {child}
      {open && (
        <span role="tooltip" id={id} className={cx(styles.tooltip, styles[placement])}>
          {content}
        </span>
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
  placement?: Exclude<Placement, "left" | "right"> | "left" | "right";
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
  const root = useRef<HTMLSpanElement>(null);
  const panel = useRef<HTMLDivElement>(null);

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
      if (root.current && !root.current.contains(e.target as Node)) setOpen(false);
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
    <span ref={root} className={cx(styles.anchor, className)}>
      {t}
      {open && (
        <div
          ref={panel}
          id={id}
          role="dialog"
          aria-label={label}
          tabIndex={-1}
          className={cx(styles.popover, styles[placement])}
        >
          {children}
        </div>
      )}
    </span>
  );
}
