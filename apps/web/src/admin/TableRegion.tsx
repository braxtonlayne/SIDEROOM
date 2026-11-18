// The one wrapper for an Admin table (SEC-AUD-441, D-949). While the table is wider than the wrapper the wrapper
// scrolls sideways, so it is a named region and a tab stop (the arrow keys then reach the last columns); while the
// table fits it is a plain wrapper and no stop. It measures itself: no list of widths.
import { useEffect, useRef, useState, type ReactNode } from 'react';

export function TableRegion({
  label,
  className,
  children,
}: {
  /** The table's own name (the string its `aria-label` or caption already uses). */
  label: string;
  /** Added after `adm-table-wrap`. */
  className?: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [overflows, setOverflows] = useState(false);

  useEffect(() => {
    const wrap = ref.current;
    if (!wrap || typeof ResizeObserver === 'undefined') return;
    const measure = () => setOverflows(wrap.scrollWidth > wrap.clientWidth);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(wrap);
    const inner = wrap.querySelector('table') ?? wrap.firstElementChild;
    if (inner) ro.observe(inner);
    return () => ro.disconnect();
  }, []);

  return (
    <div
      ref={ref}
      className={className ? `adm-table-wrap ${className}` : 'adm-table-wrap'}
      {...(overflows ? { role: 'region', 'aria-label': label, tabIndex: 0 } : {})}
    >
      {children}
    </div>
  );
}
