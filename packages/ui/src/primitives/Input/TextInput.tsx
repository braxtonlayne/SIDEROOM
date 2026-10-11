import {
  forwardRef,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type InputHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";
import { cx } from "../../util/cx";
import { Field, type FieldProps } from "./Field";
import styles from "./Input.module.css";

export interface TextInputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "id">, FieldProps {}

export const TextInput = forwardRef<HTMLInputElement, TextInputProps>(function TextInput(
  { label, hint, error, id, className, ...rest },
  ref,
) {
  return (
    <Field
      label={label}
      hint={hint}
      error={error}
      id={id}
      className={className}
      render={(p) => <input {...rest} {...p} ref={ref} className={styles.control} />}
    />
  );
});

export interface TextAreaProps extends Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "id">, FieldProps {
  /** Grow with content up to `maxRows`. */
  autoGrow?: boolean;
  maxRows?: number;
}

export const TextArea = forwardRef<HTMLTextAreaElement, TextAreaProps>(function TextArea(
  { label, hint, error, id, className, autoGrow, maxRows = 10, onChange, rows = 3, ...rest },
  ref,
) {
  const inner = useRef<HTMLTextAreaElement | null>(null);
  const setRefs = useCallback(
    (node: HTMLTextAreaElement | null) => {
      inner.current = node;
      if (typeof ref === "function") ref(node);
      else if (ref) ref.current = node;
    },
    [ref],
  );
  const resize = useCallback(() => {
    const el = inner.current;
    if (!el || !autoGrow) return;
    el.style.height = "auto";
    const line = parseFloat(getComputedStyle(el).lineHeight) || 20;
    const max = line * maxRows + 16;
    el.style.height = `${Math.min(el.scrollHeight, max)}px`;
    el.style.overflowY = el.scrollHeight > max ? "auto" : "hidden";
  }, [autoGrow, maxRows]);
  useLayoutEffect(resize, [resize, rest.value]);

  return (
    <Field
      label={label}
      hint={hint}
      error={error}
      id={id}
      className={className}
      render={(p) => (
        <textarea
          {...rest}
          {...p}
          rows={rows}
          ref={setRefs}
          onChange={(e) => {
            resize();
            onChange?.(e);
          }}
          className={cx(styles.control, styles.textarea)}
        />
      )}
    />
  );
});

export type PasswordStrength = 0 | 1 | 2 | 3 | 4;

/** Cheap local heuristic (length + character classes). Not a substitute for server-side checks. */
export function passwordStrength(pw: string): PasswordStrength {
  if (!pw) return 0;
  let score = 0;
  if (pw.length >= 8) score++;
  if (pw.length >= 12) score++;
  if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) score++;
  if (/\d/.test(pw) && /[^A-Za-z0-9]/.test(pw)) score++;
  return Math.min(score, 4) as PasswordStrength;
}

const STRENGTH_LABELS = ["Too weak", "Weak", "Fair", "Good", "Strong"] as const;

export interface PasswordInputProps extends Omit<TextInputProps, "type"> {
  showStrength?: boolean;
}

export const PasswordInput = forwardRef<HTMLInputElement, PasswordInputProps>(function PasswordInput(
  { showStrength, label, hint, error, id, className, value, defaultValue, onChange, ...rest },
  ref,
) {
  const [visible, setVisible] = useState(false);
  const [internal, setInternal] = useState(String(defaultValue ?? ""));
  const current = value !== undefined ? String(value) : internal;
  const strength = passwordStrength(current);
  return (
    <Field
      label={label}
      hint={hint}
      error={error}
      id={id}
      className={className}
      render={(p) => (
        <div className={styles.passwordWrap}>
          <input
            {...rest}
            {...p}
            ref={ref}
            value={value}
            defaultValue={value === undefined ? defaultValue : undefined}
            onChange={(e) => {
              setInternal(e.target.value);
              onChange?.(e);
            }}
            type={visible ? "text" : "password"}
            className={cx(styles.control, styles.hasToggle)}
          />
          <button
            type="button"
            className={styles.toggle}
            aria-pressed={visible}
            aria-label={visible ? "Hide password" : "Show password"}
            onClick={() => setVisible((v) => !v)}
          >
            {visible ? "Hide" : "Show"}
          </button>
        </div>
      )}
    >
      {showStrength && (
        <div className={styles.strength}>
          <meter min={0} max={4} value={strength} aria-label="Password strength" className={styles.meter} />
          <span className={styles.hint} data-testid="strength-label">
            {STRENGTH_LABELS[strength]}
          </span>
        </div>
      )}
    </Field>
  );
});
