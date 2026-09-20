use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Configured skill registry URLs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryConfig {
    /// The registry URLs, sorted and trailing-slash-trimmed.
    pub registries: Vec<String>,
}

/// Persistent store for the list of configured skill registries.
pub struct RegistryStore {
    path: PathBuf,
}

impl RegistryStore {
    /// Creates a store backed by `root/registries.json`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            path: root.into().join("registries.json"),
        }
    }

    /// Loads the registry configuration, defaulting to an empty list.
    pub fn load(&self) -> Result<RegistryConfig> {
        if !self.path.exists() {
            return Ok(RegistryConfig::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    /// Adds a registry URL, returning whether it was newly added.
    pub fn add(&self, url: &str) -> Result<bool> {
        // Trim before the duplicate check so `https://host/` and `https://host`
        // are recognized as the same registry instead of being stored twice.
        let url = url.trim_end_matches('/');
        let mut config = self.load()?;
        if config.registries.iter().any(|u| u == url) {
            return Ok(false);
        }
        config.registries.push(url.to_owned());
        config.registries.sort();
        self.save(&config)?;
        Ok(true)
    }

    /// Removes a registry URL, returning whether it was present.
    pub fn remove(&self, url: &str) -> Result<bool> {
        let mut config = self.load()?;
        let old = config.registries.len();
        config.registries.retain(|u| u != url);
        if old == config.registries.len() {
            return Ok(false);
        }
        self.save(&config)?;
        Ok(true)
    }

    /// Persists the registry configuration.
    pub fn save(&self, config: &RegistryConfig) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(config)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_defaults_to_empty_when_missing() {
        let temp = tempfile::tempdir().unwrap();
        let store = RegistryStore::new(temp.path());
        assert!(store.load().unwrap().registries.is_empty());
    }

    #[test]
    fn add_is_idempotent_and_sorts() {
        let temp = tempfile::tempdir().unwrap();
        let store = RegistryStore::new(temp.path());
        assert!(store.add("https://b.example.com").unwrap());
        assert!(store.add("https://a.example.com").unwrap());
        assert!(!store.add("https://a.example.com").unwrap()); // duplicate

        let config = store.load().unwrap();
        assert_eq!(
            config.registries,
            vec!["https://a.example.com", "https://b.example.com"]
        );
    }

    #[test]
    fn add_trims_trailing_slash_before_deduplicating() {
        let temp = tempfile::tempdir().unwrap();
        let store = RegistryStore::new(temp.path());
        assert!(store.add("https://a.example.com").unwrap());
        // the same URL with a trailing slash is the same registry
        assert!(!store.add("https://a.example.com/").unwrap());
        let config = store.load().unwrap();
        assert_eq!(config.registries, vec!["https://a.example.com"]);
    }

    #[test]
    fn remove_reports_existence_and_persists() {
        let temp = tempfile::tempdir().unwrap();
        let store = RegistryStore::new(temp.path());
        assert!(!store.remove("https://gone.example.com").unwrap());
        store.add("https://gone.example.com").unwrap();
        assert!(store.remove("https://gone.example.com").unwrap());
        assert!(store.load().unwrap().registries.is_empty());
    }

    #[test]
    fn remove_does_not_match_a_trailing_slash_entry() {
        let temp = tempfile::tempdir().unwrap();
        let store = RegistryStore::new(temp.path());
        store.add("https://a.example.com").unwrap();
        // removal is exact-match: the slashed variant does not remove it
        assert!(!store.remove("https://a.example.com/").unwrap());
        assert_eq!(store.load().unwrap().registries.len(), 1);
    }

    #[test]
    fn save_load_round_trips_and_creates_parents() {
        let temp = tempfile::tempdir().unwrap();
        let store = RegistryStore::new(temp.path().join("nested/root"));
        store
            .save(&RegistryConfig {
                registries: vec!["https://one.example.com".into()],
            })
            .unwrap();
        assert!(temp.path().join("nested/root/registries.json").is_file());
        let config = store.load().unwrap();
        assert_eq!(config.registries, vec!["https://one.example.com"]);
    }
}
