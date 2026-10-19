# Third-party notices

Hand-maintained (D-287, D-297, D-309). This file lists the dependencies whose licence carries a condition that
attribution or the licence text must accompany the software. Permissive dependencies under plain MIT, Apache-2.0,
BSD or ISC terms are checked mechanically (`cargo deny`, and the licence allow-list in `web.yml`) and are not
repeated here.

The release image ships this file at `/usr/share/doc/sideroom/THIRD_PARTY_NOTICES.md`; the web app serves a
byte-identical copy at `/THIRD_PARTY_NOTICES.md` (linked from `/help`). `apps/web/public/THIRD_PARTY_NOTICES.md`
is a build-time copy, not a second source: edit this file and run `node scripts/check-notices-copy.mjs --write`.

## RARLAB UnRAR (rar / cbr reading)

<!--
  Why this is here: `cargo deny` trusts the `unrar_sys` crate's manifest, which says `MIT OR Apache-2.0`, and
  cannot see the RARLAB licence of the vendored C++ source (D-170, D-287). The licence below is copied unmodified
  from `vendor/unrar/license.txt` of `unrar_sys` 0.5.8 in the cargo registry. SIDE ROOM only decompresses
  archives (behind `library.ingest.rar_enabled`), never creates them, and does not modify the UnRAR source. If
  the `unrar` / `unrar_sys` version changes, re-copy the file.
-->

Used through the `unrar` / `unrar_sys` crates (vendored UnRAR source, unmodified).

```text
 ******    *****   ******   UnRAR - free utility for RAR archives
 **   **  **   **  **   **  ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
 ******   *******  ******    License for use and distribution of
 **   **  **   **  **   **   ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
 **   **  **   **  **   **         FREE portable version
                                   ~~~~~~~~~~~~~~~~~~~~~

      The source code of UnRAR utility is freeware. This means:

   1. All copyrights to RAR and the utility UnRAR are exclusively
      owned by the author - Alexander Roshal.

   2. UnRAR source code may be used in any software to handle
      RAR archives without limitations free of charge, but cannot be
      used to develop RAR (WinRAR) compatible archiver and to
      re-create RAR compression algorithm, which is proprietary.
      Distribution of modified UnRAR source code in separate form
      or as a part of other software is permitted, provided that
      full text of this paragraph, starting from "UnRAR source code"
      words, is included in license, or in documentation if license
      is not available, and in source code comments of resulting package.

   3. The UnRAR utility may be freely distributed. It is allowed
      to distribute UnRAR inside of other software packages.

   4. THE RAR ARCHIVER AND THE UnRAR UTILITY ARE DISTRIBUTED "AS IS".
      NO WARRANTY OF ANY KIND IS EXPRESSED OR IMPLIED.  YOU USE AT
      YOUR OWN RISK. THE AUTHOR WILL NOT BE LIABLE FOR DATA LOSS,
      DAMAGES, LOSS OF PROFITS OR ANY OTHER KIND OF LOSS WHILE USING
      OR MISUSING THIS SOFTWARE.

   5. Installing and using the UnRAR utility signifies acceptance of
      these terms and conditions of the license.

   6. If you don't agree with terms of the license you must remove
      UnRAR files from your storage devices and cease to use the
      utility.

      Thank you for your interest in RAR and UnRAR.


                                            Alexander L. Roshal
```

## JASSUB (ASS / SSA subtitle rendering), pending

<!-- Pending: the JASSUB branch (M-08) confirms the exact versions and licence texts when it lands. -->

- `jassub` 2.5.16, LGPL-2.1-or-later. Loaded as its own chunk, with its worker script and `.wasm` files served as
  separate, unminified files so the library can be replaced (LGPL-2.1 section 6). Source: the `jassub` package on
  npm, https://github.com/ThaUnknown/jassub. Full licence text: to be added by the JASSUB branch.
- libass, ISC licence (compiled into the JASSUB `.wasm`). Text: to be added by the JASSUB branch.
- FreeType, FreeType Licence (FTL) (compiled into the JASSUB `.wasm`). Text: to be added by the JASSUB branch.
- fribidi, LGPL-2.1 (compiled into the JASSUB `.wasm`). Text: to be added by the JASSUB branch.
- JASSUB default font: name and licence to be confirmed by the JASSUB branch.

## rvfc-polyfill, pending

<!-- Pending: confirm version and copyright line when the JASSUB branch adds the dependency (D-309). -->

- `rvfc-polyfill`, GPL-3.0 (a dependency of `jassub`, bundled so browsers without `requestVideoFrameCallback`
  still get ASS typesetting; D-309). GPL obligations attach to distribution: serving the web bundle to people outside
  the operator counts as distributing it, so revisit this entry before the instance is opened up. Source:
  https://github.com/ThaUnknown/rvfc-polyfill. Full licence text: to be added by the JASSUB branch.
