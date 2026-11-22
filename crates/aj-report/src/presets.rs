//! Provider presets: well-known OpenAI-compatible services, read from the plain-text
//! `presets/providers.toml` embedded at compile time. Presets only fill the provider form;
//! they are never written to `config.toml`.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const PRESETS_TOML: &str = include_str!("../presets/providers.toml");

/// How a preset is grouped in the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetGroup {
    Vendor,
    Aggregator,
    Local,
}

/// Which site of a vendor that runs separate global and mainland-China services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetRegion {
    Global,
    China,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderPreset {
    /// Suggested provider name.
    pub id: String,
    pub name: String,
    pub group: PresetGroup,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<PresetRegion>,
    pub base_url: String,
    /// `reasoning_effort` hints; `None` means the generic list applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efforts: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetFile {
    preset: Vec<ProviderPreset>,
}

/// All presets, in file order.
pub fn provider_presets() -> &'static [ProviderPreset] {
    static PRESETS: OnceLock<Vec<ProviderPreset>> = OnceLock::new();
    PRESETS.get_or_init(|| {
        // The file is embedded and checked by the tests below, so a parse failure is a build bug.
        toml::from_str::<PresetFile>(PRESETS_TOML)
            .expect("presets/providers.toml is invalid")
            .preset
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::config::validate_provider_name;

    #[test]
    fn presets_parse_and_are_well_formed() {
        let presets = provider_presets();
        assert!(!presets.is_empty());
        let mut ids = HashSet::new();
        let mut urls = HashSet::new();
        for p in presets {
            validate_provider_name(&p.id).unwrap_or_else(|e| panic!("{}: {e}", p.id));
            assert!(ids.insert(p.id.as_str()), "duplicate id {}", p.id);
            assert!(
                urls.insert(p.base_url.as_str()),
                "duplicate url {}",
                p.base_url
            );
            assert!(!p.name.trim().is_empty(), "{}: empty name", p.id);
            assert!(!p.base_url.ends_with('/'), "{}: trailing slash", p.id);
            let local = p.group == PresetGroup::Local;
            let scheme = if local {
                "http://localhost:"
            } else {
                "https://"
            };
            assert!(
                p.base_url.starts_with(scheme),
                "{}: unexpected scheme",
                p.id
            );
            if let Some(efforts) = &p.efforts {
                assert!(!efforts.is_empty(), "{}: empty efforts list", p.id);
            }
        }
    }
}
