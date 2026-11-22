use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::backend::manga_provider::MangaProviders;

/// Selectable option within a declarative provider filter field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterOption {
    pub id: String,
    pub label_es: String,
    pub label_en: String,
}

/// UI control kind corresponding to a filter field in graphical frontends (Mangabrary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterFieldKind {
    Select,
    MultiSelect,
    TriStateTags,
    Text,
    Toggle,
    Sort,
}

/// Declarative filter field metadata rendered dynamically by the GUI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterField {
    pub id: String,
    pub label_es: String,
    pub label_en: String,
    pub kind: FilterFieldKind,
    pub options: Vec<FilterOption>,
    pub default_values: Vec<String>,
}

/// Complete filter schema supported by a specific manga source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFilterSchema {
    pub provider: MangaProviders,
    pub supports_remote_filters: bool,
    pub fields: Vec<FilterField>,
}

/// User-selected filter values submitted from the UI back to `manga-core`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterSelection {
    pub values: BTreeMap<String, Vec<String>>,
    pub excluded: BTreeMap<String, Vec<String>>,
}

/// Trait implemented by providers to describe their GUI-compatible search filters declaratively.
pub trait FilterSchema {
    fn filter_schema(&self) -> ProviderFilterSchema;
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn filter_schema_and_selection_round_trip_json() {
        let schema = ProviderFilterSchema {
            provider: MangaProviders::Mangadex,
            supports_remote_filters: true,
            fields: vec![FilterField {
                id: "content_rating".to_string(),
                label_es: "Clasificación".to_string(),
                label_en: "Content rating".to_string(),
                kind: FilterFieldKind::MultiSelect,
                options: vec![FilterOption {
                    id: "safe".to_string(),
                    label_es: "Seguro".to_string(),
                    label_en: "Safe".to_string(),
                }],
                default_values: vec!["safe".to_string()],
            }],
        };

        let schema_json = serde_json::to_string(&schema).expect("schema should serialize");
        let parsed_schema: ProviderFilterSchema = serde_json::from_str(&schema_json).expect("schema should deserialize");
        assert_eq!(parsed_schema, schema);

        let mut selection = FilterSelection::default();
        selection.values.insert("tags".to_string(), vec!["fantasy".to_string()]);
        selection.excluded.insert("tags".to_string(), vec!["gore".to_string()]);

        let selection_json = serde_json::to_string(&selection).expect("selection should serialize");
        let parsed_selection: FilterSelection = serde_json::from_str(&selection_json).expect("selection should deserialize");
        assert_eq!(parsed_selection, selection);
    }
}
