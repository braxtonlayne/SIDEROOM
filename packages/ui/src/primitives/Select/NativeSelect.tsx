import { forwardRef, type SelectHTMLAttributes } from 'react';
import styles from './NativeSelect.module.css';

export interface NativeSelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  /** Sets `aria-invalid` and the invalid look. */
  invalid?: boolean;
  /**
   * `fill` (default): the field fills its parent. `content`: as wide as its content instead of filling its parent,
   * for a select in an inline-flex label, a flex-wrap row or a chip (D-971). Not passed on to the `<select>`.
   */
  fit?: 'fill' | 'content';
}

/**
 * The browser's own `<select>` in the kit's field look (D-950): tokens, the kit's focus ring, disabled and invalid
 * states, a 44 px height on a coarse pointer, and a chevron drawn in CSS at the inline end. For short fixed lists;
 * the kit's `Select` stays for long or searchable ones. Every native prop passes through; `className` goes on the
 * `<select>`. `multiple` and `size` get no chevron.
 */
export const NativeSelect = forwardRef<HTMLSelectElement, NativeSelectProps>(function NativeSelect(
  { invalid, fit = 'fill', className, multiple, size, children, ...rest },
  ref,
) {
  const listbox = multiple === true || (size !== undefined && size > 1);
  const content = fit === 'content';
  const cls = [
    styles['select'],
    listbox ? styles['listbox'] : '',
    content ? styles['selectFit'] : '',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');
  const select = (
    <select
      {...rest}
      ref={ref}
      multiple={multiple}
      size={size}
      className={cls}
      aria-invalid={invalid ? true : rest['aria-invalid']}
    >
      {children}
    </select>
  );
  if (listbox) return select;
  return (
    <span className={content ? `${styles['wrap']} ${styles['wrapFit']}` : styles['wrap']}>
      {select}
    </span>
  );
});
