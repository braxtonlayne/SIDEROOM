import { useId, type ReactNode } from "react";
import { cx } from "../../util/cx";
import styles from "./Input.module.css";

export interface FieldProps {
  label: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  id?: string;
  className?: string;
}

/** Wires label, hint and error to a control via ids. `render` receives the props to spread on the control. */
export function Field({
  label,
  hint,
  error,
  id,
  className,
  render,
  children,
}: FieldProps & {
  render: (p: { id: string; "aria-describedby"?: string; "aria-invalid"?: true }) => ReactNode;
  children?: ReactNode;
}) {
  const auto = useId();
  const controlId = id ?? auto;
  const hintId = hint ? `${controlId}-hint` : undefined;
  const errId = error ? `${controlId}-error` : undefined;
  const describedBy = [hintId, errId].filter(Boolean).join(" ") || undefined;
  return (
    <div className={cx(styles.field, className)}>
      <label htmlFor={controlId} className={styles.label}>
        {label}
      </label>
      {render({ id: controlId, "aria-describedby": describedBy, "aria-invalid": error ? true : undefined })}
      {children}
      {hint && (
        <p id={hintId} className={styles.hint}>
          {hint}
        </p>
      )}
      {error && (
        <p id={errId} className={styles.error}>
          {error}
        </p>
      )}
    </div>
  );
}
