use std::path::{Path, PathBuf};

/// Check if a path corresponds to a supported source file based on its extension.
pub fn is_supported_source_file(path: &Path) -> bool {
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if file_name.starts_with('.') {
            return false;
        }
    } else {
        return false;
    }

    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_ascii_lowercase(),
        None => return false,
    };

    matches!(
        ext.as_str(),
        "rs" | "py" | "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" | "go"
    )
}

/// Recursively scan repository from root, respecting gitignore and hidden file rules,
/// returning a sorted list of relative paths to supported source files.
pub fn scan_repository(root: &Path) -> crate::Result<Vec<PathBuf>> {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true);

    let mut files = Vec::new();

    for entry in builder.build() {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => return Err(crate::RepomapError::Other(err.to_string())),
        };

        let path = entry.path();
        if entry.file_type().is_some_and(|ft| ft.is_file()) && is_supported_source_file(path) {
            let relative = path.strip_prefix(root).unwrap_or(path);
            files.push(relative.to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn create_temp_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("repomap_test_{}_{}", pid, id));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    #[test]
    fn test_scan_filters_unsupported_extensions() {
        let temp = create_temp_dir();

        // Supported files
        File::create(temp.join("lib.rs")).unwrap();
        File::create(temp.join("app.py")).unwrap();
        File::create(temp.join("index.js")).unwrap();
        File::create(temp.join("component.jsx")).unwrap();
        File::create(temp.join("module.mjs")).unwrap();
        File::create(temp.join("common.cjs")).unwrap();
        File::create(temp.join("types.ts")).unwrap();
        File::create(temp.join("view.tsx")).unwrap();
        File::create(temp.join("main.go")).unwrap();

        // Unsupported files
        File::create(temp.join("styles.css")).unwrap();
        File::create(temp.join("index.html")).unwrap();
        File::create(temp.join("data.json")).unwrap();
        File::create(temp.join("README.md")).unwrap();
        File::create(temp.join("binary.bin")).unwrap();

        let files = scan_repository(&temp).expect("scan_repository failed");

        assert_eq!(
            files,
            vec![
                PathBuf::from("app.py"),
                PathBuf::from("common.cjs"),
                PathBuf::from("component.jsx"),
                PathBuf::from("index.js"),
                PathBuf::from("lib.rs"),
                PathBuf::from("main.go"),
                PathBuf::from("module.mjs"),
                PathBuf::from("types.ts"),
                PathBuf::from("view.tsx"),
            ]
        );

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_scan_respects_gitignore() {
        let temp = create_temp_dir();

        // Create a git repo structure or .gitignore
        // ignore crate respects .gitignore by default even without .git directory,
        // but creating .git/ ensures full gitignore semantics
        fs::create_dir_all(temp.join(".git")).unwrap();

        let mut gitignore = File::create(temp.join(".gitignore")).unwrap();
        writeln!(gitignore, "ignored.rs").unwrap();
        writeln!(gitignore, "build/").unwrap();
        writeln!(gitignore, "*.tmp.py").unwrap();

        fs::create_dir_all(temp.join("build")).unwrap();
        File::create(temp.join("build/generated.rs")).unwrap();
        File::create(temp.join("ignored.rs")).unwrap();
        File::create(temp.join("test.tmp.py")).unwrap();
        File::create(temp.join("kept.rs")).unwrap();

        let files = scan_repository(&temp).expect("scan_repository failed");

        assert_eq!(files, vec![PathBuf::from("kept.rs")]);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_scan_skips_hidden_files() {
        let temp = create_temp_dir();

        File::create(temp.join(".hidden.rs")).unwrap();
        File::create(temp.join("visible.rs")).unwrap();

        fs::create_dir_all(temp.join(".hidden_dir")).unwrap();
        File::create(temp.join(".hidden_dir/inner.rs")).unwrap();

        let files = scan_repository(&temp).expect("scan_repository failed");

        assert_eq!(files, vec![PathBuf::from("visible.rs")]);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_is_supported_source_file() {
        assert!(is_supported_source_file(Path::new("src/main.rs")));
        assert!(is_supported_source_file(Path::new("script.py")));
        assert!(is_supported_source_file(Path::new("app.js")));
        assert!(is_supported_source_file(Path::new("ui.jsx")));
        assert!(is_supported_source_file(Path::new("mod.mjs")));
        assert!(is_supported_source_file(Path::new("bundle.cjs")));
        assert!(is_supported_source_file(Path::new("index.ts")));
        assert!(is_supported_source_file(Path::new("page.tsx")));
        assert!(is_supported_source_file(Path::new("server.go")));
        assert!(is_supported_source_file(Path::new("types.mts")));
        assert!(is_supported_source_file(Path::new("types.cts")));

        // Case insensitivity
        assert!(is_supported_source_file(Path::new("MAIN.RS")));
        assert!(is_supported_source_file(Path::new("SCRIPT.PY")));

        // Unsupported
        assert!(!is_supported_source_file(Path::new("file.txt")));
        assert!(!is_supported_source_file(Path::new("file.md")));
        assert!(!is_supported_source_file(Path::new("file.json")));
        assert!(!is_supported_source_file(Path::new("file")));
        assert!(!is_supported_source_file(Path::new(".hidden.rs")));
    }
}
