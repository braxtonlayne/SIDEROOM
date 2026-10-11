import { useId, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { usePortalTarget } from "../../theme/ThemeProvider";
import { cx } from "../../util/cx";
import { useFocusTrap, useOverlayStack } from "../Overlay/overlayStack";
import styles from "./Drawer.module.css";

export interface DrawerProps {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  /** Edge the drawer slides from. */
  side?: "left" | "right" | "bottom";
  closeOnBackdrop?: boolean;
  footer?: ReactNode;
  className?: string;
  children?: ReactNode;
}

/** DS-PRIM-006: modal side/bottom sheet with focus trap, Escape and backdrop close. Shares the overlay stack with Modal (Q77). */
export function Drawer({
  open,
  onClose,
  title,
  side = "right",
  closeOnBackdrop = true,
  footer,
  className,
  children,
}: DrawerProps) {
  const target = usePortalTarget();
  const titleId = useId();
  const ref = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  const { isTopmost } = useOverlayStack({ open, lockScroll: true, onEscape: () => onCloseRef.current() });
  useFocusTrap({ open, containerRef: ref, isTopmost });

  if (!open || !target) return null;
  return createPortal(
    // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- backdrop click is a pointer convenience; Escape and the Close button are the accessible paths
    <div className={styles.backdrop} onMouseDown={(e) => closeOnBackdrop && e.target === e.currentTarget && onClose()}>
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className={cx(styles.drawer, styles[side], className)}
      >
        <header className={styles.header}>
          <h2 id={titleId} className={styles.title}>
            {title}
          </h2>
          <button type="button" className={styles.close} aria-label="Close" onClick={onClose}>
            <span aria-hidden="true">×</span>
          </button>
        </header>
        <div className={styles.body}>{children}</div>
        {footer && <footer className={styles.footer}>{footer}</footer>}
      </div>
    </div>,
    target,
  );
}
