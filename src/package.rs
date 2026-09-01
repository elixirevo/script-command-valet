use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::command::python_runtime;
use crate::metadata::{CommandMetadata, Registry};
use crate::paths::AppPaths;
use crate::storage;

const MAX_FILES: usize = 128;
const MAX_ENTRIES: usize = 256;
const MAX_FILE_SIZE: u64 = 4 * 1024 * 1024;
const MAX_PACKAGE_SIZE: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub struct ValidatedPackage {
    path: PathBuf,
    pub metadata: CommandMetadata,
    pub files: Vec<String>,
    pub checks: Vec<String>,
}

pub fn validate_generated(path: &Path) -> Result<ValidatedPackage, String> {
    validate_package(path, true)
}

pub fn validate_one_shot(path: &Path) -> Result<ValidatedPackage, String> {
    let package = validate_generated(path)?;
    if !package.metadata.arguments.is_empty() || !package.metadata.options.is_empty() {
        return Err("one-shot packages cannot declare arguments or options".to_string());
    }
    if package.metadata.supports_dry_run {
        return Err("one-shot packages cannot declare dry-run support".to_string());
    }
    if package
        .metadata
        .implementation_for(crate::command::current_platform())
        .is_none()
    {
        return Err(format!(
            "one-shot package '{}' does not support the current platform",
            package.metadata.name
        ));
    }
    Ok(package)
}

pub fn digest_validated(package: &ValidatedPackage) -> Result<String, String> {
    let mut hasher = Sha256::new();
    for relative in &package.files {
        let path = package.path.join(relative);
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
    let mut output = String::with_capacity(64);
    for byte in hasher.finalize() {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    Ok(output)
}

pub fn validate_source(path: &Path) -> Result<ValidatedPackage, String> {
    validate_package(path, true)
}

fn validate_package(path: &Path, require_directory_name: bool) -> Result<ValidatedPackage, String> {
    ensure_directory(path, "generated command package")?;
    let package_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "generated package name must be valid UTF-8".to_string())?;
    let metadata_path = path.join("metadata.toml");
    ensure_regular_file(&metadata_path, "package metadata")?;
    let metadata_size = fs::metadata(&metadata_path)
        .map_err(|error| format!("could not inspect metadata size: {error}"))?
        .len();
    if metadata_size > MAX_FILE_SIZE {
        return Err(format!(
            "generated metadata exceeds the {MAX_FILE_SIZE} byte file limit"
        ));
    }
    let contents = fs::read_to_string(&metadata_path).map_err(|error| {
        format!(
            "could not read generated metadata '{}': {error}",
            metadata_path.display()
        )
    })?;
    let metadata = CommandMetadata::from_toml(&contents)
        .map_err(|error| format!("generated metadata: {error}"))?;
    if metadata.builtin {
        return Err("generated command packages cannot declare builtin=true".to_string());
    }
    if metadata.implementations.is_empty() {
        return Err(
            "generated command packages must declare at least one [[implementations]] entry"
                .to_string(),
        );
    }
    metadata
        .validate()
        .map_err(|error| format!("generated metadata: {error}"))?;
    if require_directory_name && package_name != metadata.name {
        return Err(format!(
            "generated package directory '{package_name}' does not match command name '{}'",
            metadata.name
        ));
    }

    let mut files = Vec::new();
    let mut total_size = 0;
    let mut entries = 0;
    collect_files(path, path, 0, &mut entries, &mut files, &mut total_size)?;
    files.sort();
    let mut checks = vec![
        "metadata schema and safety contract".to_string(),
        format!(
            "package layout ({} files, {} bytes)",
            files.len(),
            total_size
        ),
    ];

    let mut checked = BTreeSet::new();
    for implementation in &metadata.implementations {
        let entry = path.join(&implementation.entry);
        ensure_regular_file(&entry, "implementation entry")?;
        let key = (
            implementation.runtime.as_str(),
            implementation.entry.as_str(),
        );
        if checked.insert(key) {
            checks.push(check_syntax(
                &implementation.runtime,
                &implementation.entry,
                &entry,
            )?);
        }
    }

    Ok(ValidatedPackage {
        path: path.to_path_buf(),
        metadata,
        files,
        checks,
    })
}

pub fn install_validated(
    package: &ValidatedPackage,
    paths: &AppPaths,
    registry: &Registry,
    force: bool,
) -> Result<(), String> {
    let existing = registry.get(&package.metadata.name);
    install_from_directory(&package.path, &package.metadata, paths, existing, force)
}

pub fn install_single_entry(
    source: &Path,
    metadata: &CommandMetadata,
    paths: &AppPaths,
    existing: Option<&CommandMetadata>,
    force: bool,
) -> Result<(), String> {
    paths.ensure_source_dir()?;
    validate_install_target(metadata, paths, existing, force)?;
    let staging = create_staging(paths, &metadata.name)?;
    let result = (|| {
        let entry = &metadata.implementations[0].entry;
        let destination = staging.join(entry);
        fs::copy(source, &destination).map_err(|error| {
            format!(
                "could not copy '{}' to '{}': {error}",
                source.display(),
                destination.display()
            )
        })?;
        fs::write(staging.join("metadata.toml"), metadata.to_toml()?)
            .map_err(|error| format!("could not write staged metadata: {error}"))?;
        mark_binary_entries_executable(&staging, metadata)?;
        commit_staging(&staging, metadata, paths, existing, force)
    })();
    if staging.exists() {
        let _ = storage::remove_if_exists(&staging);
    }
    result
}

fn install_from_directory(
    source: &Path,
    metadata: &CommandMetadata,
    paths: &AppPaths,
    existing: Option<&CommandMetadata>,
    force: bool,
) -> Result<(), String> {
    paths.ensure_source_dir()?;
    validate_install_target(metadata, paths, existing, force)?;
    let staging = create_staging(paths, &metadata.name)?;
    let result = (|| {
        copy_directory_contents(source, &staging)?;
        fs::write(staging.join("metadata.toml"), metadata.to_toml()?)
            .map_err(|error| format!("could not write staged metadata: {error}"))?;
        mark_binary_entries_executable(&staging, metadata)?;
        let staged = validate_package(&staging, false)?;
        if staged.metadata.name != metadata.name {
            return Err("staged metadata changed during installation".to_string());
        }
        commit_staging(&staging, metadata, paths, existing, force)
    })();
    if staging.exists() {
        let _ = storage::remove_if_exists(&staging);
    }
    result
}

fn validate_install_target(
    metadata: &CommandMetadata,
    paths: &AppPaths,
    existing: Option<&CommandMetadata>,
    force: bool,
) -> Result<(), String> {
    if existing.is_some_and(|command| command.builtin) {
        return Err(format!(
            "'{}' is a built-in command and cannot be replaced",
            metadata.name
        ));
    }
    if existing.is_some() && !force {
        return Err(format!(
            "command '{}' already exists. Use --force to replace it",
            metadata.name
        ));
    }
    let destination = paths.source_package_dir(&metadata.name);
    if destination.exists() && existing.is_none() {
        return Err(format!(
            "destination '{}' belongs to an unregistered command",
            destination.display()
        ));
    }
    Ok(())
}

fn create_staging(paths: &AppPaths, name: &str) -> Result<PathBuf, String> {
    let nonce = nonce();
    let staging = paths
        .source_command_dir
        .join(format!(".scv-stage-{name}-{}-{nonce}", std::process::id()));
    fs::create_dir(&staging).map_err(|error| {
        format!(
            "could not create staging package '{}': {error}",
            staging.display()
        )
    })?;
    Ok(staging)
}

fn commit_staging(
    staging: &Path,
    metadata: &CommandMetadata,
    paths: &AppPaths,
    existing: Option<&CommandMetadata>,
    force: bool,
) -> Result<(), String> {
    let destination = paths.source_package_dir(&metadata.name);
    let mut backups = Vec::new();
    if force && let Some(existing) = existing {
        let originals = vec![paths.source_package_dir(&existing.name)];
        for (index, original) in originals.into_iter().enumerate() {
            if !original.exists() {
                continue;
            }
            let parent = original
                .parent()
                .ok_or_else(|| format!("'{}' has no parent", original.display()))?;
            let backup = parent.join(format!(".scv-backup-{}-{}-{index}", metadata.name, nonce()));
            fs::rename(&original, &backup).map_err(|error| {
                restore_backups(&mut backups);
                format!("could not prepare '{}': {error}", original.display())
            })?;
            backups.push((original, backup));
        }
    }

    if let Err(error) = fs::rename(staging, &destination) {
        restore_backups(&mut backups);
        return Err(format!(
            "could not install command package '{}': {error}",
            destination.display()
        ));
    }
    for (_, backup) in backups {
        storage::remove_if_exists(&backup)?;
    }
    Ok(())
}

fn restore_backups(backups: &mut Vec<(PathBuf, PathBuf)>) {
    for (original, backup) in backups.drain(..).rev() {
        let _ = fs::rename(backup, original);
    }
}

fn collect_files(
    root: &Path,
    directory: &Path,
    depth: usize,
    entries: &mut usize,
    files: &mut Vec<String>,
    total_size: &mut u64,
) -> Result<(), String> {
    if depth > 16 {
        return Err("generated package exceeds the 16 directory-depth limit".to_string());
    }
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("could not read package '{}': {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("could not read package entry: {error}"))?;
        *entries += 1;
        if *entries > MAX_ENTRIES {
            return Err(format!(
                "generated package exceeds the {MAX_ENTRIES} file/directory entry limit"
            ));
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("could not inspect '{}': {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "generated packages must not contain symbolic links: {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_files(root, &path, depth + 1, entries, files, total_size)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(format!(
                "generated packages may contain only regular files and directories: {}",
                path.display()
            ));
        }
        if metadata.len() > MAX_FILE_SIZE {
            return Err(format!(
                "generated file exceeds the {} byte limit: {}",
                MAX_FILE_SIZE,
                path.display()
            ));
        }
        *total_size += metadata.len();
        if *total_size > MAX_PACKAGE_SIZE {
            return Err(format!(
                "generated package exceeds the {MAX_PACKAGE_SIZE} byte limit"
            ));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| format!("generated file escaped package root: {}", path.display()))?;
        let relative = relative.to_str().ok_or_else(|| {
            format!(
                "generated file path must be valid UTF-8 for cross-platform installation: {}",
                path.display()
            )
        })?;
        files.push(relative.to_string());
        if files.len() > MAX_FILES {
            return Err(format!(
                "generated package exceeds the {MAX_FILES} file limit"
            ));
        }
    }
    Ok(())
}

pub(crate) fn copy_directory_contents(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source)
        .map_err(|error| format!("could not read '{}': {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("could not read package entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)
            .map_err(|error| format!("could not inspect '{}': {error}", source_path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "refusing to copy symlink '{}'",
                source_path.display()
            ));
        }
        if metadata.is_dir() {
            fs::create_dir(&destination_path).map_err(|error| {
                format!("could not create '{}': {error}", destination_path.display())
            })?;
            copy_directory_contents(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "could not copy '{}' to '{}': {error}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        } else {
            return Err(format!(
                "refusing to copy special file '{}'",
                source_path.display()
            ));
        }
    }
    Ok(())
}

fn check_syntax(runtime: &str, entry_name: &str, entry: &Path) -> Result<String, String> {
    let mut command = match runtime {
        "bash" => {
            let mut command = Command::new("bash");
            command.arg("-n").arg(entry);
            command
        }
        "node" => {
            let mut command = Command::new("node");
            command.arg("--check").arg(entry);
            command
        }
        "python" => {
            let mut command = Command::new(python_runtime());
            command
                .arg("-c")
                .arg("import pathlib,sys; p=pathlib.Path(sys.argv[1]); compile(p.read_bytes(), str(p), 'exec')")
                .arg(entry);
            command
        }
        "pwsh" => {
            let mut command = Command::new("pwsh");
            command
                .arg("-NoLogo")
                .arg("-NoProfile")
                .arg("-NonInteractive")
                .arg("-Command")
                .arg("$errors=$null; [System.Management.Automation.Language.Parser]::ParseFile($args[0],[ref]$null,[ref]$errors) | Out-Null; if ($errors.Count) { $errors | ForEach-Object { [Console]::Error.WriteLine($_) }; exit 1 }")
                .arg(entry);
            command
        }
        "binary" => return Ok(format!("binary entry present: {entry_name}")),
        _ => return Err(format!("unsupported runtime '{runtime}'")),
    };
    let output = match command.output() {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(format!(
                "{runtime} syntax skipped (runtime unavailable): {entry_name}"
            ));
        }
        Err(error) => {
            return Err(format!(
                "could not run {runtime} syntax check for '{entry_name}': {error}"
            ));
        }
    };
    if output.status.success() {
        return Ok(format!("{runtime} syntax: {entry_name}"));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim().chars().take(800).collect::<String>();
    Err(format!(
        "{runtime} syntax check failed for '{entry_name}': {detail}"
    ))
}

fn ensure_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("could not inspect {label} '{}': {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("{label} must be a non-symlink directory"));
    }
    Ok(())
}

fn ensure_regular_file(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("could not inspect {label} '{}': {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{label} must be a regular non-symlink file: {}",
            path.display()
        ));
    }
    Ok(())
}

pub(crate) fn mark_binary_entries_executable(
    package: &Path,
    metadata: &CommandMetadata,
) -> Result<(), String> {
    for implementation in &metadata.implementations {
        if implementation.runtime == "binary" {
            make_executable(&package.join(&implementation.entry))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("could not inspect '{}': {error}", path.display()))?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o100);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("could not mark '{}' executable: {error}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub(crate) fn nonce() -> String {
    storage::unique_nonce()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::metadata::Registry;
    use crate::paths::AppPaths;

    use super::{install_validated, validate_generated, validate_one_shot};

    #[test]
    fn validates_a_complete_generated_package() {
        let root = std::env::temp_dir().join(format!(
            "scv-package-test-{}-{}",
            std::process::id(),
            super::nonce()
        ));
        let package = root.join("sample");
        fs::create_dir_all(&package).expect("package should be created");
        fs::write(
            package.join("metadata.toml"),
            r#"
name = "sample"
category = "test"
description = "sample command"
usage = "scv sample"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["reads sample input"]

[[implementations]]
runtime = "python"
platforms = ["linux", "macos", "windows"]
entry = "main.py"
"#,
        )
        .expect("metadata should be written");
        fs::write(package.join("main.py"), "print('ok')\n").expect("entry should be written");
        let validated = validate_generated(&package).expect("package should validate");
        assert_eq!(validated.metadata.name, "sample");
        assert!(validated.files.contains(&"metadata.toml".to_string()));

        let metadata =
            fs::read_to_string(package.join("metadata.toml")).expect("metadata should be readable");
        fs::write(
            package.join("metadata.toml"),
            format!(
                "{metadata}\n[[arguments]]\nname = \"path\"\nrequired = true\ndescription = \"Path to read\"\n"
            ),
        )
        .expect("metadata should be changed");
        let error = validate_one_shot(&package)
            .expect_err("one-shot packages with arguments should be rejected");
        assert!(error.contains("cannot declare arguments or options"));
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[test]
    fn installs_a_validated_package_through_the_registry_contract() {
        let root = std::env::temp_dir().join(format!(
            "scv-package-install-test-{}-{}",
            std::process::id(),
            super::nonce()
        ));
        let source = root.join("source/sample");
        let app_root = root.join("app");
        fs::create_dir_all(&source).expect("source package should be created");
        fs::write(
            source.join("metadata.toml"),
            r#"
name = "sample"
category = "test"
description = "sample command"
usage = "scv sample"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["reads sample input"]

[[implementations]]
runtime = "python"
platforms = ["linux", "macos", "windows"]
entry = "main.py"
"#,
        )
        .expect("metadata should be written");
        fs::write(source.join("main.py"), "print('ok')\n").expect("entry should be written");

        let paths = AppPaths::isolated(app_root).expect("paths should resolve");
        let registry = Registry::load_source(&paths).expect("empty registry should load");
        let validated = validate_generated(&source).expect("package should validate");
        install_validated(&validated, &paths, &registry, false).expect("package should install");
        let installed = Registry::load_source(&paths).expect("installed registry should load");
        assert!(installed.get("sample").is_some());
        assert!(
            paths
                .source_package_entry_path("sample", "main.py")
                .is_file()
        );
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_generated_entries() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "scv-package-symlink-test-{}-{}",
            std::process::id(),
            super::nonce()
        ));
        let package = root.join("sample");
        fs::create_dir_all(&package).expect("package should be created");
        fs::write(
            package.join("metadata.toml"),
            r#"
name = "sample"
category = "test"
description = "sample command"
usage = "scv sample"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["reads sample input"]

[[implementations]]
runtime = "python"
platforms = ["linux", "macos"]
entry = "main.py"
"#,
        )
        .expect("metadata should be written");
        fs::write(root.join("outside.py"), "print('outside')\n")
            .expect("outside file should be written");
        symlink(root.join("outside.py"), package.join("main.py"))
            .expect("symlink should be created");

        let error = validate_generated(&package).expect_err("symlink should be rejected");
        assert!(error.contains("symbolic links") || error.contains("non-symlink"));
        fs::remove_dir_all(root).expect("fixture should be removed");
    }
}
