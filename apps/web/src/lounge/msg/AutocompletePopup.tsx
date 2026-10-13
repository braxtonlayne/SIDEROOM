import type { Suggestion } from './autocomplete';

/** Suggestion list above the composer. Keyboard handling lives in the composer; this only renders. */
export function AutocompletePopup({
  id,
  suggestions,
  active,
  title,
  onPick,
  onHover,
}: {
  id: string;
  suggestions: Suggestion[];
  active: number;
  title: string;
  onPick: (s: Suggestion) => void;
  onHover: (i: number) => void;
}) {
  return (
    <div className="lng-ac" data-testid="autocomplete">
      <div className="lng-ac-title">{title}</div>
      <ul id={id} role="listbox" aria-label={title} className="lng-ac-list">
        {suggestions.map((s, i) => (
          <li
            key={s.key}
            id={`${id}-${i}`}
            role="option"
            aria-selected={i === active}
            className="lng-ac-row"
            data-active={i === active || undefined}
            // Mouse-down keeps focus in the textarea.
            onMouseDown={(e) => {
              e.preventDefault();
              onPick(s);
            }}
            onMouseEnter={() => onHover(i)}
          >
            {s.color && (
              <span className="lng-ac-dot" style={{ background: s.color }} aria-hidden="true" />
            )}
            <span className="lng-ac-label">{s.label}</span>
            {s.hint && <span className="lng-ac-hint">{s.hint}</span>}
          </li>
        ))}
      </ul>
    </div>
  );
}
