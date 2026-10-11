/**
 * CSS-drawn stand-in for the door-ajar photograph. Operators replace it by
 * setting `--ent-door-image` (a CSS url()) on :root; see entrance.css.
 */
export function Door({ className }: { className?: string }) {
  return (
    <div className={className} data-testid="door" aria-hidden="true">
      <div className="ent-door-art">
        <div className="ent-door-leaf" />
        <div className="ent-door-light" />
      </div>
    </div>
  );
}
