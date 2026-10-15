//! The `directory.hidden_categories` platform setting (G-29, P10-LBE-3).
//!
//! Operators hide directory categories that do not fit their community: the category
//! disappears from `GET /directory/categories`, from search results, and servers cannot
//! ask to be listed in it. The feature is platform-scope only; the setting is a list of
//! category ids (`CATEGORIES` in `sr-chat`).

use crate::feature::{Feature, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};
use serde_json::Value;

/// Stable feature id.
pub const HIDDEN_CATEGORIES_ID: &str = "directory.hidden_categories";
/// The settings key holding the list.
pub const HIDDEN_CATEGORIES_KEY: &str = "categories";

#[derive(Default)]
pub struct DirectoryHiddenCategoriesFeature;

impl Feature for DirectoryHiddenCategoriesFeature {
    fn id(&self) -> &'static str {
        HIDDEN_CATEGORIES_ID
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Hidden directory categories".into(),
            description: "Categories that are not offered in the server directory.".into(),
            section: "Directory".into(),
            phase: 12,
            default_enabled: true,
        }
    }
    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(vec![
            Field::new(
                HIDDEN_CATEGORIES_KEY,
                "Hidden categories",
                FieldKind::List {
                    item: Box::new(FieldKind::String { max_len: Some(40) }),
                    max_items: Some(64),
                },
            )
            .help("Category ids from the directory category list. Hidden categories are not listed, searched or accepted for new listings.")
            .default_value(Value::Array(Vec::new())),
        ])
    }
    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform]
    }
}

/// The hidden category ids in the effective platform settings.
pub fn hidden_from(values: &serde_json::Map<String, Value>) -> Vec<String> {
    values
        .get(HIDDEN_CATEGORIES_KEY)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn stores_and_reads_the_list() {
        let mut r = FeatureRegistry::new();
        r.register(DirectoryHiddenCategoriesFeature).unwrap();
        let svc = FeatureService::new(Arc::new(r), Arc::new(InMemorySettingsStore::new()), None);
        let none = svc
            .effective_settings(HIDDEN_CATEGORIES_ID, &ScopeRef::Platform)
            .await
            .unwrap();
        assert!(hidden_from(&none).is_empty());
        let v = json!({ "categories": ["gaming", "music"] });
        svc.set_settings(
            HIDDEN_CATEGORIES_ID,
            &ScopeRef::Platform,
            v.as_object().unwrap().clone(),
        )
        .await
        .unwrap();
        let got = svc
            .effective_settings(HIDDEN_CATEGORIES_ID, &ScopeRef::Platform)
            .await
            .unwrap();
        assert_eq!(hidden_from(&got), vec!["gaming", "music"]);
    }

    #[test]
    fn rejects_non_strings() {
        let v = json!({ "categories": [1] });
        assert!(
            DirectoryHiddenCategoriesFeature
                .settings_schema()
                .validate(v.as_object().unwrap())
                .is_err()
        );
    }
}
