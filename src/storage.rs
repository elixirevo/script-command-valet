use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn unique_nonce() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{timestamp}-{}-{counter}", std::process::id())
}

pub fn remove_if_exists(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!("could not inspect '{}': {error}", path.display()));
        }
    };

    let result = if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    result.map_err(|error| format!("could not remove '{}': {error}", path.display()))
}

pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("'{}' has no parent directory", path.display()))?;
    let nonce = unique_nonce();
    let temporary = parent.join(format!(".scv-write-{nonce}"));
    fs::write(&temporary, contents)
        .map_err(|error| format!("could not write '{}': {error}", temporary.display()))?;

    let backup = parent.join(format!(".scv-backup-{nonce}"));
    let had_existing = path.exists();
    if had_existing {
        fs::rename(path, &backup).map_err(|error| {
            let _ = remove_if_exists(&temporary);
            format!("could not prepare '{}': {error}", path.display())
        })?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if had_existing {
            let _ = fs::rename(&backup, path);
        }
        let _ = remove_if_exists(&temporary);
        return Err(format!("could not replace '{}': {error}", path.display()));
    }
    if had_existing {
        remove_if_exists(&backup)?;
    }
    Ok(())
}
