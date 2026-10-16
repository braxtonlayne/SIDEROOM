/**
 * The door-ajar photograph (`public/entrance/door.jpg`). Operators replace the file, or set
 * `--ent-door-image` (a CSS url()) on :root; see entrance.css. A staff-uploaded hero image
 * (`GET /sr/entrance/hero`, H-41) takes the slot when there is one; `alt` is the operator's
 * `hero-alt` text, and without it the picture is decorative.
 */
export function Door({
  className,
  image,
}: {
  className?: string;
  image?: { src: string; alt?: string };
}) {
  if (image) {
    return (
      <div className={className} data-testid="door" data-hero="true">
        <img
          className="ent-door-img"
          src={image.src}
          alt={image.alt ?? ''}
          decoding="async"
          fetchPriority="low"
        />
      </div>
    );
  }
  return (
    <div className={className} data-testid="door" aria-hidden="true">
      <div className="ent-door-art">
        <div className="ent-door-leaf" />
        <div className="ent-door-light" />
      </div>
    </div>
  );
}
