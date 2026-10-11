import { useId, useState, type ReactNode, type SelectHTMLAttributes } from "react";
import { cx } from "../util/cx";
import styles from "./FilterChips.module.css";

export interface FilterChipOption {
  value: string;
  label: ReactNode;
  icon?: ReactNode;
}

interface Common {
  /** Accessible name for the group, e.g. "Genres". */
  label: string;
  options: FilterChipOption[];
  className?: string;
}

export type FilterChipsProps = Common &
  (
    | { multiple?: false; value?: string | null; defaultValue?: string | null; onValueChange?: (value: string | null) => void }
    | { multiple: true; value?: string[]; defaultValue?: string[]; onValueChange?: (value: string[]) => void }
  );

/** DS-SR-007: row of square filter chips; single (re-click clears) or multi select. Each chip is a toggle button. */
export function FilterChips(props: FilterChipsProps) {
  const { label, options, className } = props;
  const [internal, setInternal] = useState<string[]>(() => {
    const d = props.defaultValue;
    return Array.isArray(d) ? d : d ? [d] : [];
  });
  const controlled = props.value !== undefined;
  const selected = controlled ? (Array.isArray(props.value) ? props.value : props.value ? [props.value] : []) : internal;

  const toggle = (v: string) => {
    const on = selected.includes(v);
    if (props.multiple) {
      const next = on ? selected.filter((x) => x !== v) : [...selected, v];
      if (!controlled) setInternal(next);
      props.onValueChange?.(next);
    } else {
      const next = on ? null : v;
      if (!controlled) setInternal(next ? [next] : []);
      props.onValueChange?.(next);
    }
  };

  return (
    <div role="group" aria-label={label} className={cx(styles.chips, className)}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          className={styles.chip}
          aria-pressed={selected.includes(o.value)}
          onClick={() => toggle(o.value)}
        >
          {o.icon && (
            <span className={styles.icon} aria-hidden="true">
              {o.icon}
            </span>
          )}
          {o.label}
        </button>
      ))}
    </div>
  );
}

export interface DropdownChipProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "onChange" | "value" | "defaultValue"> {
  /** Accessible name and the placeholder text ("Room selection"). */
  label: string;
  options: Array<{ value: string; label: string }>;
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
}

/**
 * DS-SR-007: chip-styled dropdown ("Room selection ▾"). Built on a native `<select>` so the
 * popup, keyboard handling and mobile pickers are the platform's.
 */
export function DropdownChip({ label, options, value, defaultValue, onValueChange, className, id, ...rest }: DropdownChipProps) {
  const auto = useId();
  const selectId = id ?? auto;
  return (
    <span className={cx(styles.dropdown, className)}>
      <label htmlFor={selectId} className={styles.dropLabel}>
        {label}
      </label>
      <select
        {...rest}
        id={selectId}
        className={styles.select}
        value={value}
        defaultValue={value === undefined ? (defaultValue ?? options[0]?.value) : undefined}
        onChange={(e) => onValueChange?.(e.target.value)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </span>
  );
}
