import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import { cx } from "../../util/cx";
import { Spinner } from "../Loading/Spinner";
import styles from "./Button.module.css";

export type ButtonVariant = "primary" | "secondary" | "danger" | "link" | "ghost";
export type ButtonSize = "sm" | "md" | "lg";

interface ButtonBase extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Shows a spinner, sets aria-busy and blocks activation (stays focusable). */
  loading?: boolean;
  leadingIcon?: ReactNode;
  trailingIcon?: ReactNode;
  fullWidth?: boolean;
}

export type ButtonProps =
  | (ButtonBase & { iconOnly?: false; children: ReactNode })
  /** Icon-only buttons must carry an accessible name. */
  | (ButtonBase & { iconOnly: true; "aria-label": string; children: ReactNode });

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    variant = "primary",
    size = "md",
    loading = false,
    iconOnly = false,
    leadingIcon,
    trailingIcon,
    fullWidth,
    className,
    type = "button",
    disabled,
    onClick,
    children,
    ...rest
  },
  ref,
) {
  const blocked = disabled || loading;
  return (
    <button
      {...rest}
      ref={ref}
      type={type}
      disabled={disabled}
      aria-disabled={loading ? true : undefined}
      aria-busy={loading || undefined}
      data-variant={variant}
      data-loading={loading || undefined}
      className={cx(
        styles.button,
        styles[variant],
        styles[size],
        iconOnly && styles.iconOnly,
        fullWidth && styles.full,
        className,
      )}
      onClick={(e) => {
        if (blocked) {
          e.preventDefault();
          return;
        }
        onClick?.(e);
      }}
    >
      {loading && <Spinner size="sm" label="Loading" className={styles.spinner} />}
      {!loading && leadingIcon}
      <span className={cx(styles.label, loading && styles.hidden)}>{children}</span>
      {!loading && trailingIcon}
    </button>
  );
});
