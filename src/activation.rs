use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::command::current_platform;
use crate::metadata::Registry;
use crate::package;
use crate::paths::{AppPaths, valid_activation_id};
use crate::source;
use crate::storage;

const ACTIVATION_SCHEMA_VERSION: u32 = 1;
const RETAIN_ACTIVATIONS: usize = 10;

#[derive(Debug, Clone, Serialize)]
pub struct ApplyResult {
    pub activation: String,
    pub previous: Option<String>,
    pub package_count: usize,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryState {
    pub package_count: usize,
    pub digest: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivationManifest {
    schema_version: u32,
    id: String,
    created_unix_ms: u64,
    scv_version: String,
    platform: String,
    source_revision: Option<String>,
    library_digest: String,
    packages: Vec<PackageManifest>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageManifest {
    name: String,
    digest: String,
    risk: String,
}

struct ValidatedLibrary {
    digest: String,
    packages: Vec<PackageManifest>,
    package_paths: Vec<PathBuf>,
}

pub fn apply(paths: &AppPaths) -> Result<ApplyResult, String> {
    source::ensure_initialized(paths)?;
    let validated = validate_library(&paths.source_command_dir)?;
    let previous = current(paths)?;

    paths.ensure_data_dirs()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let nonce = storage::unique_nonce();
    let id = format!("{nonce}-{}", &validated.digest[..12]);
    let staging = paths.activations_dir.join(format!(".scv-stage-{nonce}"));
    let staging_commands = staging.join("commands");
    fs::create_dir_all(&staging_commands).map_err(|error| {
        format!(
            "could not create activation staging directory '{}': {error}",
            staging_commands.display()
        )
    })?;

    let result = (|| {
        for package_path in &validated.package_paths {
            let name = package_path
                .file_name()
                .ok_or_else(|| "source package has no directory name".to_string())?;
            let destination = staging_commands.join(name);
            fs::create_dir(&destination).map_err(|error| {
                format!(
                    "could not create staged command package '{}': {error}",
                    destination.display()
                )
            })?;
            package::copy_directory_contents(package_path, &destination)?;
            let copied = package::validate_source(&destination)?;
            package::mark_binary_entries_executable(&destination, &copied.metadata)?;
        }

        let copied = validate_library(&staging_commands)?;
        if copied.digest != validated.digest {
            return Err("activation contents changed while copying source packages".to_string());
        }

        let manifest = ActivationManifest {
            schema_version: ACTIVATION_SCHEMA_VERSION,
            id: id.clone(),
            created_unix_ms: now.as_millis().min(u128::from(u64::MAX)) as u64,
            scv_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: current_platform().to_string(),
            source_revision: source_revision(&paths.source_home),
            library_digest: validated.digest.clone(),
            packages: validated.packages,
        };
        let manifest_toml = toml::to_string_pretty(&manifest)
            .map_err(|error| format!("could not serialize activation manifest: {error}"))?;
        fs::write(staging.join("manifest.toml"), manifest_toml).map_err(|error| {
            format!(
                "could not write staged activation manifest '{}': {error}",
                staging.join("manifest.toml").display()
            )
        })?;

        let destination = paths.activations_dir.join(&id);
        fs::rename(&staging, &destination).map_err(|error| {
            format!(
                "could not commit activation '{}': {error}",
                destination.display()
            )
        })?;
        storage::write_atomic(&paths.current_path, format!("{id}\n").as_bytes())?;
        if let Err(error) = prune(paths, RETAIN_ACTIVATIONS) {
            eprintln!("scv: warning: activation cleanup failed: {error}");
        }
        Ok(ApplyResult {
            activation: id,
            previous,
            package_count: copied.packages.len(),
            digest: copied.digest,
        })
    })();

    if staging.exists() {
        let _ = storage::remove_if_exists(&staging);
    }
    result
}

pub fn rollback(paths: &AppPaths, requested: Option<&str>) -> Result<ApplyResult, String> {
    let result = preview_rollback(paths, requested)?;
    storage::write_atomic(
        &paths.current_path,
        format!("{}\n", result.activation).as_bytes(),
    )?;
    Ok(result)
}

pub fn preview_rollback(paths: &AppPaths, requested: Option<&str>) -> Result<ApplyResult, String> {
    paths.ensure_data_dirs()?;
    let previous = current(paths)?;
    let id = match requested {
        Some(id) => {
            if !valid_activation_id(id) {
                return Err(format!("invalid activation identifier '{id}'"));
            }
            id.to_string()
        }
        None => previous_activation(paths, previous.as_deref())?
            .ok_or_else(|| "no previous activation is available".to_string())?,
    };
    if previous.as_deref() == Some(id.as_str()) {
        return Err(format!("activation '{id}' is already current"));
    }
    let commands = paths.activations_dir.join(&id).join("commands");
    let commands_metadata = fs::symlink_metadata(&commands).map_err(|error| {
        format!(
            "activation '{id}' command directory is unavailable '{}': {error}",
            commands.display()
        )
    })?;
    if commands_metadata.file_type().is_symlink() || !commands_metadata.is_dir() {
        return Err(format!(
            "activation '{id}' command directory is unsafe: {}",
            commands.display()
        ));
    }
    let validated = validate_library(&commands)
        .map_err(|error| format!("activation '{id}' is invalid: {error}"))?;
    read_manifest(
        &paths.activations_dir.join(&id),
        Some(&id),
        Some(&validated.digest),
    )?;
    Ok(ApplyResult {
        activation: id,
        previous,
        package_count: validated.packages.len(),
        digest: validated.digest,
    })
}

pub fn verify_current_package(paths: &AppPaths, name: &str) -> Result<(), String> {
    let id = current(paths)?.ok_or_else(|| "no command activation is current".to_string())?;
    let activation_dir = paths.activations_dir.join(&id);
    let manifest = read_manifest(&activation_dir, Some(&id), None)?;
    let expected = manifest
        .packages
        .iter()
        .find(|package| package.name == name)
        .ok_or_else(|| format!("command '{name}' is not declared by activation '{id}'"))?;
    let package_path = paths.active_package_dir(name);
    let validated = package::validate_source(&package_path)
        .map_err(|error| format!("active command '{name}' failed validation: {error}"))?;
    let actual = digest_package(&package_path, &validated.files)?;
    if actual != expected.digest {
        return Err(format!(
            "active command '{name}' does not match activation '{id}'; run 'scv apply' or 'scv rollback'"
        ));
    }
    Ok(())
}

pub fn inspect_library(command_dir: &Path) -> Result<LibraryState, String> {
    let validated = validate_library(command_dir)?;
    Ok(LibraryState {
        package_count: validated.packages.len(),
        digest: validated.digest,
    })
}

pub fn current(paths: &AppPaths) -> Result<Option<String>, String> {
    match fs::read_to_string(&paths.current_path) {
        Ok(contents) => {
            let id = contents.trim();
            if !valid_activation_id(id) {
                return Err("current activation identifier is invalid".to_string());
            }
            Ok(Some(id.to_string()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "could not read current activation '{}': {error}",
            paths.current_path.display()
        )),
    }
}

pub fn list(paths: &AppPaths) -> Result<Vec<String>, String> {
    if !paths.activations_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for entry in fs::read_dir(&paths.activations_dir).map_err(|error| {
        format!(
            "could not read activations '{}': {error}",
            paths.activations_dir.display()
        )
    })? {
        let entry = entry.map_err(|error| format!("could not read activation: {error}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if valid_activation_id(&name) && entry.path().is_dir() {
            ids.push(name.into_owned());
        }
    }
    ids.sort();
    Ok(ids)
}

fn previous_activation(
    paths: &AppPaths,
    current_id: Option<&str>,
) -> Result<Option<String>, String> {
    let mut ids = list(paths)?;
    ids.retain(|id| Some(id.as_str()) != current_id);
    Ok(ids.pop())
}

fn prune(paths: &AppPaths, keep: usize) -> Result<(), String> {
    let current = current(paths)?;
    let ids = list(paths)?;
    let remove_count = ids.len().saturating_sub(keep);
    let removable = ids
        .into_iter()
        .filter(|id| current.as_deref() != Some(id.as_str()))
        .take(remove_count)
        .collect::<Vec<_>>();
    for id in removable {
        storage::remove_if_exists(&paths.activations_dir.join(id))?;
    }
    Ok(())
}

fn validate_library(command_dir: &Path) -> Result<ValidatedLibrary, String> {
    if !command_dir.exists() {
        return Ok(ValidatedLibrary {
            digest: hex_digest(Sha256::digest([])),
            packages: Vec::new(),
            package_paths: Vec::new(),
        });
    }
    let root_metadata = fs::symlink_metadata(command_dir).map_err(|error| {
        format!(
            "could not inspect command library '{}': {error}",
            command_dir.display()
        )
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(format!(
            "command library must be a regular non-symlink directory: {}",
            command_dir.display()
        ));
    }

    Registry::load_from_command_dir(command_dir)?;
    let mut package_paths = Vec::new();
    for entry in fs::read_dir(command_dir)
        .map_err(|error| format!("could not read command library: {error}"))?
    {
        let entry = entry.map_err(|error| format!("could not read command package: {error}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not inspect command package '{name}': {error}"))?;
        if file_type.is_symlink() || !file_type.is_dir() {
            return Err(format!(
                "command library may contain only package directories: {}",
                entry.path().display()
            ));
        }
        package_paths.push(entry.path());
    }
    package_paths.sort();

    let mut library_hasher = Sha256::new();
    let mut packages = Vec::new();
    for package_path in &package_paths {
        let validated = package::validate_source(package_path)?;
        let package_digest = digest_package(package_path, &validated.files)?;
        library_hasher.update(validated.metadata.name.as_bytes());
        library_hasher.update([0]);
        library_hasher.update(package_digest.as_bytes());
        library_hasher.update([0]);
        packages.push(PackageManifest {
            name: validated.metadata.name,
            digest: package_digest,
            risk: validated.metadata.risk.as_str().to_string(),
        });
    }

    Ok(ValidatedLibrary {
        digest: hex_digest(library_hasher.finalize()),
        packages,
        package_paths,
    })
}

fn digest_package(root: &Path, files: &[String]) -> Result<String, String> {
    let mut hasher = Sha256::new();
    for relative in files {
        let path = root.join(relative);
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        let mut file = fs::File::open(&path)
            .map_err(|error| format!("could not hash '{}': {error}", path.display()))?;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| format!("could not hash '{}': {error}", path.display()))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        hasher.update([0]);
    }
    Ok(hex_digest(hasher.finalize()))
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let mut output = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn read_manifest(
    directory: &Path,
    expected_id: Option<&str>,
    expected_digest: Option<&str>,
) -> Result<ActivationManifest, String> {
    let path = directory.join("manifest.toml");
    let contents = fs::read_to_string(&path).map_err(|error| {
        format!(
            "could not read activation manifest '{}': {error}",
            path.display()
        )
    })?;
    let manifest: ActivationManifest = toml::from_str(&contents)
        .map_err(|error| format!("invalid activation manifest '{}': {error}", path.display()))?;
    if manifest.schema_version != ACTIVATION_SCHEMA_VERSION {
        return Err(format!(
            "unsupported activation schema version {}",
            manifest.schema_version
        ));
    }
    if expected_id.is_some_and(|id| id != manifest.id) {
        return Err("activation identifier does not match its manifest".to_string());
    }
    if expected_digest.is_some_and(|digest| digest != manifest.library_digest) {
        return Err("activation digest does not match its manifest".to_string());
    }
    Ok(manifest)
}

fn source_revision(source_home: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(source_home)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::paths::AppPaths;

    fn fixture_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "scv-activation-test-{}",
            crate::storage::unique_nonce()
        ))
    }

    fn write_package(paths: &AppPaths, name: &str, body: &str) {
        let package = paths.source_package_dir(name);
        fs::create_dir_all(&package).expect("package should be created");
        fs::write(
            package.join("metadata.toml"),
            format!(
                r#"name = "{name}"
category = "test"
description = "test command"
usage = "scv {name}"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["reads test input"]

[[implementations]]
runtime = "python"
platforms = ["windows", "linux", "macos"]
entry = "main.py"
"#
            ),
        )
        .expect("metadata should be written");
        fs::write(package.join("main.py"), body).expect("entry should be written");
    }

    #[test]
    fn applies_source_to_an_immutable_activation_and_rolls_back() {
        let root = fixture_root();
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        crate::source::ensure_initialized(&paths).expect("source should initialize");
        write_package(&paths, "sample", "print('one')\n");
        let first = super::apply(&paths).expect("first activation should apply");
        assert!(
            paths
                .activations_dir
                .join(&first.activation)
                .join("commands/sample/main.py")
                .is_file()
        );

        fs::write(
            paths.source_package_dir("sample").join("main.py"),
            "print('two')\n",
        )
        .expect("source should change");
        let before_apply = AppPaths::isolated(root.clone()).expect("paths should refresh");
        assert_eq!(
            fs::read_to_string(before_apply.active_package_entry_path("sample", "main.py"))
                .expect("active entry should remain readable"),
            "print('one')\n",
            "manual source edits must not change the active command"
        );
        let second = super::apply(&paths).expect("second activation should apply");
        assert_ne!(first.digest, second.digest);
        let rolled_back = super::rollback(&paths, None).expect("rollback should succeed");
        assert_eq!(rolled_back.activation, first.activation);
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[test]
    fn invalid_source_does_not_replace_current_activation() {
        let root = fixture_root();
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        crate::source::ensure_initialized(&paths).expect("source should initialize");
        write_package(&paths, "sample", "print('ok')\n");
        let first = super::apply(&paths).expect("activation should apply");
        fs::write(
            paths.source_package_dir("sample").join("metadata.toml"),
            "not = [valid",
        )
        .expect("metadata should be corrupted");
        assert!(super::apply(&paths).is_err());
        assert_eq!(
            super::current(&paths).expect("current should load"),
            Some(first.activation)
        );
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[test]
    fn refuses_to_execute_a_tampered_activation_package() {
        let root = fixture_root();
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        crate::source::ensure_initialized(&paths).expect("source should initialize");
        write_package(&paths, "sample", "print('trusted')\n");
        super::apply(&paths).expect("activation should apply");
        let active = AppPaths::isolated(root.clone()).expect("active paths should resolve");
        super::verify_current_package(&active, "sample").expect("untouched package should verify");
        fs::write(
            active.active_package_entry_path("sample", "main.py"),
            "print('tampered')\n",
        )
        .expect("activation should be tampered for the test");
        assert!(super::verify_current_package(&active, "sample").is_err());
        fs::remove_dir_all(root).expect("fixture should be removed");
    }
}
