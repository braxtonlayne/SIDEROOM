import { Component, type ErrorInfo, type HTMLAttributes, type ReactNode } from "react";
import { cx } from "../../util/cx";
import styles from "./State.module.css";

export interface EmptyStateProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  /** Decorative illustration or icon. */
  icon?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  /** Call-to-action, usually a Button. */
  action?: ReactNode;
}

/** DS-PRIM-011: "nothing here yet" block. */
export function EmptyState({ icon, title, description, action, className, ...rest }: EmptyStateProps) {
  return (
    <div {...rest} className={cx(styles.empty, className)}>
      {icon && (
        <div className={styles.icon} aria-hidden="true">
          {icon}
        </div>
      )}
      <p className={styles.title}>{title}</p>
      {description && <p className={styles.desc}>{description}</p>}
      {action && <div className={styles.action}>{action}</div>}
    </div>
  );
}

export interface ErrorPageProps {
  title?: ReactNode;
  description?: ReactNode;
  /** Technical detail, shown collapsed. */
  error?: Error | null;
  /** Rendered as the primary action (retry / reload). */
  action?: ReactNode;
  className?: string;
}

/** DS-PRIM-011: full-page error presentation. Uses role=alert so the failure is announced. */
export function ErrorPage({
  title = "Something went wrong",
  description = "An unexpected error stopped this page from loading.",
  error,
  action,
  className,
}: ErrorPageProps) {
  return (
    <div role="alert" className={cx(styles.errorPage, className)}>
      <h1 className={styles.errorTitle}>{title}</h1>
      <p className={styles.desc}>{description}</p>
      {action && <div className={styles.action}>{action}</div>}
      {error && (
        <details className={styles.details}>
          <summary>Technical details</summary>
          <pre>{error.message}</pre>
        </details>
      )}
    </div>
  );
}

export interface ErrorBoundaryProps {
  children: ReactNode;
  /** Custom fallback; receives the error and a reset callback. Defaults to <ErrorPage>. */
  fallback?: (args: { error: Error; reset: () => void }) => ReactNode;
  onError?: (error: Error, info: ErrorInfo) => void;
}

interface ErrorBoundaryState {
  error: Error | null;
}

/** DS-PRIM-011: catches render errors below it and shows the error page with a retry. */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    this.props.onError?.(error, info);
  }

  reset = () => this.setState({ error: null });

  override render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    if (this.props.fallback) return this.props.fallback({ error, reset: this.reset });
    return (
      <ErrorPage
        error={error}
        action={
          <button type="button" className={styles.retry} onClick={this.reset}>
            Try again
          </button>
        }
      />
    );
  }
}
