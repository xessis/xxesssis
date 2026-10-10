use ignore::WalkBuilder;
use orbit_common::Result;
use std::path::{Path, PathBuf};

pub fn glob_files(root: impl AsRef<Path>, max_results: usize) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(true)
        .parents(true)
        .git_ignore(true)
        .build();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().is_some_and(|ft| ft.is_file()) {
            files.push(entry.path().to_path_buf());
            if files.len() >= max_results {
                break;
            }
        }
    }

    Ok(files)
}
