import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { cx } from '../../util/cx';
import styles from './Tabs.module.css';

export interface TabItem {
  id: string;
  label: ReactNode;
  content: ReactNode;
  disabled?: boolean;
}

export interface TabsProps {
  items: TabItem[];
  /** Accessible name for the tab list. */
  label: string;
  value?: string;
  defaultValue?: string;
  onValueChange?: (id: string) => void;
  variant?: 'underline' | 'pill';
  className?: string;
}

/** WAI-ARIA tabs with automatic activation, roving tabindex, Arrow/Home/End keys. */
export function Tabs({
  items,
  label,
  value,
  defaultValue,
  onValueChange,
  variant = 'underline',
  className,
}: TabsProps) {
  const base = useId();
  const firstEnabled = items.find((i) => !i.disabled)?.id;
  const [internal, setInternal] = useState(defaultValue ?? firstEnabled);
  const selected = value ?? internal;
  const refs = useRef(new Map<string, HTMLButtonElement>());
  const listRef = useRef<HTMLDivElement>(null);

  // Cue for tabs hidden past an edge (D-812): data-overflow is start, end, both, or absent.
  // scrollLeft is negative in right-to-left pages in Chromium, so its absolute value is used.
  const measure = () => {
    const el = listRef.current;
    if (!el) return;
    const pos = Math.abs(el.scrollLeft);
    const hasStart = pos > 1;
    const hasEnd = pos + el.clientWidth < el.scrollWidth - 1;
    const v = hasStart && hasEnd ? 'both' : hasStart ? 'start' : hasEnd ? 'end' : null;
    if (v) el.setAttribute('data-overflow', v);
    else el.removeAttribute('data-overflow');
  };

  useEffect(() => {
    const el = listRef.current;
    if (!el) return;
    // Bring the selected tab into view inside the list only (never scrolls the page).
    const tab = selected ? refs.current.get(selected) : undefined;
    if (tab) {
      const l = el.getBoundingClientRect();
      const t = tab.getBoundingClientRect();
      if (t.left < l.left) el.scrollLeft += t.left - l.left;
      else if (t.right > l.right) el.scrollLeft += t.right - l.right;
    }
    measure();
  }, [selected]);

  useEffect(() => {
    measure();
    const el = listRef.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [items]);

  const select = (id: string, focus: boolean) => {
    if (value === undefined) setInternal(id);
    onValueChange?.(id);
    if (focus) refs.current.get(id)?.focus();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const enabled = items.filter((i) => !i.disabled);
    const idx = enabled.findIndex((i) => i.id === selected);
    let next: TabItem | undefined;
    switch (e.key) {
      case 'ArrowRight':
        next = enabled[(idx + 1) % enabled.length];
        break;
      case 'ArrowLeft':
        next = enabled[(idx - 1 + enabled.length) % enabled.length];
        break;
      case 'Home':
        next = enabled[0];
        break;
      case 'End':
        next = enabled[enabled.length - 1];
        break;
      default:
        return;
    }
    e.preventDefault();
    if (next) select(next.id, true);
  };

  const active = items.find((i) => i.id === selected);

  return (
    <div className={cx(styles.tabs, className)}>
      {/* eslint-disable-next-line jsx-a11y/interactive-supports-focus -- focus is managed by the child tabs (roving tabindex) */}
      <div
        role="tablist"
        aria-label={label}
        aria-orientation="horizontal"
        ref={listRef}
        className={cx(styles.list, styles[variant])}
        onScroll={measure}
        onKeyDown={onKeyDown}
      >
        {items.map((item) => (
          <button
            key={item.id}
            ref={(n) => {
              if (n) refs.current.set(item.id, n);
              else refs.current.delete(item.id);
            }}
            type="button"
            role="tab"
            id={`${base}-tab-${item.id}`}
            aria-selected={item.id === selected}
            aria-controls={`${base}-panel-${item.id}`}
            tabIndex={item.id === selected ? 0 : -1}
            disabled={item.disabled}
            className={styles.tab}
            onClick={() => select(item.id, false)}
          >
            {item.label}
          </button>
        ))}
      </div>
      {active && (
        <div
          role="tabpanel"
          id={`${base}-panel-${active.id}`}
          aria-labelledby={`${base}-tab-${active.id}`}
          tabIndex={0}
          className={styles.panel}
        >
          {active.content}
        </div>
      )}
    </div>
  );
}
