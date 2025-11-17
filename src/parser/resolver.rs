use std::path::{Path, PathBuf};

pub struct ImportResolver {
    additional_dirs: Vec<PathBuf>,
}

impl ImportResolver {
    pub fn new(additional_dirs: Vec<PathBuf>) -> Self {
        Self { additional_dirs }
    }

    /// Resolves an import path to an absolute file path
    pub fn resolve_import(&self, current_file: &Path, import_path: &str) -> Option<PathBuf> {
        tracing::debug!("ImportResolver: resolving '{}' from file: {}", import_path, current_file.display());
        tracing::debug!("Additional directories: {:?}", self.additional_dirs);

        // First try additional directories (if any were configured) - highest priority
        for (i, dir) in self.additional_dirs.iter().enumerate() {
            let resolved = dir.join(import_path);
            tracing::debug!("Trying additional dir[{}] {}: {}", i, dir.display(), resolved.display());
            if resolved.exists() {
                tracing::debug!("Found in additional directory: {}", resolved.display());
                return Some(resolved);
            }
        }

        // Then try relative to current file's directory
        if let Some(parent) = current_file.parent() {
            let resolved = parent.join(import_path);
            tracing::debug!("Trying relative to parent: {}", resolved.display());
            if resolved.exists() {
                tracing::debug!("Found at relative path: {}", resolved.display());
                return Some(resolved);
            }
        }

        // Finally try walking up the directory tree from current file
        // This handles cases where imports are relative to a project root
        if let Some(mut current) = current_file.parent() {
            // Start from the parent directory and walk up
            let mut level = 0;
            loop {
                let resolved = current.join(import_path);
                tracing::debug!("Trying from level {}: {}", level, resolved.display());
                if resolved.exists() {
                    tracing::debug!("Found at level {}: {}", level, resolved.display());
                    return Some(resolved);
                }

                // Move up one directory
                match current.parent() {
                    Some(parent) => {
                        // Avoid infinite loops on root directory
                        if parent == current {
                            tracing::debug!("Reached root directory, stopping upward search");
                            break;
                        }
                        current = parent;
                        level += 1;
                    }
                    None => {
                        tracing::debug!("No more parent directories");
                        break;
                    }
                }
            }
        }

        tracing::debug!("Failed to resolve import: {}", import_path);
        None
    }

    pub fn add_directory(&mut self, dir: PathBuf) {
        if !self.additional_dirs.contains(&dir) {
            self.additional_dirs.push(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_relative_import() {
        let dir = tempdir().unwrap();
        let proto_file = dir.path().join("test.proto");
        let import_file = dir.path().join("imported.proto");

        fs::write(&proto_file, "").unwrap();
        fs::write(&import_file, "").unwrap();

        let resolver = ImportResolver::new(vec![]);
        let resolved = resolver.resolve_import(&proto_file, "imported.proto");

        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap(), import_file);
    }

    #[test]
    fn test_resolve_with_additional_dirs() {
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();

        let proto_file = dir1.path().join("test.proto");
        let import_file = dir2.path().join("imported.proto");

        fs::write(&proto_file, "").unwrap();
        fs::write(&import_file, "").unwrap();

        let resolver = ImportResolver::new(vec![dir2.path().to_path_buf()]);
        let resolved = resolver.resolve_import(&proto_file, "imported.proto");

        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap(), import_file);
    }

    #[test]
    fn test_resolve_upward_search() {
        let base_dir = tempdir().unwrap();

        // Create directory structure: base_dir/project/subdir/
        let project_dir = base_dir.path().join("project");
        let subdir = project_dir.join("subdir");
        fs::create_dir_all(&subdir).unwrap();

        // Create proto file in subdir
        let proto_file = subdir.join("test.proto");
        fs::write(&proto_file, "").unwrap();

        // Create import file in project directory
        let import_file = project_dir.join("imported.proto");
        fs::write(&import_file, "").unwrap();

        let resolver = ImportResolver::new(vec![]);
        let resolved = resolver.resolve_import(&proto_file, "imported.proto");

        // Should find the file by walking up the directory tree
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap(), import_file);
    }

    #[test]
    fn test_resolve_upward_search_multiple_levels() {
        let base_dir = tempdir().unwrap();

        // Create deep directory structure: base_dir/a/b/c/d/
        let deep_dir = base_dir.path().join("a/b/c/d");
        fs::create_dir_all(&deep_dir).unwrap();

        // Create proto file in deepest directory
        let proto_file = deep_dir.join("test.proto");
        fs::write(&proto_file, "").unwrap();

        // Create import file at the root
        let import_file = base_dir.path().join("imported.proto");
        fs::write(&import_file, "").unwrap();

        let resolver = ImportResolver::new(vec![]);
        let resolved = resolver.resolve_import(&proto_file, "imported.proto");

        // Should find the file by walking up multiple levels
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap(), import_file);
    }

    #[test]
    fn test_resolve_windows_style_paths() {
        // Test that the resolver works with Windows-style paths
        // This test uses forward slashes but should work on Windows too
        let base_dir = tempdir().unwrap();

        // Create structure with forward slashes (works on both Unix and Windows)
        let project_dir = base_dir.path().join("project/subdir");
        fs::create_dir_all(&project_dir).unwrap();

        let proto_file = project_dir.join("test.proto");
        fs::write(&proto_file, "").unwrap();

        // Test with forward slash path separators
        let import_file = base_dir.path().join("project").join("imported.proto");
        fs::write(&import_file, "").unwrap();

        let resolver = ImportResolver::new(vec![]);
        let resolved = resolver.resolve_import(&proto_file, "project/imported.proto");

        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap(), import_file);
    }

    #[test]
    fn test_additional_dirs_priority() {
        // Test that additional directories have highest priority
        let base_dir = tempdir().unwrap();
        let additional_dir = tempdir().unwrap();

        // Create proto file in base_dir
        let proto_file = base_dir.path().join("test.proto");
        fs::write(&proto_file, "").unwrap();

        // Create import file in relative directory
        let relative_import = base_dir.path().join("imported.proto");
        fs::write(&relative_import, "").unwrap();

        // Create import file in additional directory with same name
        let additional_import = additional_dir.path().join("imported.proto");
        fs::write(&additional_import, "different content").unwrap();

        // Configure resolver with additional directory
        let resolver = ImportResolver::new(vec![additional_dir.path().to_path_buf()]);
        let resolved = resolver.resolve_import(&proto_file, "imported.proto");

        // Should find the file in additional directory first
        assert!(resolved.is_some());
        assert_eq!(resolved.as_ref().unwrap(), &additional_import);
        assert_ne!(resolved.as_ref().unwrap(), &relative_import);
    }
}
