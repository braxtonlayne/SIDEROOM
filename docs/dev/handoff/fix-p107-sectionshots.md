# Handoff: fix/p107-sectionshots (wave 107, shots builder of pod sections)

## Clock
Started 18:38. Spec committed 18:44, base round 18:45-18:56 (jobs 0174-0176), merged library (75e27700) and cinema (b023bade) at 18:44, build 0187, first after round 19:07-19:28 (jobs 0196-0198; the runner queue was 20 minutes deep). FAULTS written 19:34.

## FAULTS (first after round; one cause, found in every case)
A `::after` tap area is positioned against the PADDING box, so a control with a 1 px border gets `border` less reach than the inset says. With `inset-block: -Npx` the area reaches N-1 px past the border box. The hit test's point 21 px from the centre then lands in the gap on the side where the rounding falls (below), the above point passes by a pixel. Every bordered control built this way still fails at 375 with the taker a container (never another control). Cure asked: (b) a named fix, one px more on each, inside the builder's own rule, may move nothing (a pseudo-element only).
- **library** `library.css` `.lib-genre-chip::after` (border 1px, box 38): `inset-block: -3px` -> `-4px`. 10 chips still fail (below: `ul` / `label` "Add a genre").
- **library** `library.css` `.lib-frame .lib-btn::after` (border 1px, box 40): `-2px` -> `-3px`. "Start reading" (series, review), "Browse the library" (me), "Browse series" (list, twice) still fail (below: `div`).
- **cinema** `tv.css` `.tv-chip::after` (border 1px, box 36, 8 px gaps): `-4px` -> `-5px`. 8 chips still fail (All, Anime & animation, Movies, Series, Custom, Favorites, Channel time, Now; below: `div`). Only `.tv-chip`: `.tv-nav-btn`, `.tv-fav` pass (no border).
- **library** `arcade.css` `.arc-cta::after` (border 1px, box 34): `-5px` -> `-6px`. "Open the emulator" still fails (below: `div`).
Not faults, read: `.lib-search-input` and `.emu-search` pass the contract at rest, hover and focus (see FIELDS) once the check reads the side that draws the line (the spec was wrong first; fixed). `.emu-search` has no placeholder attribute, so no placeholder rule applies: nothing to cure.

## Not this pod's (listed apart)
FilterChips `.chip`: Browse kind chips 7, My library chips 5, Study file-type chips 6, Cinema Browse Drama chip, Room selection and its chevron: still fail in the after phase as in the base (kitbits, pod staff). Drawer `.close` (Library and Cinema filters): same. Accepted by library: `.lib-chip-list a` "Drama" tag (16 px, 8 px gap), 3 rows.
