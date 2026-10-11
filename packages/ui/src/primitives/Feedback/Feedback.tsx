import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type HTMLAttributes,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { usePortalTarget } from "../../theme/ThemeProvider";
import { cx } from "../../util/cx";
import styles from "./Feedback.module.css";

export type NoticeVariant = "info" | "success" | "warning" | "danger";

const ICONS: Record<NoticeVariant, string> = { info: "i", success: "✓", warning: "!", danger: "!" };
const VARIANT_LABEL: Record<NoticeVariant, string> = {
  info: "Information",
  success: "Success",
  warning: "Warning",
  danger: "Error",
};

export interface NoticeProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  variant?: NoticeVariant;
  title?: ReactNode;
  onDismiss?: () => void;
  children?: ReactNode;
}

/** Inline banner. Warnings and errors are announced assertively (role=alert). */
export function Notice({ variant = "info", title, onDismiss, className, children, ...rest }: NoticeProps) {
  const urgent = variant === "danger" || variant === "warning";
  return (
    <div role={urgent ? "alert" : "status"} {...rest} className={cx(styles.notice, styles[variant], className)}>
      <span className={styles.icon} aria-hidden="true">
        {ICONS[variant]}
      </span>
      <div className={styles.content}>
        <span className={styles.srOnly}>{VARIANT_LABEL[variant]}: </span>
        {title && <strong className={styles.title}>{title}</strong>}
        {children && <div className={styles.text}>{children}</div>}
      </div>
      {onDismiss && (
        <button type="button" className={styles.dismiss} aria-label="Dismiss" onClick={onDismiss}>
          <span aria-hidden="true">×</span>
        </button>
      )}
    </div>
  );
}

export interface ToastOptions {
  title: ReactNode;
  description?: ReactNode;
  variant?: NoticeVariant;
  /** ms before auto-dismiss; 0 keeps it until dismissed. Default 5000. */
  duration?: number;
}

interface ToastItem extends ToastOptions {
  id: number;
}

interface ToastApi {
  show: (options: ToastOptions) => number;
  dismiss: (id: number) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

export function useToast(): ToastApi {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToast must be used inside <ToastProvider>");
  return ctx;
}

function ToastView({ toast, onDismiss }: { toast: ToastItem; onDismiss: (id: number) => void }) {
  const { id, duration = 5000 } = toast;
  const [paused, setPaused] = useState(false);
  useEffect(() => {
    if (duration <= 0 || paused) return;
    const t = setTimeout(() => onDismiss(id), duration);
    return () => clearTimeout(t);
  }, [id, duration, paused, onDismiss]);

  return (
    <li
      className={styles.toastItem}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
    >
      <Notice
        variant={toast.variant}
        title={toast.title}
        onDismiss={() => onDismiss(id)}
        // The surrounding live region announces; avoid double announcement.
        role="presentation"
        className={styles.toast}
      >
        {toast.description}
      </Notice>
    </li>
  );
}

export function ToastProvider({ children, max = 5 }: { children: ReactNode; max?: number }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const next = useRef(1);
  const target = usePortalTarget();

  const dismiss = useCallback((id: number) => setToasts((t) => t.filter((x) => x.id !== id)), []);
  const show = useCallback(
    (options: ToastOptions) => {
      const id = next.current++;
      setToasts((t) => [...t, { ...options, id }].slice(-max));
      return id;
    },
    [max],
  );
  const api = useMemo(() => ({ show, dismiss }), [show, dismiss]);
  const urgent = toasts.some((t) => t.variant === "danger");

  return (
    <ToastContext.Provider value={api}>
      {children}
      {target &&
        createPortal(
          <div role={urgent ? "alert" : "status"} aria-live={urgent ? "assertive" : "polite"} aria-label="Notifications">
            <ol className={styles.region}>
              {toasts.map((t) => (
                <ToastView key={t.id} toast={t} onDismiss={dismiss} />
              ))}
            </ol>
          </div>,
          target,
        )}
    </ToastContext.Provider>
  );
}
