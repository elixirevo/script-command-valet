use directories::{BaseDirs, ProjectDirs};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub source_home: PathBuf,
    pub source_command_dir: PathBuf,
    pub data_dir: PathBuf,
    pub activations_dir: PathBuf,
    pub history_dir: PathBuf,
    pub state_dir: PathBuf,
    pub current_path: PathBuf,
    pub active_command_dir: PathBuf,
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self, String> {
        let base = BaseDirs::new()
            .ok_or_else(|| "could not determine the user home directory".to_string())?;
        let project = ProjectDirs::from("", "", "scv")
            .ok_or_else(|| "could not determine platform application directories".to_string())?;

        let source_home = env_path("SCV_HOME").unwrap_or_else(|| base.home_dir().join(".scv"));
        let data_dir =
            env_path("SCV_DATA_DIR").unwrap_or_else(|| project.data_local_dir().to_path_buf());
        let config_dir =
            env_path("SCV_CONFIG_DIR").unwrap_or_else(|| project.config_dir().to_path_buf());
        let cache_dir =
            env_path("SCV_CACHE_DIR").unwrap_or_else(|| project.cache_dir().to_path_buf());

        Self::installed(source_home, data_dir, config_dir, cache_dir)
    }

    #[cfg(test)]
    pub fn isolated(root: PathBuf) -> Result<Self, String> {
        Self::installed(
            root.join("home/.scv"),
            root.join("data"),
            root.join("config"),
            root.join("cache"),
        )
    }

    fn installed(
        source_home: PathBuf,
        data_dir: PathBuf,
        config_dir: PathBuf,
        cache_dir: PathBuf,
    ) -> Result<Self, String> {
        ensure_source_isolation(&source_home, [&data_dir, &config_dir, &cache_dir])?;
        let activations_dir = data_dir.join("activations");
        let history_dir = data_dir.join("history");
        let state_dir = data_dir.join("state");
        let current_path = data_dir.join("current");
        let active_command_dir = resolve_active_command_dir(&current_path, &activations_dir)?
            .unwrap_or_else(|| data_dir.join("inactive/commands"));
        Ok(Self {
            source_command_dir: source_home.join("commands"),
            source_home,
            data_dir,
            activations_dir,
            history_dir,
            state_dir,
            current_path,
            active_command_dir,
            config_dir,
            cache_dir,
        })
    }

    pub fn ensure_source_dir(&self) -> Result<(), String> {
        fs::create_dir_all(&self.source_command_dir).map_err(|error| {
            format!(
                "could not create SCV source directory '{}': {error}",
                self.source_command_dir.display()
            )
        })
    }

    pub fn ensure_data_dirs(&self) -> Result<(), String> {
        for directory in [&self.activations_dir, &self.history_dir, &self.state_dir] {
            fs::create_dir_all(directory).map_err(|error| {
                format!(
                    "could not create SCV data directory '{}': {error}",
                    directory.display()
                )
            })?;
        }
        Ok(())
    }

    pub fn ensure_config_dir(&self) -> Result<(), String> {
        fs::create_dir_all(&self.config_dir).map_err(|error| {
            format!(
                "could not create config directory '{}': {error}",
                self.config_dir.display()
            )
        })
    }

    pub fn source_manifest_path(&self) -> PathBuf {
        self.source_home.join("scv.toml")
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn source_package_dir(&self, command: &str) -> PathBuf {
        self.source_command_dir.join(command)
    }

    pub fn source_package_entry_path(&self, command: &str, entry: &str) -> PathBuf {
        self.source_package_dir(command).join(entry)
    }

    pub fn source_package_metadata_path(&self, command: &str) -> PathBuf {
        self.source_package_dir(command).join("metadata.toml")
    }

    pub fn active_package_dir(&self, command: &str) -> PathBuf {
        self.active_command_dir.join(command)
    }

    #[cfg(test)]
    pub fn active_package_entry_path(&self, command: &str, entry: &str) -> PathBuf {
        self.active_package_dir(command).join(entry)
    }

    pub fn apply_to(&self, command: &mut std::process::Command) {
        command
            .env("SCV_HOME", &self.source_home)
            .env("SCV_DATA_DIR", &self.data_dir)
            .env("SCV_CONFIG_DIR", &self.config_dir)
            .env("SCV_CACHE_DIR", &self.cache_dir)
            .env("SCV_ACTIVE_DIR", &self.active_command_dir);
    }
}

fn resolve_active_command_dir(
    current_path: &Path,
    activations_dir: &Path,
) -> Result<Option<PathBuf>, String> {
    let current = match fs::read_to_string(current_path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "could not read current activation '{}': {error}",
                current_path.display()
            ));
        }
    };
    let id = current.trim();
    if !valid_activation_id(id) {
        return Err(format!(
            "current activation contains an invalid identifier: {}",
            current_path.display()
        ));
    }
    let commands = activations_dir.join(id).join("commands");
    let metadata = fs::symlink_metadata(&commands).map_err(|error| {
        format!(
            "current activation '{}' is unavailable at '{}': {error}",
            id,
            commands.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "current activation command directory is unsafe: {}",
            commands.display()
        ));
    }
    Ok(Some(commands))
}

pub fn valid_activation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn ensure_source_isolation<'a>(
    source_home: &Path,
    local_directories: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<(), String> {
    let source = normalized_absolute(source_home)?;
    for directory in local_directories {
        let local = normalized_absolute(directory)?;
        if source.starts_with(&local) || local.starts_with(&source) {
            return Err(format!(
                "SCV source and machine-local directories must not overlap: '{}' and '{}'",
                source_home.display(),
                directory.display()
            ));
        }
    }
    Ok(())
}

fn normalized_absolute(path: &Path) -> Result<PathBuf, String> {
    let input = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|error| format!("could not resolve current directory: {error}"))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in input.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{AppPaths, valid_activation_id};

    fn fixture_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("scv-{label}-{}", crate::storage::unique_nonce()))
    }

    #[test]
    fn isolated_paths_separate_source_and_runtime_data() {
        let root = fixture_root("path-test");
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        assert_eq!(paths.source_command_dir, root.join("home/.scv/commands"));
        assert_eq!(
            paths.active_command_dir,
            root.join("data/inactive/commands")
        );
        assert_ne!(paths.source_command_dir, paths.active_command_dir);
        assert_eq!(paths.history_dir, root.join("data/history"));
    }

    #[test]
    fn resolves_current_activation_without_using_a_symlink() {
        let root = fixture_root("current-test");
        fs::create_dir_all(root.join("data/activations/one/commands"))
            .expect("activation should be created");
        fs::write(root.join("data/current"), "one\n").expect("current should be written");
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        assert_eq!(
            paths.active_command_dir,
            root.join("data/activations/one/commands")
        );
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[test]
    fn validates_activation_identifiers() {
        assert!(valid_activation_id("1234-abcd"));
        assert!(!valid_activation_id("../escape"));
        assert!(!valid_activation_id(""));
    }

    #[test]
    fn rejects_machine_local_data_inside_the_git_source() {
        let root = fixture_root("overlap-test");
        let result = AppPaths::installed(
            root.join(".scv"),
            root.join(".scv/data"),
            root.join("config"),
            root.join("cache"),
        );
        assert!(result.is_err());
    }
}
