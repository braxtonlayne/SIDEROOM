# Wave 103 handoff: libshots (shots builder, pod library; SEC-AUD-465, 468, 469; D-972, D-974)

Status at 12:55: base round and after round run once each (spec passes both phases on the merged tree except the
FAULT below, which the spec's new "text is cut" check now names). Full handoff follows when the shots are all read.

## FAULTS
1. **reader, `apps/web/src/library/reader/Reader.tsx` + `library.css` (`.lib-chapter-select`), the chapter picker: the type
   size grew from 13.33 px (raw select default) to 16 px (the kit's `font: inherit` from the bar), so the selected text is cut
   with an ellipsis.** Figures (1024 and 375, both 220 px wide, the cap): selected text "Chapter 1: p103-lib first" was
   138.6 px at 13.33 px, is 170.1 px at 16 px (175.1 in the `lounge-light` run) against a text area of 170 px
   (220 - 2 border - 12 - 36), so it is cut (the shot `reader-bars-1024-sideroom.png` shows "Chapter 1: p103-lib fir...").
   Cure asked: **(b)** `font-size: 14px` on `label.lib-chapter-select` (the Library's field size, D-974; the kit select
   inherits; text about 149 px, fits with 21 px to spare). Never on `select` by element name, never on the wrapper.
   After the cure the picker needs a second after round (spec re-shot).
