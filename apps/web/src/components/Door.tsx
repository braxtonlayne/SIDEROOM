/**
 * The door-ajar photograph (`public/entrance/door.jpg`). Operators replace the file, or set
 * `--ent-door-image` (a CSS url()) on :root; see entrance.css.
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
