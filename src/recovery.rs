use crate::{
    model::Project,
    project_io::{stamp, write_json},
    runtime::fingerprint,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn state_dir() -> PathBuf {
    crate::paths::data_directory("editor")
}
pub fn normalized(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
pub fn recovery_path(path: &Path) -> PathBuf {
    let hash = format!(
        "{:x}",
        Sha256::digest(normalized(path).to_string_lossy().to_lowercase().as_bytes())
    );
    state_dir()
        .join("recovery")
        .join(format!("{}.json", &hash[..24]))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub path: PathBuf,
    pub disk_fingerprint: String,
    pub created: u128,
    pub project: Project,
}
pub fn checkpoint(file: &Path, saved: &Project, project: &Project) -> Result<()> {
    write_json(
        &recovery_path(file),
        &Snapshot {
            version: 1,
            path: normalized(file),
            disk_fingerprint: fingerprint(saved),
            created: stamp(),
            project: project.clone(),
        },
    )
}
pub fn candidate(file: &Path, disk: &Project) -> Option<Snapshot> {
    let data = std::fs::read(recovery_path(file)).ok()?;
    let snapshot: Snapshot = serde_json::from_slice(&data).ok()?;
    if snapshot.version != 1
        || snapshot.path != normalized(file)
        || snapshot.project.id != disk.id
        || snapshot.project == *disk
    {
        return None;
    }
    let modified = std::fs::metadata(file)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    (snapshot.created >= modified).then_some(snapshot)
}
pub fn dismiss(file: &Path, disk: &Project) -> Result<()> {
    checkpoint(file, disk, disk)
}
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Recent {
    pub files: Vec<PathBuf>,
}
pub fn recent() -> Recent {
    std::fs::read(state_dir().join("recent.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn remember(file: &Path) -> Result<()> {
    let mut r = recent();
    let path = normalized(file);
    r.files.retain(|p| p != &path && p.is_file());
    r.files.insert(0, path);
    r.files.truncate(12);
    write_json(&state_dir().join("recent.json"), &r)
}
