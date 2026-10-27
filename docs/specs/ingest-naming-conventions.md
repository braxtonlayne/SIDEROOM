# Spec — Ingest Naming Conventions

Phases 4–5 · Decision: D-083, D-084.

These conventions let the importer build structure and metadata automatically. They are shown in-app (an "Naming guide" link beside every upload control) and in this doc. Files that don't match still upload. The user just fills in the details by hand.

General rules: case-insensitive; separators `_`, `.`, ` `, or `-` are interchangeable; numbers may be zero-padded; text in `[brackets]` or `(parentheses)` is treated as tags (release group, year, quality) and ignored for numbering.

## 1. Cinema

### 1.1 Movies
```
<Title> (<Year>)/<Title> (<Year>).<ext>
<Title> (<Year>) - <Variant>.<ext>          → alternate cut, e.g. "Director's Cut"
<Title> (<Year>).<lang>[.forced|.sdh].srt   → subtitles beside the video
```
Example: `Akira (1988)/Akira (1988).mkv`, `Akira (1988).en.srt`

### 1.2 Series & anime
```
<Show> (<Year>)/Season <NN>/<Show> - s<NN>e<NN>[-e<NN>] - <Episode Title>.<ext>
```
Accepted alternatives (normalized by the parser):
`season_<N>_episode_<N>.<ext>` (the owner's suggested form), `S01E05`, `1x05`, `Season 1 Episode 5`, `<Show> - 05` (absolute numbering, common for anime: maps to season 1 unless an absolute-order mapping is chosen), `E05` inside a `Season 01` folder, `Specials/` or `Season 00` for specials, `-e<NN>` for multi-episode files.

Example tree:
```
Cowboy Bebop (1998)/
  Season 01/
    Cowboy Bebop - s01e01 - Asteroid Blues.mkv
    Cowboy Bebop - s01e01 - Asteroid Blues.en.ass
  Specials/
    Cowboy Bebop - s00e01 - Session XX.mkv
  poster.jpg   backdrop.jpg   logo.png
```

### 1.3 Containers & codecs accepted
Video: MKV, MP4, M4V, MOV, AVI, WebM, TS, MPEG, WMV (anything FFmpeg can decode). Subtitles: SRT, ASS/SSA (rendered via the `subtitles` feature → converted to WebVTT, with ASS styling optionally preserved via JASSUB), embedded tracks extracted. Audio: all embedded tracks preserved and labeled by language tag.

### 1.4 Artwork files (optional, beside the media)
`poster.(jpg|png|webp)`, `backdrop.*`, `logo.*` (transparent PNG), `season<NN>-poster.*`, `<episode file name>-thumb.*`.

## 2. Library

### 2.1 Folder of images
```
<Series Title>/
  [Vol <NN>/]                         optional volume level
    Chapter <NNN>[ - <Chapter Title>]/
      page_001.jpg  page_002.png  page_003.webp …
```
- Page files: `page_<N>.<jpg|jpeg|png|webp|gif>` (D-084). AVIF pages are not accepted (D-169): an archive containing an `.avif` file (or a file whose content is AVIF) is rejected, and the error lists the file names; convert them to JPEG, PNG or WebP first. Also accepted: bare numbers (`001.jpg`) and any natural-sorted names (with a warning). Double-page spreads: suffix `_spread` or width > 1.3 × height is auto-detected.
- Chapter folder alternatives: `Ch.<N>`, `c<N>`, `Chapter <N.5>` (decimals allowed for extras).
- Everything is converted to WebP on the server (lossy quality 85 via libwebp, D-168; a PNG source is also encoded lossless and the smaller file is kept, which favours line art), with the original dimensions recorded. libwebp cannot encode a side longer than 16,383 px, so a page taller or wider than that is refused: split very long webtoon strips first.

### 2.2 Archives
`<Series> v<NN> c<NNN>[ - <Title>] [<Group>].cbz` (also `.zip`, `.cbr`, `.rar`, `.cb7`, `.7z`). The format is detected from the file contents, not the extension. ZIP/CBZ and 7z/CB7 are always read; RAR/CBR is read unless the operator turns off the platform setting `library.ingest.rar_enabled` (default on, D-170), in which case a RAR upload is rejected as an unsupported format. Every format is held to the same limits (entry count, per-file size, total uncompressed size, compression ratio, no nested archives, no `..` or absolute names, no encrypted or multi-volume archives; symlinks are skipped). Pages inside follow §2.1. `ComicInfo.xml` inside the archive is read when present (title, number, writer, summary, language, manga direction `Manga=YesAndRightToLeft`).

### 2.3 Documents
`.pdf` (the original is kept for download; rasterizing pages for image-mode reading waits for an approved pdfium build, see open-questions), `.epub` (validated, title/authors/language/cover read from the package document, kept as-is for the reflowable reader). Encrypted PDFs and EPUBs are refused.

### 2.4 Series metadata file (optional)
`series.json` at the series root:
```json
{ "title": "…", "alt_titles": ["…"], "kind": "manga", "language": "en",
  "edition": "Official English", "direction": "rtl", "authors": ["…"],
  "status": "ongoing", "year": 2020, "genres": ["Fantasy"], "age_restricted": false }
```
Cinema equivalent: `title.json` with the matching fields.

## 3. Parser
`sr-media::naming` and `sr-reader::naming` (pure Rust, with a fixture test suite ≥ 300 real-world names). There's a **tester** in Admin → Ingest Sources and in the upload dialog: paste names and see the parsed result.

## 4. Folders in a storage mount (BYOS-CAT-001)
The `storage.index` job reads a member's or a server's bucket (a "mount") and builds shelves from its folders. Nothing is moved or renamed; the bucket stays the member's. The first folder under the mount's prefix picks the section (names are case-insensitive); anything outside them is placed by its extension.

| Folder | Section | Layout it reads |
|---|---|---|
| `Movies/` (`Films/`) | Cinema, films | `Movies/Alien (1979)/Alien.mp4` or `Movies/Alien (1979).mp4`; `Director's Cut` and the other cuts of section 1 apply |
| `Series/` (`Shows/`, `TV/`) | Cinema, series | `Series/<show>/Season 01/<show> S01E02.mkv`; a file without an episode number is left out |
| `Anime/` | Cinema, anime | as series, absolute numbering (`Show - 05`) allowed; a file without an episode number is a film; titles carry the tag `anime` |
| `Books/` | Library, books | `Books/Dune.epub`, `Books/<author>/<title>.pdf` (EPUB, PDF) |
| `Comics/` | Library, comics | `Comics/<series>/<series> #012.cbz` (CBZ; CBR only when `library.ingest.rar_enabled`) |
| `Manga/` | Library, manga | `Manga/<series>/<series> v03.cbz` or `c112.cbz` (a chapter number beats a volume number) |
| `Music/` | Music | `Music/<artist>/<album> (year)/03 - Title.flac`; `Disc 2/` and `CD2/` folders, `1-07 Title`, and flat `Artist - Title.mp3` are read |

Video names go through `sr-media::naming` (S01E02, year, group and quality tags), so a title is spelled the same way here as in a submission. A `cover.jpg`, `folder.jpg` or `poster.jpg` in a title, series or album folder becomes its artwork (`backdrop.jpg` and `fanart.jpg` its backdrop). Loose chapters without a number follow the numbered ones of their series, in key order.

### 4.1 Folder sidecar (optional)
`.sideroom.json` in any folder overrides what the names say for everything below it (the nearest folder wins per field):
```json
{ "title": "Cowboy Bebop", "year": 1998, "kind": "anime", "tags": ["classic"],
  "artist": "…", "album": "…", "ignore": false }
```
`kind` is one of `movie`, `series`, `anime`, `book`, `comic`, `manga`, `music` and forces the section; `ignore` skips the folder. A malformed file changes nothing.

### 4.2 Edits survive a re-index
Anything a member changes in the metadata editor (title, year, kind, tags, genres, synopsis, artwork, an episode's number or title) becomes an override recorded on the row; a re-index rewrites only the fields that were not edited. A row whose files disappear from the bucket is hidden, and returns, with its edits, when the files come back. Two files for one episode number keep the first indexed; a chapter number already taken moves to the next free one (the editor fixes it).

### 4.3 Playing from a mount
MP4 (H.264 or AV1 and AAC), WebM, MP3, FLAC, Ogg and the other web-native containers play from a presigned URL with range requests; MKV, HEVC and the like show "Convert to play" until the converter ships (wave 29). Fetched details carry the metadata provider's attribution text on the row and wherever it is shown.
