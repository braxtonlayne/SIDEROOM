import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { useVirtualizer } from "@tanstack/react-virtual";
import { usePortalTarget } from "../../theme/ThemeProvider";
import { cx } from "../../util/cx";
import { observeRectWithFallback } from "../../util/virtual";
import { useFloating } from "../Overlay/useFloating";
import styles from "./Select.module.css";

export interface SelectOption {
  value: string;
  label: string;
  /** Secondary line shown under the label. Makes rows taller (set `rowHeight` accordingly). */
  description?: string;
  disabled?: boolean;
}

interface BaseProps {
  options: SelectOption[];
  /** Accessible name (or use `aria-labelledby` / an external <label htmlFor={id}>). */
  "aria-label"?: string;
  "aria-labelledby"?: string;
  id?: string;
  name?: string;
  placeholder?: string;
  disabled?: boolean;
  invalid?: boolean;
  className?: string;
  /** Type-to-filter text box instead of a plain button trigger. */
  searchable?: boolean;
  /** Custom matcher; default is case-insensitive substring on the label. */
  filter?: (option: SelectOption, query: string) => boolean;
  emptyText?: string;
  /** Row height in px used for virtualization. Default 36. */
  rowHeight?: number;
  /** Max popup height in px. Default 280. */
  maxListHeight?: number;
}

export interface SelectProps extends BaseProps {
  value?: string | null;
  defaultValue?: string | null;
  onValueChange?: (value: string) => void;
}
export interface MultiSelectProps extends BaseProps {
  values?: string[];
  defaultValues?: string[];
  onValuesChange?: (values: string[]) => void;
}

const defaultFilter = (o: SelectOption, q: string) => o.label.toLowerCase().includes(q.trim().toLowerCase());

/* ------------------------------------------------------------ option list */

interface OptionListProps {
  listId: string;
  options: SelectOption[];
  active: number;
  isSelected: (o: SelectOption) => boolean;
  multiple: boolean;
  rowHeight: number;
  maxHeight: number;
  label?: string;
  labelledBy?: string;
  onPick: (o: SelectOption) => void;
  onHover: (index: number) => void;
}

function OptionList({
  listId,
  options,
  active,
  isSelected,
  multiple,
  rowHeight,
  maxHeight,
  label,
  labelledBy,
  onPick,
  onHover,
}: OptionListProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const height = Math.min(maxHeight, options.length * rowHeight);
  const v = useVirtualizer({
    count: options.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => rowHeight,
    overscan: 6,
    initialRect: { width: 0, height },
    observeElementRect: observeRectWithFallback({ width: 0, height }),
  });
  useEffect(() => {
    if (active >= 0) v.scrollToIndex(active, { align: "auto" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  return (
    <div
      ref={scroller}
      id={listId}
      role="listbox"
      tabIndex={-1}
      aria-label={label}
      aria-labelledby={labelledBy}
      aria-multiselectable={multiple || undefined}
      className={styles.list}
      style={{ height }}
    >
      <div style={{ height: v.getTotalSize(), position: "relative", width: "100%" }}>
        {v.getVirtualItems().map((row) => {
          const o = options[row.index] as SelectOption;
          return (
            // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/interactive-supports-focus -- keyboard is handled by the combobox (aria-activedescendant)
            <div
              key={o.value}
              id={`${listId}-o-${row.index}`}
              role="option"
              aria-selected={isSelected(o)}
              aria-disabled={o.disabled || undefined}
              aria-setsize={options.length}
              aria-posinset={row.index + 1}
              data-active={row.index === active || undefined}
              className={cx(styles.option, o.description && styles.twoLine)}
              style={{ height: row.size, transform: `translateY(${row.start}px)` }}
              onMouseMove={() => !o.disabled && row.index !== active && onHover(row.index)}
              onClick={() => !o.disabled && onPick(o)}
            >
              <span className={styles.check} aria-hidden="true">
                {isSelected(o) ? "✓" : ""}
              </span>
              <span className={styles.optionText}>
                <span className={styles.optionLabel}>{o.label}</span>
                {o.description && <span className={styles.optionDesc}>{o.description}</span>}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------- core */

interface CoreProps extends BaseProps {
  multiple: boolean;
  selected: string[];
  onToggle: (value: string) => void;
  onRemoveLast?: () => void;
}

function Core({
  options,
  multiple,
  selected,
  onToggle,
  onRemoveLast,
  searchable = false,
  filter = defaultFilter,
  emptyText = "No results",
  rowHeight = 36,
  maxListHeight = 280,
  placeholder,
  disabled,
  invalid,
  className,
  id: idProp,
  name,
  ...aria
}: CoreProps) {
  const auto = useId();
  const id = idProp ?? auto;
  const listId = `${id}-list`;
  const portalTarget = usePortalTarget();

  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [typing, setTyping] = useState(false);
  const [active, setActive] = useState(-1);
  const [wrap, setWrap] = useState<HTMLDivElement | null>(null);
  const ctrl = useRef<HTMLInputElement | HTMLButtonElement | null>(null);
  const typeahead = useRef({ buffer: "", timer: undefined as ReturnType<typeof setTimeout> | undefined });
  const floating = useFloating({ open, anchor: wrap, placement: "bottom-start", offset: 4 });

  const byValue = useMemo(() => new Map(options.map((o) => [o.value, o])), [options]);
  const visible = useMemo(
    () => (searchable && typing && query ? options.filter((o) => filter(o, query)) : options),
    [options, searchable, typing, query, filter],
  );
  const selectedSet = useMemo(() => new Set(selected), [selected]);
  const selectedLabel = !multiple && selected[0] !== undefined ? (byValue.get(selected[0])?.label ?? "") : "";

  useEffect(() => () => clearTimeout(typeahead.current.timer), []);

  const firstEnabled = useCallback(
    (list: SelectOption[], from: number, dir: 1 | -1) => {
      for (let n = 0; n < list.length; n++) {
        const i = (((from + dir * n) % list.length) + list.length) % list.length;
        if (!list[i]?.disabled) return i;
      }
      return -1;
    },
    [],
  );

  const doOpen = () => {
    if (open || disabled) return;
    const sel = visible.findIndex((o) => selectedSet.has(o.value) && !o.disabled);
    setActive(sel >= 0 ? sel : firstEnabled(visible, 0, 1));
    setOpen(true);
  };
  const doClose = useCallback(() => {
    setOpen(false);
    setTyping(false);
    setQuery("");
  }, []);

  const pick = (o: SelectOption) => {
    onToggle(o.value);
    if (multiple) {
      setQuery("");
      setTyping(false);
      setActive(visible.findIndex((x) => x.value === o.value));
    } else {
      doClose();
    }
    ctrl.current?.focus();
  };

  const step = (dir: 1 | -1) => {
    if (!visible.length) return;
    setActive((cur) => firstEnabled(visible, cur < 0 ? (dir === 1 ? 0 : visible.length - 1) : cur + dir, dir));
  };

  const onKeyDown = (e: ReactKeyboardEvent<HTMLElement>) => {
    switch (e.key) {
      case "ArrowDown":
      case "ArrowUp": {
        e.preventDefault();
        if (!open) doOpen();
        else step(e.key === "ArrowDown" ? 1 : -1);
        return;
      }
      case "Home":
      case "End":
        if (open && !searchable) {
          e.preventDefault();
          setActive(firstEnabled(visible, e.key === "Home" ? 0 : visible.length - 1, e.key === "Home" ? 1 : -1));
        }
        return;
      case "Enter":
        if (open) {
          e.preventDefault();
          const o = visible[active];
          if (o && !o.disabled) pick(o);
        } else if (!searchable) {
          // button: native click toggles
        }
        return;
      case "Escape":
        if (open) {
          e.preventDefault();
          e.stopPropagation();
          doClose();
        }
        return;
      case "Tab":
        if (open) doClose();
        return;
      case "Backspace":
        if (multiple && searchable && !query) onRemoveLast?.();
        return;
      default:
        if (!searchable && e.key.length === 1 && e.key !== " " && !e.ctrlKey && !e.metaKey && !e.altKey) {
          const t = typeahead.current;
          clearTimeout(t.timer);
          t.buffer += e.key.toLowerCase();
          t.timer = setTimeout(() => (t.buffer = ""), 600);
          const start = Math.max(0, active) + (t.buffer.length === 1 ? 1 : 0);
          const order = [...visible.slice(start), ...visible.slice(0, start)];
          const hit = order.find((o) => !o.disabled && o.label.toLowerCase().startsWith(t.buffer));
          if (hit) {
            if (!open) setOpen(true);
            setActive(visible.indexOf(hit));
          }
        }
    }
  };

  const activeId = open && active >= 0 && visible[active] ? `${listId}-o-${active}` : undefined;
  const common = {
    id,
    role: "combobox" as const,
    "aria-haspopup": "listbox" as const,
    "aria-expanded": open,
    "aria-controls": open ? listId : undefined,
    "aria-activedescendant": activeId,
    "aria-label": aria["aria-label"],
    "aria-labelledby": aria["aria-labelledby"],
    "aria-invalid": invalid || undefined,
    disabled,
    onKeyDown,
  };

  const chips = multiple
    ? selected.map((v) => (
        <span key={v} className={styles.chip}>
          {byValue.get(v)?.label ?? v}
          <button
            type="button"
            tabIndex={-1}
            disabled={disabled}
            aria-label={`Remove ${byValue.get(v)?.label ?? v}`}
            className={styles.chipRemove}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => onToggle(v)}
          >
            ×
          </button>
        </span>
      ))
    : null;

  const summary = multiple
    ? selected.length
      ? `${selected.length} selected`
      : ""
    : selectedLabel;

  // Keep the popup the same width as the control.
  useLayoutEffect(() => {
    if (open) floating.update();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible.length, open]);

  return (
    <div
      ref={setWrap}
      className={cx(styles.control, className)}
      data-open={open || undefined}
      data-invalid={invalid || undefined}
      data-disabled={disabled || undefined}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) doClose();
      }}
    >
      {searchable ? (
        // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- mousedown only keeps focus in the input
        <div className={styles.inputRow} onMouseDown={(e) => {
          if (e.target !== ctrl.current) {
            e.preventDefault();
            ctrl.current?.focus();
          }
        }}>
          {chips}
          <input
            {...common}
            ref={(el) => {
              ctrl.current = el;
            }}
            type="text"
            autoComplete="off"
            aria-autocomplete="list"
            className={styles.input}
            placeholder={selected.length && multiple ? undefined : placeholder}
            value={typing ? query : selectedLabel}
            onFocus={(e) => !multiple && e.currentTarget.select()}
            onClick={() => (open ? undefined : doOpen())}
            onChange={(e) => {
              setQuery(e.target.value);
              setTyping(true);
              const next = e.target.value ? options.filter((o) => filter(o, e.target.value)) : options;
              setActive(firstEnabled(next, 0, 1));
              setOpen(true);
            }}
          />
        </div>
      ) : (
        <button
          {...common}
          ref={(el) => {
            ctrl.current = el;
          }}
          type="button"
          className={styles.trigger}
          onClick={() => (open ? doClose() : doOpen())}
        >
          {summary ? (
            <span className={styles.value}>{summary}</span>
          ) : (
            <span className={cx(styles.value, styles.placeholder)}>{placeholder ?? ""}</span>
          )}
        </button>
      )}
      <button
        type="button"
        tabIndex={-1}
        aria-hidden="true"
        disabled={disabled}
        className={styles.chevron}
        onMouseDown={(e) => {
          e.preventDefault();
          ctrl.current?.focus();
          if (open) doClose();
          else doOpen();
        }}
      >
        ▾
      </button>
      {name && selected.map((v) => <input key={v} type="hidden" name={name} value={v} />)}
      {open &&
        portalTarget &&
        createPortal(
          // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- mousedown only keeps focus in the control
          <div
            ref={floating.ref}
            className={styles.popup}
            style={{ ...floating.style, width: wrap?.offsetWidth || undefined }}
            onMouseDown={(e) => e.preventDefault()}
          >
            <OptionList
              listId={listId}
              options={visible}
              active={active}
              isSelected={(o) => selectedSet.has(o.value)}
              multiple={multiple}
              rowHeight={rowHeight}
              maxHeight={maxListHeight}
              label={aria["aria-label"]}
              labelledBy={aria["aria-labelledby"]}
              onPick={pick}
              onHover={setActive}
            />
            {visible.length === 0 && (
              <div role="status" className={styles.empty}>
                {emptyText}
              </div>
            )}
          </div>,
          portalTarget,
        )}
    </div>
  );
}

/* ---------------------------------------------------------------- exports */

function useControllable<T>(value: T | undefined, defaultValue: T) {
  const [internal, setInternal] = useState(defaultValue);
  return [value !== undefined ? value : internal, setInternal] as const;
}

/** Single-select dropdown. Pass `searchable` for type-to-filter (or use {@link Combobox}). Virtualized. */
export function Select({ value, defaultValue = null, onValueChange, ...rest }: SelectProps) {
  const [current, setCurrent] = useControllable<string | null>(value, defaultValue);
  return (
    <Core
      {...rest}
      multiple={false}
      selected={current != null ? [current] : []}
      onToggle={(v) => {
        setCurrent(v);
        onValueChange?.(v);
      }}
    />
  );
}

/** Searchable single-select: an input that filters the (virtualized) option list. */
export function Combobox(props: SelectProps) {
  return <Select searchable {...props} />;
}

/** Searchable multi-select with removable chips. */
export function MultiSelect({ values, defaultValues = [], onValuesChange, searchable = true, ...rest }: MultiSelectProps) {
  const [current, setCurrent] = useControllable<string[]>(values, defaultValues);
  const commit = (next: string[]) => {
    setCurrent(next);
    onValuesChange?.(next);
  };
  return (
    <Core
      {...rest}
      searchable={searchable}
      multiple
      selected={current}
      onToggle={(v) => commit(current.includes(v) ? current.filter((x) => x !== v) : [...current, v])}
      onRemoveLast={() => current.length && commit(current.slice(0, -1))}
    />
  );
}
