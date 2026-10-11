import {
  createContext,
  forwardRef,
  useContext,
  useId,
  useState,
  type InputHTMLAttributes,
  type ReactNode,
} from "react";
import { cx } from "../../util/cx";
import styles from "./Controls.module.css";

interface ChoiceProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "size"> {
  label: ReactNode;
  description?: ReactNode;
}

function ChoiceShell({
  kind,
  label,
  description,
  className,
  id,
  inputRef,
  inputProps,
}: {
  kind: "checkbox" | "radio" | "switch";
  label: ReactNode;
  description?: ReactNode;
  className?: string;
  id?: string;
  inputRef?: React.Ref<HTMLInputElement>;
  inputProps: InputHTMLAttributes<HTMLInputElement>;
}) {
  const auto = useId();
  const inputId = id ?? auto;
  const descId = description ? `${inputId}-desc` : undefined;
  return (
    <div className={cx(styles.choice, className)} data-disabled={inputProps.disabled || undefined}>
      <input
        {...inputProps}
        ref={inputRef}
        id={inputId}
        type={kind === "radio" ? "radio" : "checkbox"}
        role={kind === "switch" ? "switch" : undefined}
        aria-describedby={descId}
        className={cx(styles.input, styles[kind])}
      />
      <label htmlFor={inputId} className={styles.text}>
        <span>{label}</span>
        {description && (
          <span id={descId} className={styles.desc}>
            {description}
          </span>
        )}
      </label>
    </div>
  );
}

export interface CheckboxProps extends ChoiceProps {
  /** Visual and ARIA mixed state ("select all" parents). */
  indeterminate?: boolean;
}

export const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(function Checkbox(
  { label, description, className, id, indeterminate, ...rest },
  ref,
) {
  return (
    <ChoiceShell
      kind="checkbox"
      label={label}
      description={description}
      className={className}
      id={id}
      inputRef={(node) => {
        if (node) node.indeterminate = !!indeterminate;
        if (typeof ref === "function") ref(node);
        else if (ref) ref.current = node;
      }}
      inputProps={{ ...rest, "aria-checked": indeterminate ? "mixed" : undefined }}
    />
  );
});

export const Switch = forwardRef<HTMLInputElement, ChoiceProps>(function Switch(
  { label, description, className, id, ...rest },
  ref,
) {
  return (
    <ChoiceShell kind="switch" label={label} description={description} className={className} id={id} inputRef={ref} inputProps={rest} />
  );
});

interface RadioGroupContextValue {
  name: string;
  value: string | undefined;
  disabled?: boolean;
  onChange: (value: string) => void;
}
const RadioGroupContext = createContext<RadioGroupContextValue | null>(null);

export interface RadioGroupProps {
  legend: ReactNode;
  name?: string;
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  disabled?: boolean;
  orientation?: "vertical" | "horizontal";
  className?: string;
  children: ReactNode;
}

/** Native radios in a fieldset: arrow-key navigation and roving focus come from the browser. */
export function RadioGroup({
  legend,
  name,
  value,
  defaultValue,
  onValueChange,
  disabled,
  orientation = "vertical",
  className,
  children,
}: RadioGroupProps) {
  const auto = useId();
  const [internal, setInternal] = useState(defaultValue);
  const current = value !== undefined ? value : internal;
  return (
    <RadioGroupContext.Provider
      value={{
        name: name ?? auto,
        value: current,
        disabled,
        onChange: (v) => {
          if (value === undefined) setInternal(v);
          onValueChange?.(v);
        },
      }}
    >
      <fieldset className={cx(styles.group, styles[orientation], className)} disabled={disabled}>
        <legend className={styles.legend}>{legend}</legend>
        {children}
      </fieldset>
    </RadioGroupContext.Provider>
  );
}

export interface RadioProps extends Omit<ChoiceProps, "name" | "checked" | "defaultChecked" | "value"> {
  value: string;
}

export const Radio = forwardRef<HTMLInputElement, RadioProps>(function Radio(
  { label, description, className, id, value, onChange, ...rest },
  ref,
) {
  const group = useContext(RadioGroupContext);
  if (!group) throw new Error("<Radio> must be used inside <RadioGroup>");
  return (
    <ChoiceShell
      kind="radio"
      label={label}
      description={description}
      className={className}
      id={id}
      inputRef={ref}
      inputProps={{
        ...rest,
        name: group.name,
        value,
        checked: group.value === value,
        disabled: rest.disabled ?? group.disabled,
        onChange: (e) => {
          group.onChange(value);
          onChange?.(e);
        },
      }}
    />
  );
});
