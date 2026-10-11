import { useId, type InputHTMLAttributes, type ReactNode } from "react";
import { cx } from "../../util/cx";
import styles from "./DateTime.module.css";

interface PickerProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "value" | "defaultValue" | "onChange" | "size"> {
  label: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  onValueChange?: (value: string) => void;
}

function makePicker(type: "date" | "time" | "datetime-local", displayName: string) {
  function Picker({ label, hint, error, id, className, onValueChange, ...rest }: PickerProps & { value?: string; defaultValue?: string }) {
    const auto = useId();
    const inputId = id ?? auto;
    const hintId = hint ? `${inputId}-hint` : undefined;
    const errId = error ? `${inputId}-error` : undefined;
    return (
      <div className={cx(styles.field, className)}>
        <label htmlFor={inputId} className={styles.label}>
          {label}
        </label>
        <input
          {...rest}
          id={inputId}
          type={type}
          className={styles.input}
          aria-invalid={error ? true : undefined}
          aria-describedby={[hintId, errId].filter(Boolean).join(" ") || undefined}
          onChange={(e) => onValueChange?.(e.target.value)}
        />
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
  Picker.displayName = displayName;
  return Picker;
}

export type DatePickerProps = PickerProps & { value?: string; defaultValue?: string };

/** DS-PRIM-013: date field on the native picker (`YYYY-MM-DD`), so locale, keyboard entry and mobile pickers come for free. */
export const DatePicker: (p: DatePickerProps) => ReactNode = makePicker("date", "DatePicker");
/** DS-PRIM-013: time field on the native picker (`HH:MM`). */
export const TimePicker: (p: DatePickerProps) => ReactNode = makePicker("time", "TimePicker");
/** DS-PRIM-013: combined date and time field (`YYYY-MM-DDTHH:MM`), for scheduling (watch parties, events). */
export const DateTimePicker: (p: DatePickerProps) => ReactNode = makePicker("datetime-local", "DateTimePicker");
