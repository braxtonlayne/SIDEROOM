import { useRef, type ComponentPropsWithoutRef, type ReactNode } from 'react';
import { useRovingTabindex, type RovingOptions } from './useRovingTabindex';

/** A `<nav>` whose items form one Tab stop with arrow-key movement (XC-A11Y-001). */
export function RovingNav({
  selector,
  orientation,
  ...rest
}: RovingOptions & ComponentPropsWithoutRef<'nav'>) {
  const ref = useRef<HTMLElement>(null);
  useRovingTabindex(ref, orientation ? { selector, orientation } : { selector });
  return <nav ref={ref} {...rest} />;
}

/**
 * A wrapper that makes the items inside it one Tab stop with arrow-key movement, without changing the layout
 * (`display: contents`). For lists whose markup is owned by a shared component (member list, card grids).
 */
export function RovingScope({
  selector,
  orientation,
  children,
}: RovingOptions & { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useRovingTabindex(ref, orientation ? { selector, orientation } : { selector });
  return (
    <div ref={ref} style={{ display: 'contents' }}>
      {children}
    </div>
  );
}
