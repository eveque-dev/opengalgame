use std::path::{Path, PathBuf};

fn compatible_directory(root: &Path, relative: &Path) -> PathBuf {
    let current = root.join("opengalgame").join(relative);
    // Read existing installations in place; never move or delete their saves.
    let legacy = root.join("LumenVN").join(relative);
    if !current.exists() && legacy.is_dir() {
        legacy
    } else {
        current
    }
}

pub fn data_directory(relative: impl AsRef<Path>) -> PathBuf {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    compatible_directory(&root, relative.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branding_preserves_legacy_data_and_prefers_existing_new_directory() {
        let root = std::env::temp_dir().join(format!(
            "opengalgame-paths-{}-{}",
            std::process::id(),
            crate::project_io::stamp()
        ));
        let relative = Path::new("saves/project");
        let current = root.join("opengalgame").join(relative);
        let legacy = root.join("LumenVN").join(relative);
        assert_eq!(compatible_directory(&root, relative), current);
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("slot-1.json"), b"existing save").unwrap();
        assert_eq!(compatible_directory(&root, relative), legacy);
        std::fs::create_dir_all(&current).unwrap();
        assert_eq!(compatible_directory(&root, relative), current);
        assert_eq!(
            std::fs::read(legacy.join("slot-1.json")).unwrap(),
            b"existing save"
        );
    }
}
