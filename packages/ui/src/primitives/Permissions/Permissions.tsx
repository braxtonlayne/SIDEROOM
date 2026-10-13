import { useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { cx } from "../../util/cx";
import { Switch } from "../Controls/Controls";
import styles from "./Permissions.module.css";

export type TriState = "allow" | "neutral" | "deny";

export interface TriStateToggleProps {
  /** Accessible name, e.g. "Send Messages". */
  label: string;
  value: TriState;
  onChange: (value: TriState) => void;
  disabled?: boolean;
  className?: string;
}

const ORDER: TriState[] = ["deny", "neutral", "allow"];
const TEXT: Record<TriState, string> = { deny: "Deny", neutral: "Neutral (inherit)", allow: "Allow" };
const GLYPH: Record<TriState, string> = { deny: "✕", neutral: "/", allow: "✓" };

/**
 * Allow / neutral / deny selector used by channel permission overwrites. A radio group with a roving tabindex:
 * the arrow keys move and select, Home and End jump to deny and allow.
 */
export function TriStateToggle({ label, value, onChange, disabled, className }: TriStateToggleProps) {
  const refs = useRef(new Map<TriState, HTMLButtonElement>());
  const move = (to: TriState) => {
    onChange(to);
    refs.current.get(to)?.focus();
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (disabled) return;
    const i = ORDER.indexOf(value);
    let next: TriState | undefined;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") next = ORDER[Math.min(ORDER.length - 1, i + 1)];
    else if (e.key === "ArrowLeft" || e.key === "ArrowUp") next = ORDER[Math.max(0, i - 1)];
    else if (e.key === "Home") next = ORDER[0];
    else if (e.key === "End") next = ORDER[ORDER.length - 1];
    if (!next) return;
    e.preventDefault();
    move(next);
  };
  return (
    // The radiogroup is a composite widget; its keys are handled here.
    // eslint-disable-next-line jsx-a11y/interactive-supports-focus
    <div role="radiogroup" aria-label={label} className={cx(styles.tri, className)} onKeyDown={onKeyDown} data-disabled={disabled || undefined}>
      {ORDER.map((s) => (
        <button
          key={s}
          type="button"
          role="radio"
          aria-checked={value === s}
          aria-label={`${label}: ${TEXT[s]}`}
          title={TEXT[s]}
          disabled={disabled}
          tabIndex={value === s ? 0 : -1}
          data-state={s}
          className={styles.triBtn}
          ref={(el) => {
            if (el) refs.current.set(s, el);
            else refs.current.delete(s);
          }}
          onClick={() => onChange(s)}
        >
          <span aria-hidden="true">{GLYPH[s]}</span>
        </button>
      ))}
    </div>
  );
}

export interface PermissionDef {
  id: string;
  label: string;
  description?: string;
  /** Highlighted as dangerous (Administrator and friends). */
  danger?: boolean;
}

export interface PermissionGroupDef {
  id: string;
  label: string;
  items: PermissionDef[];
}

export interface PermissionMatrixProps {
  groups: PermissionGroupDef[];
  /** Missing ids count as neutral. */
  values: Record<string, TriState>;
  onChange: (id: string, value: TriState) => void;
  /** "binary" shows a switch (allow / neutral); "tristate" shows allow / neutral / deny. */
  mode?: "binary" | "tristate";
  /** Returns a reason string to lock a row (shown under it), or undefined when it is editable. */
  lockedReason?: (id: string) => string | undefined;
  /** Hide permissions that are not relevant (e.g. voice bits on a text channel). */
  hidden?: (id: string) => boolean;
  searchable?: boolean;
  searchLabel?: string;
  emptyText?: string;
  className?: string;
  /** Rendered above the groups next to the search box (e.g. a "Clear permissions" button). */
  toolbar?: ReactNode;
}

/** Grouped, searchable permission list shared by role and channel-overwrite editors. */
export function PermissionMatrix({
  groups,
  values,
  onChange,
  mode = "binary",
  lockedReason,
  hidden,
  searchable = true,
  searchLabel = "Search permissions",
  emptyText = "No permissions match your search.",
  className,
  toolbar,
}: PermissionMatrixProps) {
  const [query, setQuery] = useState("");
  const searchId = useId();
  const q = query.trim().toLowerCase();
  const visible = useMemo(
    () =>
      groups
        .map((g) => ({
          ...g,
          items: g.items.filter(
            (p) =>
              !hidden?.(p.id) &&
              (!q || p.label.toLowerCase().includes(q) || (p.description ?? "").toLowerCase().includes(q)),
          ),
        }))
        .filter((g) => g.items.length > 0),
    [groups, hidden, q],
  );
  return (
    <div className={cx(styles.matrix, className)}>
      {(searchable || toolbar) && (
        <div className={styles.toolbar}>
          {searchable && (
            <>
              <label htmlFor={searchId} className={styles.srOnly}>
                {searchLabel}
              </label>
              <input
                id={searchId}
                type="search"
                className={styles.search}
                placeholder={searchLabel}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </>
          )}
          {toolbar}
        </div>
      )}
      {visible.length === 0 && <p className={styles.empty}>{emptyText}</p>}
      {visible.map((g) => (
        <section key={g.id} aria-labelledby={`${searchId}-${g.id}`} className={styles.group}>
          <h3 id={`${searchId}-${g.id}`} className={styles.groupTitle}>
            {g.label}
          </h3>
          <ul className={styles.list}>
            {g.items.map((p) => {
              const lock = lockedReason?.(p.id);
              const state = values[p.id] ?? "neutral";
              return (
                <li key={p.id} className={styles.row} data-danger={p.danger || undefined} data-locked={lock ? true : undefined}>
                  <div className={styles.text}>
                    {mode === "binary" ? (
                      <Switch
                        label={p.label}
                        description={[p.description, lock].filter(Boolean).join(" ") || undefined}
                        checked={state === "allow"}
                        disabled={!!lock}
                        onChange={(e) => onChange(p.id, e.target.checked ? "allow" : "neutral")}
                      />
                    ) : (
                      <>
                        <span className={styles.label}>{p.label}</span>
                        {p.description && <span className={styles.desc}>{p.description}</span>}
                        {lock && <span className={styles.desc}>{lock}</span>}
                      </>
                    )}
                  </div>
                  {mode === "tristate" && (
                    <TriStateToggle label={p.label} value={state} disabled={!!lock} onChange={(v) => onChange(p.id, v)} />
                  )}
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}
