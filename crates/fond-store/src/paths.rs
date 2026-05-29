use std::path::PathBuf;

use directories::ProjectDirs;

/// Platform-appropriate directories for fond data and config.
///
/// Falls back to `$HOME/.fond` when `ProjectDirs` cannot determine
/// the standard directories (e.g., in containerised environments).
pub struct FondPaths {
    /// Root data directory (recipes, pantry, etc.)
    pub data_dir: PathBuf,
    /// Config directory
    pub config_dir: PathBuf,
}

impl FondPaths {
    /// Resolve paths using the platform-standard directories.
    ///
    /// Override the data directory with `data_dir_override` (e.g., from `--data-dir`).
    pub fn resolve(data_dir_override: Option<PathBuf>) -> Self {
        if let Some(dir) = data_dir_override {
            return Self {
                config_dir: dir.join("config"),
                data_dir: dir,
            };
        }

        if let Some(proj) = ProjectDirs::from("com", "kafkade", "fond") {
            Self {
                data_dir: proj.data_dir().to_path_buf(),
                config_dir: proj.config_dir().to_path_buf(),
            }
        } else {
            let home = dirs_fallback();
            Self {
                data_dir: home.join("data"),
                config_dir: home.join("config"),
            }
        }
    }

    /// Ensure the data directory and its subdirectories exist.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(self.data_dir.join("recipes"))?;
        std::fs::create_dir_all(&self.config_dir)?;
        Ok(())
    }
}

fn dirs_fallback() -> PathBuf {
    #[allow(deprecated)]
    std::env::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".fond")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_with_override() {
        let tmp = std::env::temp_dir().join("fond-test-paths");
        let paths = FondPaths::resolve(Some(tmp.clone()));
        assert_eq!(paths.data_dir, tmp);
        assert_eq!(paths.config_dir, tmp.join("config"));
    }

    #[test]
    fn resolve_default_returns_valid_paths() {
        let paths = FondPaths::resolve(None);
        // Should not panic and should return non-empty paths
        assert!(!paths.data_dir.as_os_str().is_empty());
        assert!(!paths.config_dir.as_os_str().is_empty());
    }
}
