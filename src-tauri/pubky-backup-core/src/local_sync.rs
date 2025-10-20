use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const MANIFEST_FILENAME: &str = "local_manifest.json";
const CURSOR_FILENAME: &str = "cursor";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileRecord {
    pub modified: u64,
    pub size: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    pub uploads: Vec<String>,
    pub deletions: Vec<String>,
}

/// Location of the manifest file for a pubky directory.
pub fn manifest_path(pubky_dir: &Path) -> PathBuf {
    pubky_dir.join(MANIFEST_FILENAME)
}

fn should_skip(relative: &str) -> bool {
    relative == CURSOR_FILENAME
        || relative == MANIFEST_FILENAME
        || relative.starts_with(".") && !relative.starts_with(".well-known")
}

/// Scan the pubky directory and collect file metadata.
pub fn scan_local_files(pubky_dir: &Path) -> io::Result<HashMap<String, FileRecord>> {
    let mut map = HashMap::new();

    for entry in WalkDir::new(pubky_dir).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.into_path();
        let relative = match path.strip_prefix(pubky_dir) {
            Ok(rel) => rel,
            Err(_) => continue,
        };

        let relative_str = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        if should_skip(&relative_str) {
            continue;
        }

        let metadata = fs::metadata(&path)?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let modified_secs = modified
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        map.insert(
            relative_str,
            FileRecord {
                modified: modified_secs,
                size: metadata.len(),
            },
        );
    }

    Ok(map)
}

/// Load the persisted manifest from disk.
pub fn load_manifest(manifest_path: &Path) -> io::Result<HashMap<String, FileRecord>> {
    match fs::read(manifest_path) {
        Ok(bytes) => {
            let manifest =
                serde_json::from_slice::<HashMap<String, FileRecord>>(&bytes).unwrap_or_default();
            Ok(manifest)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(e),
    }
}

/// Persist the manifest to disk.
pub fn save_manifest(manifest_path: &Path, state: &HashMap<String, FileRecord>) -> io::Result<()> {
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(state).unwrap_or_default();
    fs::write(manifest_path, data)
}

/// Compare current file state with the manifest and produce a plan of uploads/deletions.
pub fn diff_manifest(
    current: &HashMap<String, FileRecord>,
    previous: &HashMap<String, FileRecord>,
) -> SyncPlan {
    let mut plan = SyncPlan::default();

    for (path, record) in current.iter() {
        match previous.get(path) {
            Some(prev) if prev == record => {}
            _ => plan.uploads.push(path.clone()),
        }
    }

    for path in previous.keys() {
        if !current.contains_key(path) {
            plan.deletions.push(path.clone());
        }
    }

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_file(dir: &Path, rel: &str, contents: &[u8]) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(path).unwrap();
        file.write_all(contents).unwrap();
    }

    #[test]
    fn scan_ignores_internal_files() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "cursor", b"cursor");
        create_file(tmp.path(), MANIFEST_FILENAME, b"{}");
        create_file(tmp.path(), "pub/data.json", b"{}");

        let map = scan_local_files(tmp.path()).unwrap();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("pub/data.json"));
    }

    #[test]
    fn diff_detects_changes_and_deletions() {
        let mut previous = HashMap::new();
        previous.insert(
            "pub/data.json".to_string(),
            FileRecord {
                modified: 1,
                size: 2,
            },
        );

        let mut current = HashMap::new();
        current.insert(
            "pub/data.json".to_string(),
            FileRecord {
                modified: 2,
                size: 3,
            },
        );
        current.insert(
            "pub/new.json".to_string(),
            FileRecord {
                modified: 5,
                size: 10,
            },
        );

        let plan = diff_manifest(&current, &previous);
        assert_eq!(plan.uploads.len(), 2);
        assert!(plan.uploads.contains(&"pub/data.json".to_string()));
        assert!(plan.uploads.contains(&"pub/new.json".to_string()));
        assert_eq!(plan.deletions.len(), 0);

        let empty = HashMap::new();
        let plan_delete = diff_manifest(&empty, &previous);
        assert_eq!(plan_delete.deletions, vec!["pub/data.json".to_string()]);
    }

    #[test]
    fn manifest_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let manifest_path = tmp.path().join(MANIFEST_FILENAME);

        let mut data = HashMap::new();
        data.insert(
            "pub/sample.json".to_string(),
            FileRecord {
                modified: 10,
                size: 42,
            },
        );

        save_manifest(&manifest_path, &data).unwrap();
        let loaded = load_manifest(&manifest_path).unwrap();
        assert_eq!(loaded, data);
    }
}
