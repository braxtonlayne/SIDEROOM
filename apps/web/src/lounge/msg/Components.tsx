import { useToast } from '@sideroom/ui';
import type { RawComponent } from '../../store/types';
import { isHttpUrl } from './format';

export type { RawComponent };

const STYLE = ['', 'primary', 'secondary', 'success', 'danger', 'link'] as const;

/**
 * Buttons and select menus on app messages (LNG-MSGUI-022). Link buttons open their URL; everything else
 * reports "This interaction failed" because the interaction endpoint is not part of the server yet.
 */
export function ComponentRows({ rows }: { rows: RawComponent[] }) {
  const toast = useToast();
  const fail = () =>
    toast.show({ title: 'This interaction failed', variant: 'danger', duration: 3000 });
  return (
    <div className="lng-components">
      {rows
        .filter((r) => r.type === 1)
        .slice(0, 5)
        .map((row, i) => (
          <div key={i} className="lng-component-row" role="group" aria-label="Message actions">
            {(row.components ?? []).slice(0, 5).map((c, j) => {
              if (c.type === 2) {
                const style = STYLE[c.style ?? 1] ?? 'primary';
                if (style === 'link') {
                  return isHttpUrl(c.url) ? (
                    <a
                      key={j}
                      className="lng-cbtn"
                      data-style="link"
                      href={c.url}
                      target="_blank"
                      rel="noreferrer noopener"
                      aria-disabled={c.disabled || undefined}
                    >
                      {c.label ?? 'Link'}
                    </a>
                  ) : null;
                }
                return (
                  <button
                    key={j}
                    type="button"
                    className="lng-cbtn"
                    data-style={style}
                    disabled={c.disabled}
                    onClick={fail}
                  >
                    {c.label ?? 'Button'}
                  </button>
                );
              }
              if (c.type === 3) {
                return (
                  <select
                    key={j}
                    className="lng-cselect"
                    aria-label={c.placeholder ?? 'Select an option'}
                    disabled={c.disabled}
                    value=""
                    onChange={fail}
                  >
                    <option value="" disabled>
                      {c.placeholder ?? 'Select an option'}
                    </option>
                    {(c.options ?? []).slice(0, 25).map((o) => (
                      <option key={o.value} value={o.value}>
                        {o.label}
                      </option>
                    ))}
                  </select>
                );
              }
              return null;
            })}
          </div>
        ))}
    </div>
  );
}
