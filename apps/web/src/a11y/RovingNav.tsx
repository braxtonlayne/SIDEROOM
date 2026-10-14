import { useRef, type ComponentPropsWithoutRef } from 'react';
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
