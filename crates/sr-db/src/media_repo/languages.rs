//! D-694: a subtitle track that members can pick adds its language to the title's `subtitle_languages`.
//!
//! The list is what the catalog's "subtitles in" filter reads (`cinema_repo.rs`, `= ANY(t.subtitle_languages)`);
//! the search index holds only text fields, so the column is all there is to update. The title is whatever the
//! asset plays for (a title, an episode's show or a cut: the catalog's `asset_id` columns) or was uploaded for
//! (`video_assets.title_id` and `episode_id`). Nothing here ever removes a language.

use sqlx::PgPool;

/// The cap `sr_cinema::rules` puts on the list (40 entries).
const MAX_LANGUAGES: i32 = 40;

/// A track tag as the title editor accepts it (`pt-br` becomes `pt-BR`), or `None` for `und`, an empty tag and
/// a tag the editor would refuse.
pub(crate) fn title_language(tag: &str) -> Option<String> {
    let tag = tag.trim();
    if tag.is_empty() || tag.eq_ignore_ascii_case("und") {
        return None;
    }
    sr_cinema::rules::language(tag).ok()
}

/// Add `tag` to every title linked to `asset_id` that lacks it. Returns the stored form when at least one title
/// changed.
pub(crate) async fn add_title_language(
    pool: &PgPool,
    asset_id: i64,
    tag: &str,
) -> Result<Option<String>, sqlx::Error> {
    let Some(lang) = title_language(tag) else {
        return Ok(None);
    };
    let changed = sqlx::query(
        "WITH linked AS ( \
           SELECT id AS title_id FROM video_titles WHERE asset_id = $1 \
           UNION SELECT title_id FROM video_episodes WHERE asset_id = $1 \
           UNION SELECT title_id FROM video_title_variants WHERE asset_id = $1 \
           UNION SELECT title_id FROM video_assets WHERE id = $1 AND title_id IS NOT NULL \
           UNION SELECT e.title_id FROM video_assets a JOIN video_episodes e ON e.id = a.episode_id WHERE a.id = $1) \
         UPDATE video_titles t SET subtitle_languages = array_append(t.subtitle_languages, $2), updated_at = now() \
         FROM linked WHERE t.id = linked.title_id AND NOT ($2 = ANY(t.subtitle_languages)) \
           AND cardinality(t.subtitle_languages) < $3",
    )
    .bind(asset_id)
    .bind(&lang)
    .bind(MAX_LANGUAGES)
    .execute(pool)
    .await?
    .rows_affected();
    Ok((changed > 0).then_some(lang))
}
