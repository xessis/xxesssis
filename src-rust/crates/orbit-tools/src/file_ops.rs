use orbit_common::{OrbitError, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

const BINARY_SNIFF_BYTES: usize = 8192;
const MAX_READ_BYTES: usize = 10 * 1024 * 1024; // 10MB limit

pub struct FileReadOutput {
    pub content: String,
    pub total_lines: usize,
    pub line_count_returned: usize,
    pub is_truncated: bool,
}

pub fn read_file_lines(
    path: impl AsRef<Path>,
    offset: usize, // 1-indexed
    limit: usize,
) -> Result<FileReadOutput> {
    let path = path.as_ref();
    crate::security::assert_no_traversal(path)?;

    let metadata = fs::metadata(path)?;

    if metadata.len() > MAX_READ_BYTES as u64 {
        return Err(OrbitError::ToolExecution(format!(
            "File size ({} bytes) exceeds maximum allowable limit of {} bytes",
            metadata.len(),
            MAX_READ_BYTES
        )));
    }

    let bytes = fs::read(path)?;

    // Sniff for binary
    let sniff_len = bytes.len().min(BINARY_SNIFF_BYTES);
    if bytes[..sniff_len].contains(&0) {
        return Err(OrbitError::ToolExecution(
            "File appears to be binary (contains NUL bytes)".to_string(),
        ));
    }

    let text = std::str::from_utf8(&bytes)
        .map_err(|e| OrbitError::ToolExecution(format!("Invalid UTF-8: {}", e)))?;

    let lines: Vec<&str> = text.lines().collect();
    let total_lines = lines.len();

    let start_idx = offset.saturating_sub(1).min(total_lines);
    let end_idx = (start_idx + limit).min(total_lines);

    let selected_lines = &lines[start_idx..end_idx];
    let is_truncated = end_idx < total_lines;

    let mut content = String::new();
    for (idx, line) in selected_lines.iter().enumerate() {
        let line_num = start_idx + idx + 1;
        content.push_str(&format!("{:>4}| {}\n", line_num, line));
    }

    Ok(FileReadOutput {
        content,
        total_lines,
        line_count_returned: selected_lines.len(),
        is_truncated,
    })
}

pub fn atomic_write_file(path: impl AsRef<Path>, content: &str) -> Result<()> {
    let path = path.as_ref();
    crate::security::assert_no_traversal(path)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
    {
        let mut file = File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }

    if let Err(e) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(e.into());
    }
    Ok(())
}
