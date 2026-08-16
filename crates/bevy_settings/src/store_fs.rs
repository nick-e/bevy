use bevy_log::{debug, error, warn};
use bevy_platform::dirs::preferences_dir;
use bevy_tasks::IoTaskPool;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Persistent storage which uses the local filesystem. Settings will be located in the
/// OS-specific directory for user settings.
pub(crate) struct SettingsStore {
    base_path: Option<PathBuf>,
}

impl SettingsStore {
    /// Construct a new filesystem settings store.
    ///
    /// # Arguments
    /// * `app_name` - The name of the application. See [`crate::SettingsPlugin`] for usage.
    /// * `base_dir` - Directory override. `None` uses the OS preferences directory joined with
    ///   `app_name`; `Some` is used as-is.
    pub(crate) fn new(app_name: &str, base_dir: Option<&Path>) -> Self {
        let base_path = match base_dir {
            Some(dir) => Some(dir.to_path_buf()),
            None => match preferences_dir() {
                Some(prefs) => Some(prefs.join(app_name)),
                None => {
                    warn!("Could not find user configuration directories");
                    None
                }
            },
        };
        if let Some(path) = &base_path {
            debug!("Settings path: {:?}", path);
        }
        Self { base_path }
    }

    /// Save a [`toml::Table`] to disk.
    ///
    /// # Arguments
    /// * `filename` - the name of the file to be saved
    /// * `contents` - the contents of the file
    pub(crate) fn save(&self, filename: &str, contents: toml::Table) {
        if let Some(base_path) = &self.base_path {
            // Recursively create the settings directory if it doesn't exist.
            let mut dir_builder = fs::DirBuilder::new();
            dir_builder.recursive(true);
            if let Err(e) = dir_builder.create(base_path.clone()) {
                warn!("Could not create settings directory: {:?}", e);
                return;
            }

            // Save settings to temp file
            let temp_path = base_path.join(format!("{filename}.toml.new"));
            if let Err(e) = fs::write(&temp_path, contents.to_string()) {
                error!("Error saving settings file: {}", e);
            }

            // Replace old settings file with new one.
            let file_path = base_path.join(format!("{filename}.toml"));
            if let Err(e) = fs::rename(&temp_path, file_path) {
                warn!("Could not save settings file: {:?}", e);
            }
        }
    }

    /// Save the contents of a [`toml::Table`] to disk in another thread.
    ///
    /// # Arguments
    /// * `filename` - the name of the file to be saved
    /// * `contents` - the contents of the file
    pub(crate) fn save_async(&self, filename: &str, contents: toml::Table) {
        if let Some(base_path) = &self.base_path {
            IoTaskPool::get().scope(|scope| {
                scope.spawn(async {
                    // Recursively create the settings directory if it doesn't exist.
                    let mut dir_builder = fs::DirBuilder::new();
                    dir_builder.recursive(true);
                    if let Err(e) = dir_builder.create(base_path.clone()) {
                        warn!("Could not create settings directory: {:?}", e);
                        return;
                    }

                    // Save settings to temp file
                    let temp_path = base_path.join(format!("{filename}.toml.new"));
                    if let Err(e) = fs::write(&temp_path, contents.to_string()) {
                        error!("Error saving settings file: {}", e);
                    }

                    // Replace old settings file with new one.
                    let file_path = base_path.join(format!("{filename}.toml"));
                    if let Err(e) = fs::rename(&temp_path, file_path) {
                        warn!("Could not save settings file: {:?}", e);
                    }
                });
            });
        }
    }

    /// Deserialize a [`toml::Table`] from disk. If the file does not exist, `None` will
    /// be returned.
    ///
    /// # Arguments
    /// * `filename` - The name of the settings file, without the file extension.
    pub(crate) fn load(&self, filename: &str) -> Option<toml::Table> {
        let Some(base_path) = &self.base_path else {
            return None;
        };

        let file_path = base_path.join(format!("{filename}.toml"));
        decode_toml_file(&file_path)
    }
}

/// Load a settings file from disk in TOML format.
pub(crate) fn decode_toml_file(file: &PathBuf) -> Option<toml::Table> {
    if file.exists() && file.is_file() {
        let settings_str = match fs::read_to_string(file) {
            Ok(settings_str) => settings_str,
            Err(e) => {
                error!("Error reading settings file: {}", e);
                return None;
            }
        };

        let table_value = match toml::from_str::<toml::Value>(&settings_str) {
            Ok(table_value) => table_value,
            Err(e) => {
                error!("Error parsing settings file: {}", e);
                return None;
            }
        };

        match table_value {
            toml::Value::Table(table) => Some(table),
            _ => {
                error!("Settings file must be a table");
                None
            }
        }
    } else {
        // Settings file does not exist yet.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_dir_override_is_honored() {
        let tmp =
            std::env::temp_dir().join(format!("bevy-settings-basedir-{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();

        let store = SettingsStore::new("com.example.app", Some(tmp.as_path()));
        let mut table = toml::Table::new();
        table.insert("k".to_string(), toml::Value::Integer(7));
        store.save("settings", table);

        assert!(
            tmp.join("settings.toml").exists(),
            "settings.toml should be written directly under the override dir"
        );
        assert!(
            !tmp.join("com.example.app").exists(),
            "the override dir is used as-is, with no app-name subdir"
        );

        let loaded = store.load("settings").expect("file should reload");
        assert_eq!(loaded.get("k").and_then(toml::Value::as_integer), Some(7));

        fs::remove_dir_all(&tmp).ok();
    }
}
