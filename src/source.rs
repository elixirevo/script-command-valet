use serde::{Deserialize, Serialize};
use std::fs;

use crate::paths::AppPaths;
use crate::storage;

const SOURCE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceManifest {
    schema_version: u32,
    library: LibraryManifest,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryManifest {
    format: String,
}

pub fn ensure_initialized(paths: &AppPaths) -> Result<(), String> {
    paths.ensure_source_dir()?;
    let manifest_path = paths.source_manifest_path();
    if manifest_path.exists() {
        return validate(paths);
    }
    let manifest = SourceManifest {
        schema_version: SOURCE_SCHEMA_VERSION,
        library: LibraryManifest {
            format: "command-packages".to_string(),
        },
    };
    let contents = toml::to_string_pretty(&manifest)
        .map_err(|error| format!("could not serialize SCV source manifest: {error}"))?;
    storage::write_atomic(&manifest_path, contents.as_bytes())?;
    Ok(())
}

pub fn validate(paths: &AppPaths) -> Result<(), String> {
    validate_home(&paths.source_home)
}

pub fn validate_home(source_home: &std::path::Path) -> Result<(), String> {
    let path = source_home.join("scv.toml");
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        format!(
            "could not inspect SCV source manifest '{}': {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "SCV source manifest must be a regular non-symlink file: {}",
            path.display()
        ));
    }
    let contents = fs::read_to_string(&path).map_err(|error| {
        format!(
            "could not read SCV source manifest '{}': {error}",
            path.display()
        )
    })?;
    let manifest: SourceManifest = toml::from_str(&contents)
        .map_err(|error| format!("invalid SCV source manifest '{}': {error}", path.display()))?;
    if manifest.schema_version != SOURCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported SCV source schema version {} (expected {})",
            manifest.schema_version, SOURCE_SCHEMA_VERSION
        ));
    }
    if manifest.library.format != "command-packages" {
        return Err(format!(
            "unsupported SCV source library format '{}'",
            manifest.library.format
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::paths::AppPaths;

    #[test]
    fn initializes_and_validates_a_source_library() {
        let root = std::env::temp_dir().join(format!(
            "scv-source-test-{}",
            crate::storage::unique_nonce()
        ));
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        super::ensure_initialized(&paths).expect("source should initialize");
        super::validate(&paths).expect("source should validate");
        assert!(paths.source_manifest_path().is_file());
        fs::remove_dir_all(root).expect("fixture should be removed");
    }
}
