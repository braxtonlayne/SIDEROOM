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
- Page files: `page_<N>.<jpg|jpeg|png|webp|avif|gif>` (D-084). Also accepted: bare numbers (`001.jpg`) and any natural-sorted names (with a warning). Double-page spreads: suffix `_spread` or width > 1.3 × height is auto-detected.
- Chapter folder alternatives: `Ch.<N>`, `c<N>`, `Chapter <N.5>` (decimals allowed for extras).
- Everything is converted to WebP on the server (quality 85 default, lossless for PNG line art if smaller), with the original dimensions recorded.

### 2.2 Archives
`<Series> v<NN> c<NNN>[ - <Title>] [<Group>].cbz` (also `.zip`, `.cbr`, `.rar`, `.cb7`, `.7z`). Pages inside follow §2.1. `ComicInfo.xml` inside the archive is read when present (title, number, writer, summary, language, manga direction `Manga=YesAndRightToLeft`).

### 2.3 Documents
`.pdf` (rasterized to pages for image-mode reading, with the original kept for download), `.epub` (kept as-is, reflowable reader).

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
