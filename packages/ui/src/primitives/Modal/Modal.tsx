import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { usePortalTarget } from "../../theme/ThemeProvider";
import { cx } from "../../util/cx";
import { Button } from "../Button/Button";
import { TextInput } from "../Input/TextInput";
import { useFocusTrap, useOverlayStack } from "../Overlay/overlayStack";
import styles from "./Modal.module.css";

export interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  size?: "sm" | "md" | "lg";
  /** Close when the backdrop is clicked (default true). */
  closeOnBackdrop?: boolean;
  /** Close on Escape (default true). */
  closeOnEscape?: boolean;
  footer?: ReactNode;
  /** Element to focus first; defaults to the first focusable element, else the dialog. */
  initialFocusRef?: React.RefObject<HTMLElement>;
  className?: string;
  children?: ReactNode;
}

export function Modal({
  open,
  onClose,
  title,
  description,
  size = "md",
  closeOnBackdrop = true,
  closeOnEscape = true,
  footer,
  initialFocusRef,
  className,
  children,
}: ModalProps) {
  const portalTarget = usePortalTarget();
  const titleId = useId();
  const descId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  const { isTopmost } = useOverlayStack({
    open,
    lockScroll: true,
    onEscape: closeOnEscape ? () => onCloseRef.current() : undefined,
  });
  useFocusTrap({ open, containerRef: dialogRef, isTopmost, initialFocusRef });

  if (!open || !portalTarget) return null;

  return createPortal(
    // Backdrop click is a pointer convenience only; the keyboard equivalent is Escape / the Close button.
    // eslint-disable-next-line jsx-a11y/no-static-element-interactions
    <div
      className={styles.backdrop}
      data-sr-part="modal-backdrop"
      onMouseDown={(e) => {
        if (closeOnBackdrop && e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descId : undefined}
        tabIndex={-1}
        className={cx(styles.dialog, styles[size], className)}
        data-sr-part="modal"
      >
        <header className={styles.header}>
          <h2 id={titleId} className={styles.title}>
            {title}
          </h2>
          <button type="button" className={styles.close} aria-label="Close" onClick={onClose}>
            <span aria-hidden="true">×</span>
          </button>
        </header>
        {description && (
          <p id={descId} className={styles.description}>
            {description}
          </p>
        )}
        <div className={styles.body}>{children}</div>
        {footer && <footer className={styles.footer}>{footer}</footer>}
      </div>
    </div>,
    portalTarget,
  );
}

export interface ConfirmDialogProps {
  open: boolean;
  title: ReactNode;
  description?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Styles the confirm button as destructive. */
  danger?: boolean;
  /** Typed-confirmation variant: confirm stays disabled until this exact text is typed. */
  requireText?: string;
  loading?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  danger,
  requireText,
  loading,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const [typed, setTyped] = useState("");
  useEffect(() => {
    if (!open) setTyped("");
  }, [open]);
  const ready = requireText === undefined || typed === requireText;
  return (
    <Modal
      open={open}
      onClose={onCancel}
      title={title}
      description={description}
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={onCancel} data-autofocus={requireText === undefined ? "" : undefined}>
            {cancelLabel}
          </Button>
          <Button variant={danger ? "danger" : "primary"} disabled={!ready} loading={loading} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </>
      }
    >
      {requireText !== undefined && (
        <TextInput
          label={`Type "${requireText}" to confirm`}
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          autoComplete="off"
          data-autofocus=""
        />
      )}
    </Modal>
  );
}
