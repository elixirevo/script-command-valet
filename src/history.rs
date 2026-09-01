use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::i18n::Locale;
use crate::metadata::Risk;
use crate::package::{self, ValidatedPackage};
use crate::paths::{AppPaths, valid_activation_id};
use crate::storage;

const HISTORY_SCHEMA_VERSION: u32 = 1;
const RETAIN_HISTORY: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct HistorySummary {
    pub id: String,
    pub created_unix_ms: u64,
    pub request: String,
    pub locale: Locale,
    pub agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub working_directory: String,
    pub command: String,
    pub description: String,
    pub risk: Risk,
    pub network: bool,
}

#[derive(Debug)]
pub struct StoredHistory {
    pub summary: HistorySummary,
    pub package: ValidatedPackage,
    pub package_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryManifest {
    schema_version: u32,
    id: String,
    created_unix_ms: u64,
    request: String,
    locale: Locale,
    agent: String,
    model: Option<String>,
    working_directory: String,
    command: String,
    description: String,
    risk: Risk,
    network: bool,
    package_digest: String,
}

pub struct StoreRequest<'a> {
    pub request: &'a str,
    pub locale: Locale,
    pub agent: &'a str,
    pub model: Option<&'a str>,
    pub working_directory: &'a Path,
}

pub fn store(
    package: &ValidatedPackage,
    source: &Path,
    request: &StoreRequest<'_>,
    paths: &AppPaths,
) -> Result<StoredHistory, String> {
    ensure_history_dir(paths)?;
    let created_unix_ms = now_unix_ms();
    let source_digest = package::digest_validated(package)?;
    let id = history_id(created_unix_ms, &source_digest, paths);
    let staging = paths
        .history_dir
        .join(format!(".scv-stage-{}", storage::unique_nonce()));
    let staging_package_root = staging.join("package");
    let staging_package = staging_package_root.join(&package.metadata.name);
    fs::create_dir_all(&staging_package).map_err(|error| {
        format!(
            "history: could not create staging directory '{}': {error}",
            staging_package.display()
        )
    })?;

    let result = (|| {
        package::copy_directory_contents(source, &staging_package)?;
        let copied = package::validate_one_shot(&staging_package)
            .map_err(|error| format!("history: copied package failed validation: {error}"))?;
        package::mark_binary_entries_executable(&staging_package, &copied.metadata)?;
        let copied_digest = package::digest_validated(&copied)?;
        if copied_digest != source_digest {
            return Err("history: package changed while being copied".to_string());
        }

        let manifest = HistoryManifest {
            schema_version: HISTORY_SCHEMA_VERSION,
            id: id.clone(),
            created_unix_ms,
            request: request.request.to_string(),
            locale: request.locale,
            agent: request.agent.to_string(),
            model: request.model.map(str::to_string),
            working_directory: request.working_directory.to_string_lossy().into_owned(),
            command: copied.metadata.name.clone(),
            description: copied.metadata.description.clone(),
            risk: copied.metadata.risk,
            network: copied.metadata.network,
            package_digest: copied_digest,
        };
        let contents = toml::to_string_pretty(&manifest)
            .map_err(|error| format!("history: could not serialize manifest: {error}"))?;
        fs::write(staging.join("manifest.toml"), contents).map_err(|error| {
            format!(
                "history: could not write staged manifest '{}': {error}",
                staging.join("manifest.toml").display()
            )
        })?;
        let destination = paths.history_dir.join(&id);
        fs::rename(&staging, &destination).map_err(|error| {
            format!(
                "history: could not commit entry '{}': {error}",
                destination.display()
            )
        })?;
        let stored = match load(&id, paths) {
            Ok(stored) => stored,
            Err(error) => {
                let _ = storage::remove_if_exists(&destination);
                return Err(error);
            }
        };
        if let Err(error) = prune(paths, RETAIN_HISTORY, &id) {
            eprintln!("scv: warning: history cleanup failed: {error}");
        }
        Ok(stored)
    })();

    if staging.exists() {
        let _ = storage::remove_if_exists(&staging);
    }
    result
}

pub fn list(paths: &AppPaths) -> Result<Vec<HistorySummary>, String> {
    if !paths.history_dir.exists() {
        return Ok(Vec::new());
    }
    ensure_safe_directory(&paths.history_dir, "history directory")?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&paths.history_dir).map_err(|error| {
        format!(
            "history: could not read '{}': {error}",
            paths.history_dir.display()
        )
    })? {
        let entry = entry.map_err(|error| format!("history: could not read entry: {error}"))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| "history: entry ID must be valid UTF-8".to_string())?;
        if name.starts_with('.') {
            continue;
        }
        if !valid_activation_id(name) {
            return Err(format!("history: invalid entry ID '{name}'"));
        }
        ensure_safe_directory(&entry.path(), "history entry")?;
        entries.push(read_manifest(&entry.path(), name)?.summary());
    }
    entries.sort_by(|left, right| {
        right
            .created_unix_ms
            .cmp(&left.created_unix_ms)
            .then_with(|| right.id.cmp(&left.id))
    });
    Ok(entries)
}

pub fn load(id: &str, paths: &AppPaths) -> Result<StoredHistory, String> {
    if !valid_activation_id(id) {
        return Err(format!("history: invalid entry ID '{id}'"));
    }
    let directory = paths.history_dir.join(id);
    ensure_safe_directory(&directory, "history entry")?;
    let manifest = read_manifest(&directory, id)?;
    let package_root = directory.join("package");
    ensure_safe_directory(&package_root, "history package directory")?;
    let package_dir = package_root.join(&manifest.command);
    let package = package::validate_one_shot(&package_dir)
        .map_err(|error| format!("history: stored package failed validation: {error}"))?;
    if package.metadata.description != manifest.description
        || package.metadata.risk != manifest.risk
        || package.metadata.network != manifest.network
    {
        return Err(format!(
            "history: entry '{id}' metadata does not match its manifest"
        ));
    }
    let digest = package::digest_validated(&package)?;
    if digest != manifest.package_digest {
        return Err(format!(
            "history: entry '{id}' failed its SHA-256 integrity check"
        ));
    }
    Ok(StoredHistory {
        summary: manifest.summary(),
        package,
        package_dir,
    })
}

impl HistoryManifest {
    fn summary(&self) -> HistorySummary {
        HistorySummary {
            id: self.id.clone(),
            created_unix_ms: self.created_unix_ms,
            request: self.request.clone(),
            locale: self.locale,
            agent: self.agent.clone(),
            model: self.model.clone(),
            working_directory: self.working_directory.clone(),
            command: self.command.clone(),
            description: self.description.clone(),
            risk: self.risk,
            network: self.network,
        }
    }
}

fn read_manifest(directory: &Path, expected_id: &str) -> Result<HistoryManifest, String> {
    let path = directory.join("manifest.toml");
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        format!(
            "history: could not inspect manifest '{}': {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "history: manifest must be a regular non-symlink file: {}",
            path.display()
        ));
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("history: could not read '{}': {error}", path.display()))?;
    let manifest: HistoryManifest = toml::from_str(&contents)
        .map_err(|error| format!("history: invalid manifest '{}': {error}", path.display()))?;
    if manifest.schema_version != HISTORY_SCHEMA_VERSION {
        return Err(format!(
            "history: unsupported schema version {}",
            manifest.schema_version
        ));
    }
    if manifest.id != expected_id {
        return Err("history: entry ID does not match its manifest".to_string());
    }
    if !valid_activation_id(&manifest.id)
        || manifest.request.trim().is_empty()
        || manifest.agent.trim().is_empty()
        || manifest.command.trim().is_empty()
        || manifest.package_digest.len() != 64
        || !manifest
            .package_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "history: entry '{expected_id}' has an invalid manifest"
        ));
    }
    Ok(manifest)
}

fn ensure_history_dir(paths: &AppPaths) -> Result<(), String> {
    fs::create_dir_all(&paths.history_dir).map_err(|error| {
        format!(
            "history: could not create '{}': {error}",
            paths.history_dir.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&paths.history_dir, fs::Permissions::from_mode(0o700)).map_err(
            |error| {
                format!(
                    "history: could not secure '{}': {error}",
                    paths.history_dir.display()
                )
            },
        )?;
    }
    Ok(())
}

fn ensure_safe_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "history: could not inspect {label} '{}': {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "history: {label} must be a non-symlink directory: {}",
            path.display()
        ));
    }
    Ok(())
}

fn history_id(created_unix_ms: u64, digest: &str, paths: &AppPaths) -> String {
    let base = format!("{created_unix_ms}-{}", &digest[..12]);
    if !paths.history_dir.join(&base).exists() {
        return base;
    }
    format!("{base}-{}", storage::unique_nonce())
}

fn prune(paths: &AppPaths, keep: usize, protected_id: &str) -> Result<(), String> {
    let entries = list(paths)?;
    let mut retained = 0;
    for entry in entries {
        if entry.id == protected_id {
            continue;
        }
        if retained < keep.saturating_sub(1) {
            retained += 1;
            continue;
        }
        storage::remove_if_exists(&paths.history_dir.join(entry.id))?;
    }
    Ok(())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::i18n::Locale;
    use crate::package;
    use crate::paths::AppPaths;

    use super::{StoreRequest, list, load, store};

    fn fixture() -> (std::path::PathBuf, AppPaths, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "scv-history-test-{}",
            crate::storage::unique_nonce()
        ));
        let package = root.join("source/instant-task");
        fs::create_dir_all(&package).expect("package should be created");
        fs::write(
            package.join("metadata.toml"),
            r#"name = "instant-task"
category = "one-shot"
description = "Print a result"
usage = "scv instant-task"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["prints a result"]

[[implementations]]
runtime = "binary"
platforms = ["linux", "macos", "windows"]
entry = "task.bin"
"#,
        )
        .expect("metadata should be written");
        fs::write(package.join("task.bin"), b"fixture").expect("entry should be written");
        let paths = AppPaths::isolated(root.join("app")).expect("paths should resolve");
        (root, paths, package)
    }

    #[test]
    fn stores_lists_and_revalidates_a_one_shot_package() {
        let (root, paths, source) = fixture();
        let package = package::validate_one_shot(&source).expect("package should validate");
        let stored = store(
            &package,
            &source,
            &StoreRequest {
                request: "print a result",
                locale: Locale::En,
                agent: "codex",
                model: Some("model"),
                working_directory: &root,
            },
            &paths,
        )
        .expect("history should store");
        let entries = list(&paths).expect("history should list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, stored.summary.id);
        assert_eq!(entries[0].request, "print a result");
        load(&stored.summary.id, &paths).expect("stored package should revalidate");
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[test]
    fn rejects_a_tampered_history_package() {
        let (root, paths, source) = fixture();
        let package = package::validate_one_shot(&source).expect("package should validate");
        let stored = store(
            &package,
            &source,
            &StoreRequest {
                request: "print a result",
                locale: Locale::En,
                agent: "codex",
                model: None,
                working_directory: &root,
            },
            &paths,
        )
        .expect("history should store");
        fs::write(stored.package_dir.join("task.bin"), b"tampered")
            .expect("fixture should be changed");
        let error = load(&stored.summary.id, &paths).expect_err("tampering should fail");
        assert!(error.contains("SHA-256"));
        fs::remove_dir_all(root).expect("fixture should be removed");
    }
}
