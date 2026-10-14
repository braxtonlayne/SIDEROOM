# Spec — The Cinema (movies, series, anime)

Phase 5 · Theme `sideroom` · Mockups `cinema-discover.jpg`, `cinema-movies.jpg` · Decisions: D-075, D-080–D-086, D-089, D-120.

## 1. Navigation
Top bar: `SIDE ROOM / The lounge · **The cinema** · The library`. Cinema header: door mark + `SIDE ROOM` / `C I N E M A` · **Discover · Movies · Series · Anime ✦ · Live TV • · My list** · search · avatar (menu: Profile, My uploads, Upload, Requests, Settings, Back to Lounge). Footer: "Side Room *Cinema*" · "A little further from the ordinary." · About this library.

## 2. Pages

### 2.1 Discover (`/cinema`)
| Block | Behavior |
|---|---|
| **Hero ("TONIGHT'S FEATURE")** | Full-bleed backdrop, title in display serif (italic second line), year · runtime · rating badge · genre, synopsis, `▷ Play` (or `Resume 42:10`), `ⓘ More info`, `Watch trailer` if one exists, carousel index "01 / THE EVENING SELECTION" with auto-advance (pauses on hover). Staff-curated (Collections & Spotlight), falling back to trending |
| **Recently explored** | Your recently viewed or started titles |
| **A good place to start** | Curated row |
| **Animation. *Without limits.*** | Banner card with a fanned poster collage → Anime room |
| **The anime room** | Anime row with `ANIME` corner tag |
| **One more episode** | Series row (continue-watching first) |
| **For the love of cinema** | Films row |
| Generated rows | Continue watching · Trending (scope: Friends / People you know / Global) · Because you watched X · New this week · Friends are watching · Requests fulfilled |

Poster card: 2:3 art, hover lift + quick actions (▷, `+` My list, ✓ watched, rating), title, "2024 · Film" / "2022 · Series" / "1998 · Anime," and a progress bar if started.

### 2.2 Movies / Series / Anime (`/cinema/movies|series|anime`)
Per the Movies mockup: eyebrow "— THE SIDE ROOM LIBRARY," H1 "The cinema." (the Series and Anime pages use their own heading copy), subline "Films for a night well spent.", genre filter chips (All, Science fiction, Adventure, Fantasy, Drama, Thriller, Comedy, Horror…, single select + "More"), **"Room selection ▾"** dropdown = curated collections, title count ("10 titles"), grid of poster cards with `+`. Extra filters drawer: year range, rating, runtime, audio language, subtitles, age-restricted (if confirmed), sort.

### 2.3 Title detail (`/cinema/t/:id`)
Backdrop + logo or title, metadata row (year, runtime or seasons count, content rating, genres), synopsis, `▷ Play`/`Resume`, `+ My list`, rate, **Watch together** (start a watch party → choose a Lounge channel or create a private party link), share to Lounge, **Download** (Patrons only: rendition picker), trailer. Series: season selector, episode list (still, number, title, runtime, synopsis, progress, watched mark, "mark season watched"). Tabs: Episodes · Details (cast/crew text fields, studios, original title, audio/subtitle languages, variants such as "Director's Cut") · Reviews · Comments · More like this · Contributors.

### 2.4 My list (`/cinema/my-list`)
Saved titles + Continue watching + Watch history (pause/clear) + custom lists (shareable, visibility private/friends/everyone).

### 2.5 Requests, Upload, My uploads
See the submissions spec.

### 2.6 Live TV
See the cinema-live-tv spec.

## 3. Player
- hls.js (MSE) or native HLS (Safari/iOS). Adaptive 1080p/720p/480p ladder, manual quality override, data saver (cap 480p).
- Controls: play/pause, ±10s skip, scrubber with thumbnail sprites (generated during transcode: one frame every 10s, as a WebVTT sprite map), volume, playback speed 0.5–2×, audio track picker, subtitles (WebVTT + ASS via JASSUB, size/color/background styling), full screen, Picture-in-Picture, AirPlay/Remote Playback API, cast (Chromecast via the Web Sender SDK, optional), keyboard shortcuts (space, ←/→, ↑/↓, F, M, C, <, >).
- **Skip intro / skip credits** buttons from `chapters/markers` (set by reviewers or auto-detected later).
- **Next episode** card at credits with a 10s countdown. Autoplay toggle.
- Resume from `watch_progress` (saved every 10s and on pause/unload). Marked watched at ≥ 90%.
- "Are you still watching?" after 3 auto-played episodes.
- Watching presence → Lounge activity (D-120 privacy).
- Signed segment URLs expire after 6 hours and are refreshed transparently.

## 4. Ingest & transcoding (D-086)
1. Upload original (any size) → `video_assets.state = uploaded`.
2. Probe (`ffmpeg-next`): duration, streams, codecs, HDR flags, interlace detection, audio languages, embedded subtitles.
3. A low-res **review proxy** (480p, fast CPU encode or the first GPU job) lets reviewers watch before approval.
4. On approval: enqueue a **ladder job** → GPU worker produces:
   - H.264 High, CMAF/fMP4 HLS, 6s segments, 2s GOP, closed GOP aligned across rungs.
   - Renditions: 1080p ~6 Mbps, 720p ~3 Mbps, 480p ~1.5 Mbps (capped at the source resolution; **never above 1080p**). HDR → SDR tone-map (zscale/tonemap). Deinterlace when needed.
   - Audio: AAC-LC stereo 160 kbps per audio track (plus optional 5.1 AAC 384 kbps passthrough rung).
   - Subtitles → WebVTT. ASS also kept for styled rendering.
   - Thumbnail sprite sheet + VTT, poster frame candidates.
   - Segment index (durations) stored in `renditions.segment_index` for the Live TV stitcher.
5. Originals are kept (Storage Box or Garage cold bucket) for re-transcoding. The proxy is deleted after publishing.
6. Storage budget ≈ 4.7 GB per hour of content (doc 08).

## 5. Downloads (Patron, D-075)
Download = progressive MP4 remux of the chosen rendition (generated on demand by a job, cached for 7 days) or the original if staff enable "original downloads" per title. Links are signed and single-user, with a rate limit (e.g. 20 downloads/day).

## 6. Social
Ratings (1–10), reviews (spoiler toggle), episode comments, lists, trending scopes (Friends / Known / Global, same algorithm as the Library), "Friends are watching" row, contributor credits per movie and episode (D-074).

## 7. Acceptance criteria
- [ ] Discover and Movies match the mockups at desktop widths, with no "Board" link and no demo labels.
- [ ] A 4K HDR source becomes a 1080p SDR ladder. No rendition exceeds 1080p.
- [ ] Playback starts in < 2s on broadband, and switches quality without stalls on throttled networks (Playwright + network shaping).
- [ ] Resume position syncs across devices within 10s.
- [ ] Non-Patrons get 403 on download endpoints.
- [ ] Safari iOS (PWA) plays HLS with subtitles and resumes correctly.

## 8. HTTP API as built (catalog)

Owner: `p6-cinema-data` (crate `sr-cinema`, `sr-db::cinema_repo`, `sr-api::routes::cinema`). This section is the contract the web agents code against; it was written before the code and is kept in step with it. Later changes are listed at the end of the handoff (`docs/dev/handoff/feat-p6-cinema-data.md`). Playback manifests, assets, renditions and transcoding (CIN-ING) and watch progress, lists, ratings and the other social tables (CIN-SOC, CIN-MY) belong to other work and are not in this section.

Conventions (same as the Library): ids are Snowflakes and travel as decimal **strings**; timestamps are Unix **milliseconds**; every route needs a session (401 otherwise); errors use the standard `{code, message}` body; unknown, unpublished or merged-away objects are 404 (a merged title answers with its target).

### 8.1 Kinds and age restriction (D-184)

* `kind` is `movie`, `series` or `anime`. An `anime` title is episodic when it has seasons, otherwise it is a single film (playable through the title's own asset). Doc 04 listed four kinds (`anime_series`, `anime_movie`); the catalog uses three because the episode structure already tells them apart.
* Every account sees age-restricted titles everywhere (browse, search, rows, detail, seasons, episodes, artwork). `age=include|hide|only` on listings, default `include`; a member who set `cinema.hide_age_restricted: true` in the user settings blob (`{"cinema": {"hide_age_restricted": true}}`, default false) gets `hide` as the default. An explicit `age=` always wins. The same flag leaves restricted titles out of search suggestions.
* **Opening** a restricted title (the `/play` routes below, and the trailers' `asset_id`) answers **403 code 900011** `AGE_CONFIRMATION_REQUIRED` until `accounts.age_confirmed_at` is set (`POST /sr/age-gate {over_18:true}`, D-032); retry afterwards. Other cinema backends gate on the same rule through `CinemaService::resolve_play`.

### 8.2 Member routes (`/cinema/*`)

| Route | Result |
|---|---|
| `GET /cinema/discover` | `Discover {hero: Row, rows: Row[]}` |
| `GET /cinema/rows/{key}?limit=&...` | `Row` (`limit` 1..50, default 20; other query params go to provider rows, e.g. `scope=friends\|known\|global`) |
| `GET /cinema/collections?surface=` | `CollectionList {items: Collection[]}` (published, by `position`); `surface` is `discover\|movies\|series\|anime` |
| `GET /cinema/titles` | `TitlePage {items: Title[], total, next_cursor?}` browse, see 8.4 |
| `GET /cinema/facets?kind=` | `Facets` for the genre chips and the filters drawer |
| `GET /cinema/titles/{id}` | `TitleDetail {title, seasons, variants, trailers, markers}` |
| `GET /cinema/titles/{id}/episodes?season=&limit=&after=` | `EpisodeList {seasons, episodes, next_cursor?}` |
| `GET /cinema/episodes/{id}` | `Episode` |
| `GET /cinema/titles/{id}/markers`, `GET /cinema/episodes/{id}/markers` | `MarkerList {markers: Marker[]}` |
| `GET /cinema/titles/{id}/play?variant=` | `PlayTarget`, 403 `900011` for a restricted title without age confirmation |
| `GET /cinema/episodes/{id}/play` | `PlayTarget` (same gate) |
| `GET /cinema/search?q=&limit=&kind=` | `InstantSearch {titles, people, genres, tags}` (`limit` 1..50, default 8) |

#### Title

```
Title {
  id, kind, title, original_title?, alt_titles[], year?, runtime_min?, content_rating?,
  synopsis, tagline?, genres[], tags[], languages[], subtitle_languages[],
  cast[], directors[], writers[], studios[], crew[],      // free text lists (D-089)
  age_restricted, poster_url?, backdrop_url?, logo_url?,  // signed /cdn/cinema/... links
  external_ids: {imdb?, tmdb?, ...},
  season_count, episode_count,                            // published seasons/episodes
  has_trailer, playable,                                  // playable: something can be opened
  published, published_at_ms?, last_content_at_ms?, created_at_ms, updated_at_ms,
  asset_id?                                               // staff routes only
}
```

`RowItem` is a `Title` plus optional `episode_id`, `progress_percent` (0..100) and `reason` (row-provider extras), flattened into one object. `Season {id, title_id, number, name?, synopsis?, poster_url?, episode_count}`. `Episode {id, title_id, season_id, season_number, number, absolute_number?, title, synopsis, air_date? ("YYYY-MM-DD"), runtime_min?, still_url?, playable, published, published_at_ms?, asset_id?}`. `Variant {id, title_id, label, notes?, runtime_min?, playable, position, asset_id?}` (the title itself is the default cut; variants are the other cuts, for example "Director's Cut"). `Trailer {id, title_id, label, kind (trailer|teaser|clip), language?, playable, age_gated, asset_id?}` (`asset_id` is withheld for a restricted title until the account confirmed its age). `Marker {kind (intro|credits|recap), start_ms, end_ms}` (staff routes add `id` and `source`).

`PlayTarget {title_id, episode_id?, variant_id?, asset_id, title, episode_title?, season_number?, episode_number?, runtime_min?, age_restricted, markers[], next_episode_id?, previous_episode_id?}`. `asset_id` is what the ingest/playback routes take; `next_episode_id` follows season then episode order over published, playable episodes.

#### Rows

`Row {key, kind (curated|generated|provided), display (row|hero|banner), title, eyebrow?, subtitle?, collection_id?, explore?, items: RowItem[]}`. `explore` is a query string for `GET /cinema/titles` that lists the same set ("Explore all"). Keys:

* generated: `new-this-week` (changed in the last 7 days), `recently-added`, `anime-room`, `one-more-episode` (episodic titles by latest episode), `films` ("For the love of cinema"), `animation-banner` (display `banner`, the fanned poster collage), `genre:<name>`;
* curated: `collection:<id>` and the spotlight rows (display `hero`);
* provided (other backends plug these in through `RowProvider`, section 8.6): `continue-watching`, `recently-explored`, `trending`, `because-you-watched`, `friends-watching`, `requests-fulfilled`. A key nobody provides is 404 on `/cinema/rows/{key}` and simply absent from `discover`.

`discover` order: hero, continue-watching, recently-explored, the curated rows for surface `discover`, animation-banner, anime-room, one-more-episode, films, trending, because-you-watched, new-this-week, friends-watching, requests-fulfilled, recently-added. Empty rows are left out. The hero is the published collection with `display=hero` for the surface (staff-curated), falling back to the newest published titles that have a backdrop.

`Collection {id, slug, name, eyebrow?, blurb?, display, surfaces[], position, published, item_count}`.

#### 8.3 Facets

`Facets {genres: [{name, count}], ratings: [{name, count}], languages: [{name, count}], subtitle_languages: [{name, count}], year_min?, year_max?, runtime_max?, total}` over the published titles of `kind` (all kinds when omitted).

#### 8.4 Browse parameters

`GET /cinema/titles`: `kind`, `genres` (csv, every one), `exclude_genres` (csv), `tags` (csv), `year_from`, `year_to`, `rating` (csv of content ratings, any), `runtime_min`, `runtime_max` (minutes), `language` (audio, BCP 47), `subtitle_language`, `age` (`include|hide|only`), `collection` (collection id, "Room selection"), `sort` (`updated` default, `newest`, `title`, `year`, `runtime`, `curated` only with `collection`), `order` (`asc|desc`, default depends on the sort: title, runtime and curated ascending, the others descending), `limit` (1..100, default 24), `cursor` (the previous page's `next_cursor`). Keyset pagination: `next_cursor` is absent on the last page; a cursor made for another sort is 400. `total` is the count for the filters, independent of the cursor.

### 8.5 Staff routes (`/admin/cinema/*`)

Staff session required (401/403). Every successful change is written to the staff access log (`cinema.title.create`, `cinema.title.publish`, `cinema.episode.update`, `cinema.collection.update`, ...). Permissions (doc 07): `PUBLISH_MEDIA` creates, edits and publishes; `DELETE_CONTENT` deletes titles, seasons and episodes; unpublishing takes either; merging takes both; collections take `MANAGE_COLLECTIONS`.

| Route | Body -> result |
|---|---|
| `GET\|POST /admin/cinema/titles` | list (filters `kind`, `published`, `title`, `age`, `sort`, `order`, `limit`, `offset`; drafts included) -> `TitlePage`; `TitleInput` -> 201 `Title` |
| `GET\|PUT\|DELETE /admin/cinema/titles/{id}` | `AdminTitleDetail {title, seasons, episodes, variants, trailers, markers}` (everything, unpublished too); `TitleInput` -> `Title`; 204 |
| `POST /admin/cinema/titles/{id}/publish`, `/unpublish` | `Title` (publish needs something playable, else 409) |
| `POST /admin/cinema/titles/{id}/merge` | `{target_id}` -> target `Title` |
| `POST /admin/cinema/titles/{id}/artwork` | `{slot: poster\|backdrop\|logo\|still\|season-poster, content_type: image/webp\|image/jpeg\|image/png}` -> `ArtworkUpload {key, upload_url, content_type, expires_in_secs}`: `PUT` the image to `upload_url` (same `Content-Type`), then save `key` as `poster_key` / `backdrop_key` / `logo_key` / `still_key` / a season's `poster_key` |
| `PUT /admin/cinema/titles/{id}/asset` | `{asset_id: string\|null}` -> `Title` (links the playable asset; the asset tables belong to the ingest work) |
| `POST /admin/cinema/titles/{id}/seasons` | `SeasonInput` -> 201 `Season` |
| `PUT\|DELETE /admin/cinema/seasons/{id}` | `SeasonInput` -> `Season`; 204 |
| `POST /admin/cinema/seasons/{id}/episodes` | `EpisodeInput` -> 201 `Episode` |
| `PUT\|DELETE /admin/cinema/episodes/{id}` | `EpisodeInput` -> `Episode`; 204 |
| `POST /admin/cinema/episodes/{id}/publish`, `/unpublish` | `Episode` (publish needs an asset, else 409) |
| `PUT /admin/cinema/episodes/{id}/asset` | `{asset_id\|null}` -> `Episode` |
| `POST /admin/cinema/titles/{id}/variants`, `PUT\|DELETE /admin/cinema/variants/{id}` | `VariantInput` -> `Variant` |
| `POST /admin/cinema/titles/{id}/trailers`, `PUT\|DELETE /admin/cinema/trailers/{id}` | `TrailerInput` -> `Trailer` |
| `PUT /admin/cinema/titles/{id}/markers`, `GET\|PUT /admin/cinema/episodes/{id}/markers` | `{markers: [{kind, start_ms, end_ms}]}` replaces the set -> `MarkerList` |
| `GET\|POST /admin/cinema/collections` | all collections, drafts too; `CollectionInput` -> 201 `Collection` |
| `GET\|PUT\|DELETE /admin/cinema/collections/{id}` | `CollectionDetail {collection, items: Title[]}`; `CollectionInput` -> `Collection`; 204 |
| `PUT /admin/cinema/collections/{id}/items` | `{title_ids: string[]}` (ordered, replaces) -> `CollectionDetail` |

`TitleInput` carries the editable fields of `Title` (`kind`, `title`, `original_title`, `alt_titles`, `year`, `runtime_min`, `content_rating`, `synopsis`, `tagline`, `genres`, `tags`, `languages`, `subtitle_languages`, `cast`, `directors`, `writers`, `studios`, `crew`, `age_restricted`, `poster_key`, `backdrop_key`, `logo_key`, `external_ids`); artwork keys must start with `cinema/`. A published title always has something playable: unlinking the last asset, unpublishing or deleting the last published episode (or its season) unpublishes the title. The asset link is deliberately not part of the metadata input, so editing metadata never unlinks a playable asset. `SeasonInput {number, name?, synopsis?, poster_key?}`, `EpisodeInput {number, absolute_number?, title, synopsis, air_date?, runtime_min?, still_key?}`, `VariantInput {label, notes?, runtime_min?, asset_id?, position?}`, `TrailerInput {label, kind, language?, asset_id?}`, `CollectionInput {slug, name, eyebrow?, blurb?, display (row|hero|banner), surfaces[], position, published}`.

### 8.6 Plugging in user-data rows (`RowProvider`)

Rows that need user data are produced by the social work through `sr_cinema::RowProvider` (`key()`, `async row(&RowContext) -> Option<ProvidedRow>`); register with `CinemaService::with_provider`. The provider returns ordered `RowEntry {title_id, episode_id?, progress_percent?, reason?}`; the service loads the titles through the viewer's scope (published, age filter), so a provider cannot leak a draft. Catalog changes that social tables must follow (a merge) reach them through `sr_cinema::CatalogObserver::title_merged`.

## 9. Social and progress API as built (phase 6, crate `sr-cinema-social`)

Ids are decimal strings, times are Unix milliseconds, errors are the usual `{code, message}` body, all routes need a signed-in
user session. The OpenAPI document (`packages/api-client/openapi.json`) is the exact contract; this is the reading guide.
The tables keep plain `title_id` / `episode_id` columns **without foreign keys** to the catalog (the catalog lives in
`sr-cinema`); every request is validated through the `CatalogLookup` port (section 8.9). **A movie is addressed by its title
alone; its `episode_id` is `"0"` everywhere** (progress, ratings, comments, history).

Shared shapes:
- Every schema of this section is `Cinema`-prefixed in the OpenAPI document (`CinemaProgress`, `CinemaReview`, `CinemaTitleRef`, ...) so it cannot clash with the Library's `Progress`, `HistoryPage` and friends.
- `TitleRef` `{id, kind: movie|series|anime, title, year?, age_restricted, poster_key?}` and `EpisodeRef`
  `{id, title_id, season, number, title?, runtime_secs?}` come from the catalog at read time (a title that is unpublished or
  gone is left out of lists).
- `Progress` `{title_id, episode_id, position_secs, duration_secs, percent, completed, updated_at_ms}`.
- Pages: `{items, next_before?}`; pass `next_before` back as `before`; `limit` 1 to 100 (default 24).

### 9.1 Watch progress (CIN-PLY-007)

| Route | Meaning |
|---|---|
| `GET /cinema/titles/{title_id}/progress`, `GET /cinema/titles/{title_id}/episodes/{episode_id}/progress` | The caller's progress (`404` when none). |
| `PUT` the same paths | Heartbeat / save. Body `{position_secs, duration_secs, completed?, playing?, base_updated_at_ms?, force?}`. Answers `{progress, applied, recorded}`. |
| `DELETE` the same paths | Forget this item (history per-item delete). `204`, or `404`. |
| `GET /cinema/titles/{title_id}/watch-state` | `{items: Progress[]}`: every item of a title (episode ticks, "mark season watched" state). |
| `POST /cinema/titles/{title_id}/watched` | Body `{watched, episode_id?, season?}`: mark one episode, a season, or (neither) the whole title watched/unwatched. Answers `{changed}`. |

The player saves every 10 s while playing, on pause and on unload. `completed` is `percent >= 90` unless the body says
otherwise, and it is sticky: a rewatch keeps the item watched until the client sends `completed:false` (or `DELETE`).
`playing:false` (pause, stop) also ends the "watching" presence at once. Writes cost one row; the default API bucket (40 per
10 s per route) is far above the 10 s cadence. Opening an age-restricted title needs age confirmation (`900011`) like the Library.

**Conflicts** work as in the Library: with `base_updated_at_ms` (the `updated_at_ms` last seen, `0` for none) and a newer stored
row, nothing is written and the answer is `{applied:false, progress:<server's>}`; repeat with `force:true` to overwrite. Without a
base the write always applies. **Paused history** (section 8.2): the heartbeat is accepted (`applied:true`) but not stored
(`recorded:false`).

### 9.2 History, continue watching

| Route | Meaning |
|---|---|
| `GET /cinema/me/history` | `{items: [{title, episode?, progress}], next_before}`, newest first, every item (episodes separately). |
| `DELETE /cinema/me/history` | Clear all history (progress included). `204`. |
| `GET /cinema/me/history/settings`, `PUT` | `{paused}`. While paused nothing is recorded and no presence is shown. |
| `GET /cinema/me/continue-watching` | `{items: [{reason, title, episode?, progress?}]}`: per title the latest touched item. `reason: "resume"` is an unfinished item with its `progress`; `"up_next"` is a finished episode whose next episode exists (`episode` is the next one, no `progress`). Finished movies are left out. Capped at `limit` (default 24). |

### 9.3 Gateway: `SR_CINEMA_UPDATE` (cross-device sync)

User-scoped, to the caller's own sessions (doc 05 section 6.3). `d.type`: `progress` (the `Progress`), `progress_cleared`
(`title_id`, `episode_id`), `watched` (`title_id`, `changed`), `history_cleared`, `history_settings` (`paused`),
`list_item_added` / `list_item_removed` (`list_id`, `title_id`), `list_changed` (the list), `list_deleted` (`list_id`),
`rating` (`title_id`, `episode_id`, `score`) / `rating_removed`. Clients treat it as a hint and refetch. Resume position
reaches the other device within the 10 s heartbeat plus delivery.

### 9.4 "Watching" presence (CIN-PLY-009)

User setting **`cinema.share_watching`** in the settings blob: `"off"`, `"friends"` (default) or `"everyone"` (friends and people
who share a server, D-120). Absent or unknown means `friends`. A heartbeat with `playing` true writes a short-lived record
(45 s, refreshed by each heartbeat) and, when the item changed or the record had lapsed, dispatches **`SR_ACTIVITY_UPDATE`**
to the audience: `{type:"watching", user_id, activity:{type:3, name:"SIDE ROOM Cinema", details, state, timestamps:{start},
application_id:null}, title_id?, episode_id?, expires_at_ms}`; `{type:"stopped", user_id}` when it ends. `activity` is
the Discord "Watching" shape so it renders like an activity card. Clients drop the activity at `expires_at_ms` if no refresh
came. **Age-restricted titles** are announced to friends only (never to the shared-server audience) and masked
(`details:"An age-restricted title"`, no ids); `GET /cinema/me/friends-watching` shows the real title only to a
viewer who confirmed 18+. `GET /cinema/me/friends-watching` returns `{items: [{user_id, title, episode?, started_at_ms,
expires_at_ms}]}` for the friends who share. `DELETE /cinema/me/watching` stops the caller's own presence. Merging the activity
into the gateway's `PRESENCE_UPDATE.activities` is a gateway follow-up; the dedicated event carries the same object.

### 9.5 Lists (CIN-MY-002)

`visibility`: `private` (default), `friends` (shared with friends), `public` (everyone signed in). List: `{id, owner_id, name,
description, kind: saved|custom, visibility, item_count, created_at_ms, updated_at_ms}`; item `{title, note, added_at_ms}` in order.

| Route | Meaning |
|---|---|
| `GET /cinema/me/list` | "My list": the saved titles `{items, next_before}`, newest first. |
| `PUT /cinema/me/list/{title_id}`, `DELETE` | Save / unsave. `PUT` is idempotent. |
| `GET /cinema/lists` | The caller's lists (the saved one first), `{lists}`. |
| `POST /cinema/lists` | `{name, description?, visibility?}`. 100 lists per account. |
| `GET /cinema/lists/{id}` | `{list, items, next_before}` if the caller owns it or its visibility allows (else `404`). Items in the owner's order (`before` is an offset cursor). |
| `PATCH /cinema/lists/{id}` | Rename / describe / change visibility. The saved list cannot be renamed. |
| `DELETE /cinema/lists/{id}` | Delete (custom lists only). |
| `PUT /cinema/lists/{id}/items/{title_id}` | Add at the end (body `{note?}`); 1000 items per list. `DELETE` removes. |
| `PUT /cinema/lists/{id}/order` | Body `{title_ids: []}`, the complete new order (must be the same set). |
| `GET /cinema/users/{user_id}/lists` | The lists of someone else that the caller may see. |

The Library has no lists yet (LIB-SOC-006), so there was nothing to generalise; the table shapes are generic enough to share later.

### 9.6 Ratings, reviews, comments (CIN-SOC-001)

Ratings are integers 1 to 10 (no thumbs). `PUT /cinema/titles/{id}/rating` and `PUT .../episodes/{eid}/rating` with `{score}`;
`DELETE` removes. `GET .../ratings` answers `{mine?, global, friends, known}` each `{average?, count}` (ratings are public
independent of history, D-120; `known` is people sharing a server).

Reviews (one per account and title): `GET /cinema/titles/{id}/reviews?sort=helpful|recent`, `PUT /cinema/titles/{id}/review`
(`{body, spoiler?, rating?}`, creates or replaces; `rating` also sets the rating), `DELETE ...`, `PUT/DELETE /cinema/reviews/{id}/helpful`.
Review `{id, title_id, author_id, rating?, body, spoiler, helpful_count, voted, mine, created_at_ms, updated_at_ms}`.

Comments on a title or an episode, threaded one level: `GET/POST /cinema/titles/{id}/comments`,
`GET/POST /cinema/titles/{id}/episodes/{eid}/comments` (`{body, spoiler?, parent_id?}`; a reply's parent must be top level),
`GET /cinema/comments/{id}/replies`, `PATCH/DELETE /cinema/comments/{id}` (author), `PUT/DELETE /cinema/comments/{id}/like`.
Comment `{id, title_id, episode_id, parent_id?, author_id, body, spoiler, like_count, reply_count, liked, mine, created_at_ms,
edited_at_ms?}`. Authors the caller blocked or ignored are left out. Deleting a comment with replies removes the replies.

**Moderation hooks.** `POST /cinema/reviews/{id}/report` and `POST /cinema/comments/{id}/report` (`{reason}`) store a report (once per
reporter and target) and call `ModerationHook::on_report`; `ModerationHook::screen` runs before a review or comment is stored and
may reject it. Staff hiding is `CinemaService::set_hidden`; hidden content is not listed. No staff HTTP surface yet (gap).

### 9.7 Trending and recommendations (CIN-SOC-002)

`GET /cinema/trending?scope=global|friends|known&kind=movie|series|anime&limit=` answers `{scope, items: [{title, score, viewers}]}`.
Score is distinct viewers in the last 7 days plus twice the completions plus high ratings (7+). Scopes count only people whose
`cinema.share_watching` allows it (friends scope: not `off`; known scope: `everyone`); global is an anonymous aggregate of everyone
who is not `off`, and a title needs at least 2 viewers to appear in it. Age-restricted titles appear in the `friends` scope for confirmed
adults only and are never in `known` or `global`.
`GET /cinema/me/recommendations` answers `{rows: [{because: TitleRef, items: [{title, score, reasons}]}]}` ("Because you watched X"):
candidates are the titles other viewers of X also watched (at least 2 of them) plus catalog titles sharing tags with X
(`CatalogLookup::titles_with_tags`), scored together (co-watchers count 1 each up to 10, tag overlap up to 3), minus what the
caller already started. People with `cinema.share_watching = off` are not counted as co-watchers, and age-restricted titles are never recommended.

### 9.8 Gaps

CIN-SOC-003 contribution credits are not built: they need the cinema submissions pipeline (D-074 counts approved movies and episodes).
No staff HTTP surface for moderation or featured lists. No expiry sweeper job for presence rows (expired rows are ignored by every read
and replaced by the next heartbeat; `CinemaService::sweep_presence` is available for a job).

### 9.9 `CatalogLookup` (implemented by `sr-cinema`)

```rust
#[async_trait]
pub trait CatalogLookup: Send + Sync {
    async fn title(&self, id: i64) -> CatalogResult<Option<CatalogTitle>>;        // None: unknown, unpublished or merged
    async fn titles(&self, ids: &[i64]) -> CatalogResult<Vec<CatalogTitle>>;      // missing ids are skipped
    async fn episode(&self, title_id: i64, episode_id: i64) -> CatalogResult<Option<CatalogEpisode>>;
    async fn episodes(&self, title_id: i64, season: Option<i32>) -> CatalogResult<Vec<CatalogEpisode>>; // published, in order
    async fn next_episode(&self, title_id: i64, after_episode_id: i64) -> CatalogResult<Option<CatalogEpisode>>;
    async fn titles_with_tags(&self, tags: &[String], exclude: &[i64], limit: u32) -> CatalogResult<Vec<CatalogTitle>>;
}
```
`CatalogTitle {id, kind, title, year, age_restricted, runtime_secs, poster_key, tags}` (tags include genres, lower-case);
`CatalogEpisode {id, title_id, season, number, title, runtime_secs}`. `sr-cinema-social` ships `MemoryCatalog` for tests.
